use pdm_api_types::{
    resource::{PveLxcResource, PveQemuResource, PveStorageResource, Resource},
    views::{ViewConfig, ViewConfigEntry},
};
use proxmox_section_config::typed::ApiSectionDataEntry;

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

fn parse_config(config: &str) -> ViewConfig {
    let config = ViewConfigEntry::parse_section_config("views.cfg", config).unwrap();
    let ViewConfigEntry::View(config) = config.get("test").unwrap();
    config.clone()
}

const NODE: &str = "somenode";
const STORAGE: &str = "somestorage";
const REMOTE: &str = "someremote";

#[test]
fn include_remotes() {
    let config = parse_config(
        "
view: test
    include remote=remote-a
    include remote=remote-b
",
    );

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
    let config = parse_config(
        "
view: test
    include-all true
    exclude remote=remote-a
    exclude remote=remote-b
",
    );

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
    let config = parse_config(
        "
view: test
    include remote=remote-a
    include remote=remote-b
    exclude remote=remote-b
    exclude remote=remote-c
",
    );

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
    let config = parse_config(
        "
view: test
    include-all true
",
    );
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
    let config = parse_config(
        "
view: test
    include resource-type=storage
    include resource-type=qemu
",
    );
    run_test(
        config,
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
    let config = parse_config(
        "
view: test
    include-all true
    exclude resource-type=storage
    exclude resource-type=qemu
",
    );
    run_test(
        config,
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
    let config = parse_config(
        "
view: test
    include resource-type=qemu
    exclude resource-type=storage
",
    );

    run_test(
        config,
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
    let config = parse_config(
        "
view: test
    include tag=tag1
    include tag=tag2
    exclude tag=tag3
",
    );
    run_test(
        config,
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
    let config = parse_config(
        "
view: test
    include resource-pool=pool1
    include resource-pool=pool2
    exclude resource-pool=pool2
",
    );
    run_test(
        config,
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
    let config = parse_config(
        "
view: test
    include resource-id=remote/someremote/guest/100
    include resource-id=remote/someremote/storage/somenode/somestorage
    exclude resource-id=remote/someremote/guest/101
    exclude resource-id=remote/otherremote/guest/101
    exclude resource-id=remote/someremote/storage/somenode/otherstorage
",
    );
    run_test(
        config,
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
    let config = parse_config(
        "
view: test
    include remote=remote-a
    include resource-id=remote/someremote/node/test
    exclude remote=remote-b
",
    );

    let view = View::new(config);

    assert!(view.is_node_included("remote-a", "somenode"));
    assert!(view.is_node_included("remote-a", "somenode2"));
    assert!(!view.is_node_included("remote-b", "somenode"));
    assert!(!view.is_node_included("remote-b", "somenode2"));
    assert!(view.is_node_included("someremote", "test"));

    assert_eq!(view.name(), "test");
}

#[test]
fn can_skip_remote_if_excluded() {
    let config = parse_config(
        "
view: test
    include-all true
    exclude remote=remote-b
",
    );
    let view = View::new(config);

    assert!(!view.can_skip_remote("remote-a"));
    assert!(view.can_skip_remote("remote-b"));
}

#[test]
fn can_skip_remote_if_included() {
    let config = parse_config(
        "
view: test
    include remote=remote-b
",
    );

    let view = View::new(config);

    assert!(!view.can_skip_remote("remote-b"));
    assert!(view.can_skip_remote("remote-a"));
}

#[test]
fn can_skip_remote_cannot_skip_if_any_other_include() {
    let config = parse_config(
        "
view: test
    include remote=remote-b
    include resource-id=remote/remote-a/guest/100
",
    );

    let view = View::new(config);

    assert!(!view.can_skip_remote("remote-b"));
    assert!(!view.can_skip_remote("remote-a"));
}

#[test]
fn can_skip_remote_explicit_remote_exclude() {
    let config = parse_config(
        "
view: test
    exclude remote=remote-a
    include resource-id=remote/remote-a/guest/100
",
    );

    let view = View::new(config);

    assert!(view.can_skip_remote("remote-a"));
}

#[test]
fn can_skip_remote_with_empty_config() {
    let config = parse_config(
        "
view: test
",
    );

    let view = View::new(config);

    assert!(view.can_skip_remote("remote-a"));
    assert!(view.can_skip_remote("remote-b"));
}

#[test]
fn can_skip_remote_cannot_skip_if_all_included() {
    let config = parse_config(
        "
view: test
    include-all true
",
    );

    let view = View::new(config);

    assert!(!view.can_skip_remote("remote-a"));
    assert!(!view.can_skip_remote("remote-b"));
}

#[test]
fn can_skip_remote_with_no_remote_includes() {
    let config = parse_config(
        "
view: test
    include resource-id=remote/remote-a/guest/100
",
    );

    let view = View::new(config);

    assert!(!view.can_skip_remote("remote-a"));
    assert!(!view.can_skip_remote("remote-b"));
}

#[test]
fn explicitly_included_remote() {
    let config = parse_config(
        "
view: test
    include remote=remote-b
",
    );

    let view = View::new(config);

    assert!(view.is_remote_explicitly_included("remote-b"));
}

#[test]
fn included_and_excluded_same_remote() {
    let config = parse_config(
        "
view: test
    include remote=remote-b
    exclude remote=remote-b
",
    );

    let view = View::new(config);

    assert!(!view.is_remote_explicitly_included("remote-b"));
}

#[test]
fn not_explicitly_included_remote() {
    let config = parse_config(
        "
view: test
    include-all true
",
    );
    let view = View::new(config);

    // Assert that is not *explicitly* included
    assert!(view.is_remote_explicitly_included("remote-b"));
}
