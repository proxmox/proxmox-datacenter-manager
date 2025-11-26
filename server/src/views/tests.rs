use pdm_api_types::{
    resource::{PveLxcResource, PveQemuResource, PveStorageResource, Resource, ResourceType},
    views::{EnumMatcher, FilterRule, StringMatcher, ViewConfig},
};

use super::View;

fn make_storage_resource(remote: &str, node: &str, storage_name: &str) -> Resource {
    Resource::PveStorage(PveStorageResource {
        disk: 1000,
        maxdisk: 2000,
        id: format!("remote/{remote}/storage/{node}/{storage_name}"),
        storage: storage_name.into(),
        node: node.into(),
        status: "available".into(),
    })
}

fn make_qemu_resource(
    remote: &str,
    node: &str,
    vmid: u32,
    pool: Option<&str>,
    tags: &[&str],
) -> Resource {
    Resource::PveQemu(PveQemuResource {
        disk: 1000,
        maxdisk: 2000,
        id: format!("remote/{remote}/guest/{vmid}"),
        node: node.into(),
        status: "available".into(),
        cpu: 0.0,
        maxcpu: 0.0,
        maxmem: 1024,
        mem: 512,
        name: format!("vm-{vmid}"),
        // TODO: Check the API type - i guess it should be an option?
        pool: pool.map_or_else(String::new, |a| a.into()),
        tags: tags.iter().map(|tag| String::from(*tag)).collect(),
        template: false,
        uptime: 1337,
        vmid,
    })
}

fn make_lxc_resource(
    remote: &str,
    node: &str,
    vmid: u32,
    pool: Option<&str>,
    tags: &[&str],
) -> Resource {
    Resource::PveLxc(PveLxcResource {
        disk: 1000,
        maxdisk: 2000,
        id: format!("remote/{remote}/guest/{vmid}"),
        node: node.into(),
        status: "available".into(),
        cpu: 0.0,
        maxcpu: 0.0,
        maxmem: 1024,
        mem: 512,
        name: format!("vm-{vmid}"),
        // TODO: Check the API type - i guess it should be an option?
        pool: pool.map_or_else(String::new, |a| a.into()),
        tags: tags.iter().map(|tag| String::from(*tag)).collect(),
        template: false,
        uptime: 1337,
        vmid,
    })
}

fn run_test(config: ViewConfig, tests: &[((&str, &Resource), bool)]) {
    let filter = View::new(config);

    for ((remote_name, resource), expected) in tests {
        eprintln!("remote: {remote_name}, resource: {resource:?}");
        assert_eq!(filter.resource_matches(remote_name, resource), *expected);
    }
}

const NODE: &str = "somenode";
const STORAGE: &str = "somestorage";
const REMOTE: &str = "someremote";

#[test]
fn include_remotes() {
    let config = ViewConfig {
        id: "only-includes".into(),
        include: vec![
            FilterRule::Remote(StringMatcher::Exact("remote-a".into())),
            FilterRule::Remote(StringMatcher::Exact("remote-b".into())),
        ],
        ..Default::default()
    };
    run_test(
        config.clone(),
        &[
            (
                (
                    "remote-a",
                    &make_storage_resource("remote-a", NODE, STORAGE),
                ),
                true,
            ),
            (
                (
                    "remote-b",
                    &make_storage_resource("remote-b", NODE, STORAGE),
                ),
                true,
            ),
            (
                (
                    "remote-c",
                    &make_storage_resource("remote-c", NODE, STORAGE),
                ),
                false,
            ),
        ],
    );

    let view = View::new(config);

    assert!(!view.can_skip_remote("remote-a"));
    assert!(!view.can_skip_remote("remote-b"));
    assert!(view.can_skip_remote("remote-c"));
}

#[test]
fn exclude_remotes() {
    let config = ViewConfig {
        id: "only-excludes".into(),
        exclude: vec![
            FilterRule::Remote(StringMatcher::Exact("remote-a".into())),
            FilterRule::Remote(StringMatcher::Exact("remote-b".into())),
        ],
        include_all: Some(true),
        ..Default::default()
    };

    run_test(
        config.clone(),
        &[
            (
                (
                    "remote-a",
                    &make_storage_resource("remote-a", NODE, STORAGE),
                ),
                false,
            ),
            (
                (
                    "remote-b",
                    &make_storage_resource("remote-b", NODE, STORAGE),
                ),
                false,
            ),
            (
                (
                    "remote-c",
                    &make_storage_resource("remote-c", NODE, STORAGE),
                ),
                true,
            ),
        ],
    );

    let view = View::new(config);

    assert!(view.can_skip_remote("remote-a"));
    assert!(view.can_skip_remote("remote-b"));
    assert!(!view.can_skip_remote("remote-c"));
}

