use anyhow::{format_err, Error};

use pdm_api_types::{
    resource::{Resource, ResourceType},
    views::{FilterRule, ViewConfig, ViewConfigEntry},
};

#[cfg(test)]
mod tests;

/// Get view with a given ID.
///
/// Returns an error if the view configuration file could not be read, or
/// if the view with the provided ID does not exist.
pub fn get_view(view_id: &str) -> Result<View, Error> {
    let config = pdm_config::views::config()?;

    let entry = config
        .get(view_id)
        .cloned()
        .ok_or_else(|| format_err!("unknown view: {view_id}"))?;

    match entry {
        ViewConfigEntry::View(view_config) => Ok(View::new(view_config)),
    }
}

/// Get (optional) view with a given ID.
///
/// Returns an error if the view configuration file could not be read, or
/// if the view with the provided ID does not exist.
pub fn get_optional_view(view_id: Option<&str>) -> Result<Option<View>, Error> {
    view_id.map(get_view).transpose()
}

/// View implementation.
///
/// Given a [`ViewConfig`], this struct can be used to check if a resource/remote/node
/// matches the include/exclude rules.
#[derive(Clone)]
pub struct View {
    config: ViewConfig,
}

impl View {
    /// Create a new [`View`].
    pub fn new(config: ViewConfig) -> Self {
        Self { config }
    }

    /// Check if a [`Resource`] matches the filter rules.
    pub fn resource_matches(&self, remote: &str, resource: &Resource) -> bool {
        // NOTE: Establishing a cache here is not worth the effort at the moment, evaluation of
        // rules is *very* fast.

        let resource_data = resource.into();

        self.check_if_included(remote, &resource_data)
            && !self.check_if_excluded(remote, &resource_data)
    }

    /// Check if a remote can be safely skipped based on the filter rule definition.
    ///
    /// When there are `include remote:<...>` or `exclude remote:<...>` rules, we can use these to
    /// check if a remote needs to be considered at all.
    pub fn can_skip_remote(&self, remote: &str) -> bool {
        let matches_any_exclude_remote = self
            .config
            .exclude
            .iter()
            .any(|rule| Self::matches_remote_rule(remote, rule));

        if matches_any_exclude_remote {
            return true;
        }

        if self.config.include_all.unwrap_or_default() {
            return false;
        }

        for include in &self.config.include {
            if let FilterRule::Remote(r) = include {
                if r.matches(remote) {
                    return false;
                }
            } else {
                // If there is any other type of rule, we cannot safely infer whether we can skip
                // the remote (e.g. for 'tag' matches, we have to check *all* remotes for resources
                // with a given tag)
                return false;
            }
        }

        true
    }

    /// Check if a remote is *explicitly* included (and not excluded).
    ///
    /// A subset of the resources of a remote might still be pulled in by other rules,
    /// but this function check if the remote as a whole is matched.
    pub fn is_remote_explicitly_included(&self, remote: &str) -> bool {
        let included = if self.config.include_all.unwrap_or_default() {
            true
        } else {
            self.config
                .include
                .iter()
                .any(|rule| Self::matches_remote_rule(remote, rule))
        };

        let matches_exclude_remote = self
            .config
            .exclude
            .iter()
            .any(|rule| Self::matches_remote_rule(remote, rule));

        included && !matches_exclude_remote
    }

    /// Check if a node is matched by the filter rules.
    ///
    /// This is equivalent to checking an actual node resource.
    pub fn is_node_included(&self, remote: &str, node: &str) -> bool {
        let resource_data = ResourceData {
            resource_type: ResourceType::Node,
            tags: None,
            resource_pool: None,
            resource_id: &format!("remote/{remote}/node/{node}"),
        };

        self.check_if_included(remote, &resource_data)
            && !self.check_if_excluded(remote, &resource_data)
    }

    /// Returns the name of the view.
    pub fn name(&self) -> &str {
        &self.config.id
    }

    fn check_if_included(&self, remote: &str, resource: &ResourceData) -> bool {
        if self.config.include_all.unwrap_or_default() {
            return true;
        }

        check_rules(&self.config.include, remote, resource)
    }

    fn check_if_excluded(&self, remote: &str, resource: &ResourceData) -> bool {
        check_rules(&self.config.exclude, remote, resource)
    }

    fn matches_remote_rule(remote: &str, rule: &FilterRule) -> bool {
        if let FilterRule::Remote(r) = rule {
            r.matches(remote)
        } else {
            false
        }
    }
}

fn check_rules(rules: &[FilterRule], remote: &str, resource: &ResourceData) -> bool {
    rules.iter().any(|rule| match rule {
        FilterRule::ResourceType(resource_type) => resource_type.matches(&resource.resource_type),
        FilterRule::ResourcePool(pool) => {
            if let Some(resource_pool) = resource.resource_pool {
                pool.matches(resource_pool)
            } else {
                false
            }
        }
        FilterRule::ResourceId(resource_id) => resource_id.matches(resource.resource_id),
        FilterRule::Tag(tag) => {
            if let Some(resource_tags) = resource.tags {
                resource_tags.iter().any(|t| tag.matches(t))
            } else {
                false
            }
        }
        FilterRule::Remote(included_remote) => included_remote.matches(remote),
    })
}

struct ResourceData<'a> {
    resource_type: ResourceType,
    tags: Option<&'a [String]>,
    resource_pool: Option<&'a String>,
    resource_id: &'a str,
}

impl<'a> From<&'a Resource> for ResourceData<'a> {
    fn from(value: &'a Resource) -> Self {
        match value {
            Resource::PveQemu(resource) => ResourceData {
                resource_type: value.resource_type(),
                tags: Some(&resource.tags),
                resource_pool: Some(&resource.pool),
                resource_id: value.global_id(),
            },
            Resource::PveLxc(resource) => ResourceData {
                resource_type: value.resource_type(),
                tags: Some(&resource.tags),
                resource_pool: Some(&resource.pool),
                resource_id: value.global_id(),
            },
            Resource::PveNode(_)
            | Resource::PveNetwork(_)
            | Resource::PbsNode(_)
            | Resource::PbsDatastore(_)
            | Resource::PveStorage(_) => ResourceData {
                resource_type: value.resource_type(),
                tags: None,
                resource_pool: None,
                resource_id: value.global_id(),
            },
        }
    }
}
