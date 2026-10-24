use proxmox_router::{ApiMethod, Permission, Router, SubdirMap};

use pdm_api_types::PRIV_SYS_AUDIT;

const API_METHOD_GET_ACCESS_LOG_WITH_ACCESS: ApiMethod =
    proxmox_access_log::API_METHOD_GET_ACCESS_LOG.access(
        None,
        &Permission::Privilege(&["system", "log"], PRIV_SYS_AUDIT, false),
    );

const API_METHOD_GET_ACCESS_LOG_STATS_WITH_ACCESS: ApiMethod =
    proxmox_access_log::API_METHOD_GET_ACCESS_LOG_STATS.access(
        None,
        &Permission::Privilege(&["system", "log"], PRIV_SYS_AUDIT, false),
    );

const SUBDIRS: SubdirMap = &[(
    "stats",
    &Router::new().get(&API_METHOD_GET_ACCESS_LOG_STATS_WITH_ACCESS),
)];

pub const ROUTER: Router = Router::new()
    .get(&API_METHOD_GET_ACCESS_LOG_WITH_ACCESS)
    .subdirs(SUBDIRS);

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use http::Method;
    use proxmox_router::{ApiMethod, Permission};
    use proxmox_schema::ObjectSchemaType;

    use pdm_api_types::PRIV_SYS_AUDIT;

    fn routed(path: &[&str], method: Method) -> Option<&'static ApiMethod> {
        crate::api::ROUTER.find_method(path, method, &mut HashMap::new())
    }

    #[test]
    fn auditors_of_the_system_log_may_read_the_access_log() {
        for (path, shared) in [
            (
                &["nodes", "localhost", "access-log"][..],
                &proxmox_access_log::API_METHOD_GET_ACCESS_LOG,
            ),
            (
                &["nodes", "localhost", "access-log", "stats"][..],
                &proxmox_access_log::API_METHOD_GET_ACCESS_LOG_STATS,
            ),
        ] {
            let method = routed(path, Method::GET).expect("routed");
            assert_eq!(
                method.parameters.description(),
                shared.parameters.description()
            );
            let Permission::Privilege(acl_path, privs, partial) = method.access.permission else {
                panic!("not the system log audit privilege");
            };
            assert_eq!(
                (*acl_path, *privs, *partial),
                (&["system", "log"][..], PRIV_SYS_AUDIT, false)
            );
            // the public API daemon reads the log
            assert!(!method.protected);
            assert!(routed(path, Method::POST).is_none());
        }
    }
}