#[test]
fn include_exclude_remotes() {
    let config = ViewConfig {
        id: "both".into(),
        include: vec![
            FilterRule::Remote(StringMatcher::Exact("remote-a".into())),
            FilterRule::Remote(StringMatcher::Exact("remote-b".into())),
        ],
        exclude: vec![
            FilterRule::Remote(StringMatcher::Exact("remote-b".into())),
            FilterRule::Remote(StringMatcher::Exact("remote-c".into())),
        ],

        ..Default::default()
    };
    run_test(
        config.clone(),
        &[
            (
                (
                    "remote-a",
                    &make_storage_resource("remote-a", NODE, STORAGE),
                ),
                true,
            ),
            (
                (
                    "remote-b",
                    &make_storage_resource("remote-b", NODE, STORAGE),
                ),
                false,
            ),
            (
                (
                    "remote-c",
                    &make_storage_resource("remote-c", NODE, STORAGE),
                ),
                false,
            ),
        ],
    );

    let view = View::new(config);

    assert!(!view.can_skip_remote("remote-a"));
    assert!(view.can_skip_remote("remote-b"));
    assert!(view.can_skip_remote("remote-c"));
    assert!(view.can_skip_remote("remote-d"));
}

#[test]
fn empty_config() {
    let config = ViewConfig {
        id: "empty".into(),
        include_all: Some(true),
        ..Default::default()
    };
    run_test(
        config.clone(),
        &[
            (
                (
                    "remote-a",
                    &make_storage_resource("remote-a", NODE, STORAGE),
                ),
                true,
            ),
            (
                (
                    "remote-b",
                    &make_storage_resource("remote-b", NODE, STORAGE),
                ),
                true,
            ),
            (
                (
                    "remote-c",
                    &make_storage_resource("remote-c", NODE, STORAGE),
                ),
                true,
            ),
            (
                (REMOTE, &make_qemu_resource(REMOTE, NODE, 100, None, &[])),
                true,
            ),
        ],
    );

    let view = View::new(config);

    assert!(!view.can_skip_remote("remote-a"));
    assert!(!view.can_skip_remote("remote-b"));
    assert!(!view.can_skip_remote("remote-c"));
}

#[test]
fn include_type() {
    run_test(
        ViewConfig {
            id: "include-resource-type".into(),
            include: vec![
                FilterRule::ResourceType(EnumMatcher(ResourceType::PveStorage)),
                FilterRule::ResourceType(EnumMatcher(ResourceType::PveQemu)),
            ],
            ..Default::default()
        },
        &[
            (
                (REMOTE, &make_storage_resource(REMOTE, NODE, STORAGE)),
                true,
            ),
            (
                (REMOTE, &make_qemu_resource(REMOTE, NODE, 100, None, &[])),
                true,
            ),
            (
                (REMOTE, &make_lxc_resource(REMOTE, NODE, 101, None, &[])),
                false,
            ),
        ],
    );
}

#[test]
fn exclude_type() {
    run_test(
        ViewConfig {
            id: "exclude-resource-type".into(),
            exclude: vec![
                FilterRule::ResourceType(EnumMatcher(ResourceType::PveStorage)),
                FilterRule::ResourceType(EnumMatcher(ResourceType::PveQemu)),
            ],
            include_all: Some(true),
            ..Default::default()
        },
        &[
            (
                (REMOTE, &make_storage_resource(REMOTE, NODE, STORAGE)),
                false,
            ),
            (
                (REMOTE, &make_qemu_resource(REMOTE, NODE, 100, None, &[])),
                false,
            ),
            (
                (REMOTE, &make_lxc_resource(REMOTE, NODE, 101, None, &[])),
                true,
            ),
        ],
    );
}

