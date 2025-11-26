use anyhow::Error;
use serde::{Deserialize, Serialize};

use proxmox_access_control::CachedUserInfo;
use proxmox_config_digest::ConfigDigest;
use proxmox_router::{http_bail, http_err, Permission, Router, RpcEnvironment};
use proxmox_schema::{api, param_bail};

use pdm_api_types::{
    views::{ViewConfig, ViewConfigEntry, ViewConfigUpdater, ViewTemplate},
    PRIV_RESOURCE_AUDIT, PRIV_RESOURCE_MODIFY,
};

const VIEW_ROUTER: Router = Router::new()
    .put(&API_METHOD_UPDATE_VIEW)
    .delete(&API_METHOD_REMOVE_VIEW)
    .get(&API_METHOD_READ_VIEW);

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_VIEWS)
    .post(&API_METHOD_ADD_VIEW)
    .match_all("id", &VIEW_ROUTER);

#[api(
    protected: true,
    access: {
        permission: &Permission::Anybody,
        description: "Returns the views the user has access to.",
    },
    returns: {
        description: "List of views.",
        type: Array,
        items: {
            type: String,
            description: "The name of a view."
        },
    },
)]
/// List views.
pub fn get_views(rpcenv: &mut dyn RpcEnvironment) -> Result<Vec<ViewConfig>, Error> {
    let (config, _) = pdm_config::views::config()?;

    let user_info = CachedUserInfo::new()?;
    let auth_id = rpcenv.get_auth_id().unwrap().parse()?;
    let top_level_allowed = user_info
        .check_privs(&auth_id, &["view"], PRIV_RESOURCE_AUDIT, false)
        .is_ok();

    let views: Vec<ViewConfig> = config
        .into_iter()
        .filter_map(|(view, value)| {
            if !top_level_allowed
                && user_info
                    .check_privs(&auth_id, &["view", &view], PRIV_RESOURCE_AUDIT, false)
                    .is_err()
            {
                return None;
            };
            match value {
                ViewConfigEntry::View(conf) => Some(conf),
            }
        })
        .collect();

    Ok(views)
}

#[api(
    protected: true,
    input: {
        properties: {
            view: {
                flatten: true,
                type: ViewConfig,
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["view"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Add new view.
pub fn add_view(view: ViewConfig, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = pdm_config::views::lock_config()?;

    let (mut config, config_digest) = pdm_config::views::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    let id = view.id.clone();

    if !view.layout.is_empty() {
        if let Err(err) = serde_json::from_str::<ViewTemplate>(&view.layout) {
            param_bail!("layout", "layout is not valid: '{}'", err)
        }
    }

    if let Some(ViewConfigEntry::View(_)) = config.insert(id.clone(), ViewConfigEntry::View(view)) {
        param_bail!("id", "view '{}' already exists.", id)
    }

    pdm_config::views::save_config(&config)?;

    Ok(())
}

#[api()]
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// Deletable property name
pub enum DeletableProperty {
    /// Delete the include filters.
    Include,
    /// Delete the exclude filters.
    Exclude,
    /// Delete the layout.
    Layout,
    /// Delete include-all flag
    IncludeAll,
}

#[api(
    protected: true,
    input: {
        properties: {
            id: {
                type: String,
                description: "",
            },
            view: {
                flatten: true,
                type: ViewConfigUpdater,
            },
            delete: {
                description: "List of properties to delete.",
                type: Array,
                optional: true,
                items: {
                    type: DeletableProperty,
                }
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["view", "{id}"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Update View.
pub fn update_view(
    id: String,
    view: ViewConfigUpdater,
    delete: Option<Vec<DeletableProperty>>,
    digest: Option<ConfigDigest>,
) -> Result<(), Error> {
    let _lock = pdm_config::views::lock_config()?;

    let (mut config, config_digest) = pdm_config::views::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    let entry = config
        .get_mut(&id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such remote {id}"))?;

    let ViewConfigEntry::View(conf) = entry;

    if let Some(delete) = delete {
        for delete_prop in delete {
            match delete_prop {
                DeletableProperty::Include => conf.include = Vec::new(),
                DeletableProperty::Exclude => conf.exclude = Vec::new(),
                DeletableProperty::Layout => conf.layout = String::new(),
                DeletableProperty::IncludeAll => conf.include_all = None,
            }
        }
    }

    if let Some(include) = view.include {
        conf.include = include;
    }

    if let Some(exclude) = view.exclude {
        conf.exclude = exclude;
    }

    if view.include_all.is_some() {
        conf.include_all = view.include_all;
    }

    if let Some(layout) = view.layout {
        if !layout.is_empty() {
            if let Err(err) = serde_json::from_str::<ViewTemplate>(&layout) {
                param_bail!("layout", "layout is not valid: '{}'", err)
            }
        }
        conf.layout = layout;
    }

    pdm_config::views::save_config(&config)?;

    Ok(())
}

#[api(
    protected: true,
    input: {
        properties: {
            id: {
                type: String,
                description: "",
            },
            digest: {
                type: ConfigDigest,
                optional: true,
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["view"], PRIV_RESOURCE_MODIFY, false),
    },
)]
/// Delete the view with the given id.
pub fn remove_view(id: String, digest: Option<ConfigDigest>) -> Result<(), Error> {
    let _lock = pdm_config::views::lock_config()?;

    let (mut config, config_digest) = pdm_config::views::config()?;

    config_digest.detect_modification(digest.as_ref())?;

    match config.remove(&id) {
        Some(ViewConfigEntry::View(_)) => {}
        None => http_bail!(NOT_FOUND, "view '{id}' does not exist."),
    }

    pdm_config::views::save_config(&config)?;

    Ok(())
}

#[api(
    input: {
        properties: {
            id: {
                type: String,
                description: "",
            },
        },
    },
    access: {
        permission: &Permission::Privilege(&["view", "{id}"], PRIV_RESOURCE_AUDIT, false),
    },
)]
/// Get the config of a single view.
pub fn read_view(id: String) -> Result<ViewConfig, Error> {
    let (config, _) = pdm_config::views::config()?;

    let view = config
        .get(&id)
        .ok_or_else(|| http_err!(NOT_FOUND, "no such view '{id}'"))?;

    let view = match view {
        ViewConfigEntry::View(view) => view.clone(),
    };

    Ok(view)
}
