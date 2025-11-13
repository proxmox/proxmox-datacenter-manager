use std::collections::HashMap;

use pdm_api_types::resource::{Resource, ResourceRrdData, TopEntities, TopEntity};

use super::rrd_cache;

fn insert_sorted<T>(vec: &mut Vec<(usize, T)>, value: (usize, T), limit: usize) {
    let index = match vec.binary_search_by_key(&value.0, |(idx, _)| *idx) {
        Ok(idx) | Err(idx) => idx,
    };

    vec.insert(index, value);
    if vec.len() > limit {
        for _ in 0..(vec.len() - limit) {
            vec.remove(0);
        }
    }
}

// for now simple sum of the values => area under the graph curve
fn calculate_coefficient(values: &proxmox_rrd::Entry) -> f64 {
    let mut coefficient = 0.0;
    for point in values.data.iter() {
        let value = point.unwrap_or_default();
        if value.is_finite() {
            coefficient += value;
        }
    }

    coefficient
}

// FIXME: cache the values instead of calculate freshly every time?
// FIXME: find better way to enumerate nodes/guests/etc.(instead of relying on the cache)
pub fn calculate_top(
    remotes: &HashMap<String, pdm_api_types::remotes::Remote>,
    timeframe: proxmox_rrd_api_types::RrdTimeframe,
    num: usize,
    check_remote_privs: impl Fn(&str) -> bool,
    is_resource_included: impl Fn(&str, &Resource) -> bool,
) -> TopEntities {
    let mut guest_cpu = Vec::new();
    let mut node_cpu = Vec::new();
    let mut node_memory = Vec::new();

    for (remote_name, remote) in remotes {
        if !check_remote_privs(remote_name) {
            continue;
        }

        if let Some(data) =
            crate::api::resources::get_cached_resources(remote_name, i64::MAX as u64)
        {
            for res in data.resources {
                if !is_resource_included(remote_name, &res) {
                    continue;
                }

                let id = res.id().to_string();
                let name = format!("pve/{remote_name}/{id}");
                match &res {
                    Resource::PveStorage(_) => {}
                    Resource::PveQemu(_) | Resource::PveLxc(_) => {
                        if let Some(entity) =
                            get_entity(timeframe, remote_name, res, name, "cpu_current")
                        {
                            let coefficient = (entity.0 * 100.0).round() as usize;
                            insert_sorted(&mut guest_cpu, (coefficient, entity.1), num);
                        }
                    }
                    Resource::PveNode(_) | Resource::PbsNode(_) => {
                        // pbs node datapoints are always saved with 'host' instead of nodename
                        let name = if remote.ty == pdm_api_types::remotes::RemoteType::Pbs {
                            format!("pbs/{remote_name}/host")
                        } else {
                            name
                        };
                        if let Some(entity) = get_entity(
                            timeframe,
                            remote_name,
                            res.clone(),
                            name.clone(),
                            "cpu_current",
                        ) {
                            let coefficient = (entity.0 * 100.0).round() as usize;
                            insert_sorted(&mut node_cpu, (coefficient, entity.1), num);
                        }
                        // convert mem/mem_total into a single entity
                        if let Some(mut mem) = get_entity(
                            timeframe,
                            remote_name,
                            res.clone(),
                            name.clone(),
                            "mem_used",
                        ) {
                            if let Some(mem_total) =
                                get_entity(timeframe, remote_name, res, name, "mem_total")
                            {
                                // skip if we don't have the same amount of data for used and total
                                let mem_rrd = &mem.1.rrd_data.data;
                                let mem_total_rrd = &mem_total.1.rrd_data.data;
                                if mem_rrd.len() != mem_total_rrd.len() {
                                    continue;
                                }
                                let coefficient = (100.0 * mem.0 / mem_total.0).round() as usize;
                                let mut mem_usage = Vec::new();
                                for i in 0..mem_rrd.len() {
                                    let point = match (mem_rrd[i], mem_total_rrd[i]) {
                                        (Some(mem), Some(total)) => Some(mem / total),
                                        _ => None,
                                    };
                                    mem_usage.push(point)
                                }
                                mem.1.rrd_data.data = mem_usage;
                                insert_sorted(&mut node_memory, (coefficient, mem.1), num);
                            }
                        }
                    }
                    Resource::PveNetwork(_) => {}
                    Resource::PbsDatastore(_) => {}
                }
            }
        }
    }

    TopEntities {
        guest_cpu: guest_cpu.into_iter().map(|(_, entity)| entity).collect(),
        node_cpu: node_cpu.into_iter().map(|(_, entity)| entity).collect(),
        node_memory: node_memory.into_iter().map(|(_, entity)| entity).collect(),
    }
}

fn get_entity(
    timeframe: proxmox_rrd_api_types::RrdTimeframe,
    remote_name: &String,
    res: Resource,
    name: String,
    metric: &str,
) -> Option<(f64, TopEntity)> {
    let cache = rrd_cache::get_cache();

    if let Ok(Some(values)) = cache.extract_data(
        &name,
        metric,
        timeframe,
        proxmox_rrd_api_types::RrdMode::Average,
    ) {
        let coefficient = calculate_coefficient(&values);
        if coefficient > 0.0 {
            return Some((
                coefficient,
                TopEntity {
                    remote: remote_name.to_string(),
                    resource: res,
                    rrd_data: ResourceRrdData {
                        start: values.start,
                        resolution: values.resolution,
                        data: values.data,
                    },
                },
            ));
        }
    }

    None
}