#[test]
fn include_exclude_type() {
    run_test(
        ViewConfig {
            id: "exclude-resource-type".into(),
            include: vec![FilterRule::ResourceType(EnumMatcher(ResourceType::PveQemu))],
            exclude: vec![FilterRule::ResourceType(EnumMatcher(
                ResourceType::PveStorage,
            ))],
            ..Default::default()
        },
        &[
            (
                (REMOTE, &make_storage_resource(REMOTE, NODE, STORAGE)),
                false,
            ),
            (
                (REMOTE, &make_qemu_resource(REMOTE, NODE, 100, None, &[])),
                true,
            ),
            (
                (REMOTE, &make_lxc_resource(REMOTE, NODE, 101, None, &[])),
                false,
            ),
        ],
    );
}

#[test]
fn include_exclude_tags() {
    run_test(
        ViewConfig {
            id: "include-tags".into(),
            include: vec![
                FilterRule::Tag(StringMatcher::Exact("tag1".to_string())),
                FilterRule::Tag(StringMatcher::Exact("tag2".to_string())),
            ],
            exclude: vec![FilterRule::Tag(StringMatcher::Exact("tag3".to_string()))],
            ..Default::default()
        },
        &[
            (
                (REMOTE, &make_storage_resource(REMOTE, NODE, STORAGE)),
                // only qemu/lxc can match tags for now
                false,
            ),
            (
                (
                    REMOTE,
                    &make_qemu_resource(REMOTE, NODE, 100, None, &["tag1", "tag3"]),
                ),
                // because tag3 is excluded
                false,
            ),
            (
                (
                    REMOTE,
                    &make_lxc_resource(REMOTE, NODE, 101, None, &["tag1"]),
                ),
                // matches since it's in the includes
                true,
            ),
            (
                (
                    REMOTE,
                    &make_lxc_resource(REMOTE, NODE, 102, None, &["tag4"]),
                ),
                // Not in includes, can never match
                false,
            ),
        ],
    );
}

#[test]
fn include_exclude_resource_pool() {
    run_test(
        ViewConfig {
            id: "pools".into(),
            include: vec![
                FilterRule::ResourcePool(StringMatcher::Exact("pool1".to_string())),
                FilterRule::ResourcePool(StringMatcher::Exact("pool2".to_string())),
            ],
            exclude: vec![FilterRule::ResourcePool(StringMatcher::Exact(
                "pool2".to_string(),
            ))],
            ..Default::default()
        },
        &[
            (
                (REMOTE, &make_storage_resource(REMOTE, NODE, STORAGE)),
                // only qemu/lxc can match pools for now
                false,
            ),
            (
                (
                    REMOTE,
                    &make_qemu_resource(REMOTE, NODE, 100, Some("pool2"), &[]),
                ),
                // because pool2 is excluded (takes precedence over includes)
                false,
            ),
            (
                (
                    REMOTE,
                    &make_lxc_resource(REMOTE, NODE, 101, Some("pool1"), &[]),
                ),
                // matches since it's in the includes
                true,
            ),
            (
                (
                    REMOTE,
                    &make_lxc_resource(REMOTE, NODE, 102, Some("pool4"), &[]),
                ),
                // Not in includes, can never match
                false,
            ),
        ],
    );
}

#[test]
fn include_exclude_resource_id() {
    run_test(
        ViewConfig {
            id: "resource-id".into(),
            include: vec![
                FilterRule::ResourceId(StringMatcher::Exact(format!("remote/{REMOTE}/guest/100"))),
                FilterRule::ResourceId(StringMatcher::Exact(format!(
                    "remote/{REMOTE}/storage/{NODE}/{STORAGE}"
                ))),
            ],
            exclude: vec![
                FilterRule::ResourceId(StringMatcher::Exact(format!("remote/{REMOTE}/guest/101"))),
                FilterRule::ResourceId(StringMatcher::Exact(
                    "remote/otherremote/guest/101".to_string(),
                )),
                FilterRule::ResourceId(StringMatcher::Exact(format!(
                    "remote/{REMOTE}/storage/{NODE}/otherstorage"
                ))),
            ],
            ..Default::default()
        },
        &[
            (
                (REMOTE, &make_storage_resource(REMOTE, NODE, STORAGE)),
                true,
            ),
            (
                (REMOTE, &make_qemu_resource(REMOTE, NODE, 100, None, &[])),
                true,
            ),
            (
                (REMOTE, &make_lxc_resource(REMOTE, NODE, 101, None, &[])),
                false,
            ),
            (
                (REMOTE, &make_lxc_resource(REMOTE, NODE, 102, None, &[])),
                false,
            ),
            (
                (
                    "otherremote",
                    &make_lxc_resource("otherremote", NODE, 101, None, &[]),
                ),
                false,
            ),
            (
                (
                    "yetanoterremote",
                    &make_lxc_resource("yetanotherremote", NODE, 104, None, &[]),
                ),
                false,
            ),
        ],
    );
}

