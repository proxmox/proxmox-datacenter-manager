use std::collections::HashMap;
use std::pin::pin;

use anyhow::Error;

use pbs_api_types::{MetricDataPoint, MetricDataType};
use proxmox_rrd::rrd::DataSourceType;

use pdm_api_types::{
    remotes::RemoteType,
    resource::{Resource, ResourceRrdData, TopEntities, TopEntity},
};
use pve_api_types::{ClusterMetricsData, ClusterMetricsDataType};

use crate::{connection, task_utils};

pub mod rrd_cache;

const COLLECTION_INTERVAL: u64 = 60;

/// Initialize the RRD cache
pub fn init() -> Result<(), Error> {
    rrd_cache::init()?;

    Ok(())
}

/// Start the metric collection task.
pub fn start_task() {
    tokio::spawn(async move {
        let task_scheduler = pin!(metric_collection_task());
        let abort_future = pin!(proxmox_daemon::shutdown_future());
        futures::future::select(task_scheduler, abort_future).await;
    });
}

async fn metric_collection_task() -> Result<(), Error> {
    let mut most_recent_timestamps: HashMap<String, i64> = HashMap::new();

    loop {
        let delay_target = task_utils::next_aligned_instant(COLLECTION_INTERVAL);
        tokio::time::sleep_until(tokio::time::Instant::from_std(delay_target)).await;

        let remotes = match pdm_config::remotes::config() {
            Ok((remotes, _)) => remotes,
            Err(e) => {
                log::error!("failed to collect metrics, could not read remotes.cfg: {e}");
                continue;
            }
        };

        for (remote_name, remote) in &remotes.sections {
            let start_time = *most_recent_timestamps.get(remote_name).unwrap_or(&0);
            let remote_name_clone = remote_name.clone();

            let res = async {
                let most_recent_timestamp = match remote.ty {
                    RemoteType::Pve => {
                        let client = connection::make_pve_client(remote)?;
                        let metrics = client
                            .cluster_metrics_export(Some(true), Some(false), Some(start_time))
                            .await?;

                        //// Involves some blocking file IO
                        tokio::task::spawn_blocking(move || {
                            let mut most_recent_timestamp = 0;

                            for data_point in metrics.data {
                                most_recent_timestamp =
                                    most_recent_timestamp.max(data_point.timestamp);
                                store_metric_pve(&remote_name_clone, &data_point);
                            }

                            most_recent_timestamp
                        })
                        .await
                    }
                    RemoteType::Pbs => {
                        let client = connection::make_pbs_client(remote)?;
                        let metrics = client.metrics(Some(true), Some(start_time)).await?;

                        // Involves some blocking file IO
                        tokio::task::spawn_blocking(move || {
                            let mut most_recent_timestamp = 0;

                            for data_point in metrics.data {
                                most_recent_timestamp =
                                    most_recent_timestamp.max(data_point.timestamp);
                                store_metric_pbs(&remote_name_clone, &data_point);
                            }

                            most_recent_timestamp
                        })
                        .await
                    }
                }?;

                Ok::<i64, Error>(most_recent_timestamp)
            }
            .await;

            match res {
                Ok(ts) => {
                    most_recent_timestamps.insert(remote_name.to_string(), ts);
                }
                Err(err) => log::error!("failed to collect metrics for {remote_name}: {err}"),
            }
        }
    }
}

fn store_metric_pve(remote_name: &str, data_point: &ClusterMetricsData) {
    let name = format!(
        "pve/{remote_name}/{id}/{metric}",
        id = data_point.id,
        metric = data_point.metric,
    );

    let data_source_type = match data_point.ty {
        ClusterMetricsDataType::Gauge => DataSourceType::Gauge,
        ClusterMetricsDataType::Counter => DataSourceType::Counter,
        ClusterMetricsDataType::Derive => DataSourceType::Derive,
    };

    rrd_cache::update_value(
        &name,
        data_point.value,
        data_point.timestamp,
        data_source_type,
    );
}

fn store_metric_pbs(remote_name: &str, data_point: &MetricDataPoint) {
    let name = format!(
        "pbs/{remote_name}/{id}/{metric}",
        id = data_point.id,
        metric = data_point.metric,
    );

    let data_source_type = match data_point.ty {
        MetricDataType::Gauge => DataSourceType::Gauge,
        MetricDataType::Counter => DataSourceType::Counter,
        MetricDataType::Derive => DataSourceType::Derive,
    };

    rrd_cache::update_value(
        &name,
        data_point.value,
        data_point.timestamp,
        data_source_type,
    );
}

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
) -> TopEntities {
    let mut guest_cpu = Vec::new();
    let mut node_cpu = Vec::new();
    let mut node_memory = Vec::new();

    for remote_name in remotes.keys() {
        if let Some(data) =
            crate::api::resources::get_cached_resources(remote_name, i64::MAX as u64)
        {
            for res in data.resources {
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
                    Resource::PveNode(_) => {
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
                    Resource::PbsNode(_) => {}
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
    if let Ok(Some(values)) = rrd_cache::extract_data(
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