#[test]
fn node_included() {
    let view = View::new(ViewConfig {
        id: "both".into(),

        include: vec![
            FilterRule::Remote(StringMatcher::Exact("remote-a".to_string())),
            FilterRule::ResourceId(StringMatcher::Exact(
                "remote/someremote/node/test".to_string(),
            )),
        ],
        exclude: vec![FilterRule::Remote(StringMatcher::Exact(
            "remote-b".to_string(),
        ))],
        ..Default::default()
    });

    assert!(view.is_node_included("remote-a", "somenode"));
    assert!(view.is_node_included("remote-a", "somenode2"));
    assert!(!view.is_node_included("remote-b", "somenode"));
    assert!(!view.is_node_included("remote-b", "somenode2"));
    assert!(view.is_node_included("someremote", "test"));

    assert_eq!(view.name(), "both");
}

#[test]
fn can_skip_remote_if_excluded() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include: vec![],
        exclude: vec![FilterRule::Remote(StringMatcher::Exact(
            "remote-b".to_string(),
        ))],
        include_all: Some(true),
    });

    assert!(!view.can_skip_remote("remote-a"));
    assert!(view.can_skip_remote("remote-b"));
}

#[test]
fn can_skip_remote_if_included() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include: vec![FilterRule::Remote(StringMatcher::Exact(
            "remote-b".to_string(),
        ))],
        exclude: vec![],
        ..Default::default()
    });

    assert!(!view.can_skip_remote("remote-b"));
    assert!(view.can_skip_remote("remote-a"));
}

#[test]
fn can_skip_remote_cannot_skip_if_any_other_include() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include: vec![
            FilterRule::Remote(StringMatcher::Exact("remote-b".to_string())),
            FilterRule::ResourceId(StringMatcher::Exact(
                "resource/remote-a/guest/100".to_string(),
            )),
        ],
        exclude: vec![],
        ..Default::default()
    });

    assert!(!view.can_skip_remote("remote-b"));
    assert!(!view.can_skip_remote("remote-a"));
}

#[test]
fn can_skip_remote_explicit_remote_exclude() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include: vec![FilterRule::ResourceId(StringMatcher::Exact(
            "resource/remote-a/guest/100".to_string(),
        ))],
        exclude: vec![FilterRule::Remote(StringMatcher::Exact(
            "remote-a".to_string(),
        ))],
        ..Default::default()
    });

    assert!(view.can_skip_remote("remote-a"));
}

#[test]
fn can_skip_remote_with_empty_config() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        ..Default::default()
    });

    assert!(view.can_skip_remote("remote-a"));
    assert!(view.can_skip_remote("remote-b"));
}

#[test]
fn can_skip_remote_cannot_skip_if_all_included() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include_all: Some(true),
        ..Default::default()
    });

    assert!(!view.can_skip_remote("remote-a"));
    assert!(!view.can_skip_remote("remote-b"));
}

#[test]
fn can_skip_remote_with_no_remote_includes() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include: vec![FilterRule::ResourceId(StringMatcher::Exact(
            "resource/remote-a/guest/100".to_string(),
        ))],
        exclude: vec![],
        ..Default::default()
    });

    assert!(!view.can_skip_remote("remote-a"));
    assert!(!view.can_skip_remote("remote-b"));
}

#[test]
fn explicitly_included_remote() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include: vec![FilterRule::Remote(StringMatcher::Exact(
            "remote-b".to_string(),
        ))],
        exclude: vec![],
        ..Default::default()
    });

    assert!(view.is_remote_explicitly_included("remote-b"));
}

#[test]
fn included_and_excluded_same_remote() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include: vec![FilterRule::Remote(StringMatcher::Exact(
            "remote-b".to_string(),
        ))],
        exclude: vec![FilterRule::Remote(StringMatcher::Exact(
            "remote-b".to_string(),
        ))],
        ..Default::default()
    });

    assert!(!view.is_remote_explicitly_included("remote-b"));
}

#[test]
fn not_explicitly_included_remote() {
    let view = View::new(ViewConfig {
        id: "abc".into(),
        include: vec![],
        exclude: vec![],
        include_all: Some(true),
    });

    // Assert that is not *explicitly* included
    assert!(view.is_remote_explicitly_included("remote-b"));
}
