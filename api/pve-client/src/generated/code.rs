//! PVE API client
//! Note that the following API URLs are not handled currently:
//!
//! ```text
//! - /access
//! - /access/acl
//! - /access/domains
//! - /access/domains/{realm}
//! - /access/domains/{realm}/sync
//! - /access/groups
//! - /access/groups/{groupid}
//! - /access/openid
//! - /access/openid/auth-url
//! - /access/openid/login
//! - /access/password
//! - /access/permissions
//! - /access/roles
//! - /access/roles/{roleid}
//! - /access/tfa
//! - /access/tfa/{userid}
//! - /access/tfa/{userid}/{id}
//! - /access/ticket
//! - /access/users
//! - /access/users/{userid}
//! - /access/users/{userid}/tfa
//! - /access/users/{userid}/token
//! - /access/users/{userid}/token/{tokenid}
//! - /cluster
//! - /cluster/acme
//! - /cluster/acme/account
//! - /cluster/acme/account/{name}
//! - /cluster/acme/challenge-schema
//! - /cluster/acme/directories
//! - /cluster/acme/plugins
//! - /cluster/acme/plugins/{id}
//! - /cluster/acme/tos
//! - /cluster/backup
//! - /cluster/backup-info
//! - /cluster/backup-info/not-backed-up
//! - /cluster/backup/{id}
//! - /cluster/backup/{id}/included_volumes
//! - /cluster/ceph
//! - /cluster/ceph/flags
//! - /cluster/ceph/flags/{flag}
//! - /cluster/ceph/metadata
//! - /cluster/ceph/status
//! - /cluster/config
//! - /cluster/config/apiversion
//! - /cluster/config/join
//! - /cluster/config/nodes
//! - /cluster/config/nodes/{node}
//! - /cluster/config/qdevice
//! - /cluster/config/totem
//! - /cluster/firewall
//! - /cluster/firewall/aliases
//! - /cluster/firewall/aliases/{name}
//! - /cluster/firewall/groups
//! - /cluster/firewall/groups/{group}
//! - /cluster/firewall/groups/{group}/{pos}
//! - /cluster/firewall/ipset
//! - /cluster/firewall/ipset/{name}
//! - /cluster/firewall/ipset/{name}/{cidr}
//! - /cluster/firewall/macros
//! - /cluster/firewall/options
//! - /cluster/firewall/refs
//! - /cluster/firewall/rules
//! - /cluster/firewall/rules/{pos}
//! - /cluster/ha
//! - /cluster/ha/groups
//! - /cluster/ha/groups/{group}
//! - /cluster/ha/resources
//! - /cluster/ha/resources/{sid}
//! - /cluster/ha/resources/{sid}/migrate
//! - /cluster/ha/resources/{sid}/relocate
//! - /cluster/ha/status
//! - /cluster/ha/status/current
//! - /cluster/ha/status/manager_status
//! - /cluster/jobs
//! - /cluster/jobs/schedule-analyze
//! - /cluster/log
//! - /cluster/metrics
//! - /cluster/metrics/server
//! - /cluster/metrics/server/{id}
//! - /cluster/nextid
//! - /cluster/options
//! - /cluster/replication
//! - /cluster/replication/{id}
//! - /cluster/status
//! - /cluster/tasks
//! - /nodes/{node}
//! - /nodes/{node}/aplinfo
//! - /nodes/{node}/apt
//! - /nodes/{node}/apt/changelog
//! - /nodes/{node}/apt/repositories
//! - /nodes/{node}/apt/update
//! - /nodes/{node}/apt/versions
//! - /nodes/{node}/capabilities
//! - /nodes/{node}/capabilities/qemu
//! - /nodes/{node}/capabilities/qemu/cpu
//! - /nodes/{node}/capabilities/qemu/machines
//! - /nodes/{node}/ceph
//! - /nodes/{node}/ceph/cfg
//! - /nodes/{node}/ceph/cfg/db
//! - /nodes/{node}/ceph/cfg/raw
//! - /nodes/{node}/ceph/cmd-safety
//! - /nodes/{node}/ceph/config
//! - /nodes/{node}/ceph/configdb
//! - /nodes/{node}/ceph/crush
//! - /nodes/{node}/ceph/fs
//! - /nodes/{node}/ceph/fs/{name}
//! - /nodes/{node}/ceph/init
//! - /nodes/{node}/ceph/log
//! - /nodes/{node}/ceph/mds
//! - /nodes/{node}/ceph/mds/{name}
//! - /nodes/{node}/ceph/mgr
//! - /nodes/{node}/ceph/mgr/{id}
//! - /nodes/{node}/ceph/mon
//! - /nodes/{node}/ceph/mon/{monid}
//! - /nodes/{node}/ceph/osd
//! - /nodes/{node}/ceph/osd/{osdid}
//! - /nodes/{node}/ceph/osd/{osdid}/in
//! - /nodes/{node}/ceph/osd/{osdid}/lv-info
//! - /nodes/{node}/ceph/osd/{osdid}/metadata
//! - /nodes/{node}/ceph/osd/{osdid}/out
//! - /nodes/{node}/ceph/osd/{osdid}/scrub
//! - /nodes/{node}/ceph/pool
//! - /nodes/{node}/ceph/pool/{name}
//! - /nodes/{node}/ceph/pool/{name}/status
//! - /nodes/{node}/ceph/pools
//! - /nodes/{node}/ceph/pools/{name}
//! - /nodes/{node}/ceph/restart
//! - /nodes/{node}/ceph/rules
//! - /nodes/{node}/ceph/start
//! - /nodes/{node}/ceph/status
//! - /nodes/{node}/ceph/stop
//! - /nodes/{node}/certificates
//! - /nodes/{node}/certificates/acme
//! - /nodes/{node}/certificates/acme/certificate
//! - /nodes/{node}/certificates/custom
//! - /nodes/{node}/certificates/info
//! - /nodes/{node}/config
//! - /nodes/{node}/disks
//! - /nodes/{node}/disks/directory
//! - /nodes/{node}/disks/directory/{name}
//! - /nodes/{node}/disks/initgpt
//! - /nodes/{node}/disks/list
//! - /nodes/{node}/disks/lvm
//! - /nodes/{node}/disks/lvm/{name}
//! - /nodes/{node}/disks/lvmthin
//! - /nodes/{node}/disks/lvmthin/{name}
//! - /nodes/{node}/disks/smart
//! - /nodes/{node}/disks/wipedisk
//! - /nodes/{node}/disks/zfs
//! - /nodes/{node}/disks/zfs/{name}
//! - /nodes/{node}/dns
//! - /nodes/{node}/execute
//! - /nodes/{node}/firewall
//! - /nodes/{node}/firewall/log
//! - /nodes/{node}/firewall/options
//! - /nodes/{node}/firewall/rules
//! - /nodes/{node}/firewall/rules/{pos}
//! - /nodes/{node}/hardware
//! - /nodes/{node}/hardware/pci
//! - /nodes/{node}/hardware/pci/{pciid}
//! - /nodes/{node}/hardware/pci/{pciid}/mdev
//! - /nodes/{node}/hardware/usb
//! - /nodes/{node}/hosts
//! - /nodes/{node}/journal
//! - /nodes/{node}/lxc/{vmid}
//! - /nodes/{node}/lxc/{vmid}/clone
//! - /nodes/{node}/lxc/{vmid}/config
//! - /nodes/{node}/lxc/{vmid}/feature
//! - /nodes/{node}/lxc/{vmid}/firewall
//! - /nodes/{node}/lxc/{vmid}/firewall/aliases
//! - /nodes/{node}/lxc/{vmid}/firewall/aliases/{name}
//! - /nodes/{node}/lxc/{vmid}/firewall/ipset
//! - /nodes/{node}/lxc/{vmid}/firewall/ipset/{name}
//! - /nodes/{node}/lxc/{vmid}/firewall/ipset/{name}/{cidr}
//! - /nodes/{node}/lxc/{vmid}/firewall/log
//! - /nodes/{node}/lxc/{vmid}/firewall/options
//! - /nodes/{node}/lxc/{vmid}/firewall/refs
//! - /nodes/{node}/lxc/{vmid}/firewall/rules
//! - /nodes/{node}/lxc/{vmid}/firewall/rules/{pos}
//! - /nodes/{node}/lxc/{vmid}/migrate
//! - /nodes/{node}/lxc/{vmid}/move_volume
//! - /nodes/{node}/lxc/{vmid}/mtunnel
//! - /nodes/{node}/lxc/{vmid}/mtunnelwebsocket
//! - /nodes/{node}/lxc/{vmid}/pending
//! - /nodes/{node}/lxc/{vmid}/remote_migrate
//! - /nodes/{node}/lxc/{vmid}/resize
//! - /nodes/{node}/lxc/{vmid}/rrd
//! - /nodes/{node}/lxc/{vmid}/rrddata
//! - /nodes/{node}/lxc/{vmid}/snapshot
//! - /nodes/{node}/lxc/{vmid}/snapshot/{snapname}
//! - /nodes/{node}/lxc/{vmid}/snapshot/{snapname}/config
//! - /nodes/{node}/lxc/{vmid}/snapshot/{snapname}/rollback
//! - /nodes/{node}/lxc/{vmid}/spiceproxy
//! - /nodes/{node}/lxc/{vmid}/status
//! - /nodes/{node}/lxc/{vmid}/status/current
//! - /nodes/{node}/lxc/{vmid}/status/reboot
//! - /nodes/{node}/lxc/{vmid}/status/resume
//! - /nodes/{node}/lxc/{vmid}/status/shutdown
//! - /nodes/{node}/lxc/{vmid}/status/start
//! - /nodes/{node}/lxc/{vmid}/status/stop
//! - /nodes/{node}/lxc/{vmid}/status/suspend
//! - /nodes/{node}/lxc/{vmid}/template
//! - /nodes/{node}/lxc/{vmid}/termproxy
//! - /nodes/{node}/lxc/{vmid}/vncproxy
//! - /nodes/{node}/lxc/{vmid}/vncwebsocket
//! - /nodes/{node}/migrateall
//! - /nodes/{node}/netstat
//! - /nodes/{node}/network
//! - /nodes/{node}/network/{iface}
//! - /nodes/{node}/qemu/{vmid}
//! - /nodes/{node}/qemu/{vmid}/agent
//! - /nodes/{node}/qemu/{vmid}/agent/exec
//! - /nodes/{node}/qemu/{vmid}/agent/exec-status
//! - /nodes/{node}/qemu/{vmid}/agent/file-read
//! - /nodes/{node}/qemu/{vmid}/agent/file-write
//! - /nodes/{node}/qemu/{vmid}/agent/fsfreeze-freeze
//! - /nodes/{node}/qemu/{vmid}/agent/fsfreeze-status
//! - /nodes/{node}/qemu/{vmid}/agent/fsfreeze-thaw
//! - /nodes/{node}/qemu/{vmid}/agent/fstrim
//! - /nodes/{node}/qemu/{vmid}/agent/get-fsinfo
//! - /nodes/{node}/qemu/{vmid}/agent/get-host-name
//! - /nodes/{node}/qemu/{vmid}/agent/get-memory-block-info
//! - /nodes/{node}/qemu/{vmid}/agent/get-memory-blocks
//! - /nodes/{node}/qemu/{vmid}/agent/get-osinfo
//! - /nodes/{node}/qemu/{vmid}/agent/get-time
//! - /nodes/{node}/qemu/{vmid}/agent/get-timezone
//! - /nodes/{node}/qemu/{vmid}/agent/get-users
//! - /nodes/{node}/qemu/{vmid}/agent/get-vcpus
//! - /nodes/{node}/qemu/{vmid}/agent/info
//! - /nodes/{node}/qemu/{vmid}/agent/network-get-interfaces
//! - /nodes/{node}/qemu/{vmid}/agent/ping
//! - /nodes/{node}/qemu/{vmid}/agent/set-user-password
//! - /nodes/{node}/qemu/{vmid}/agent/shutdown
//! - /nodes/{node}/qemu/{vmid}/agent/suspend-disk
//! - /nodes/{node}/qemu/{vmid}/agent/suspend-hybrid
//! - /nodes/{node}/qemu/{vmid}/agent/suspend-ram
//! - /nodes/{node}/qemu/{vmid}/clone
//! - /nodes/{node}/qemu/{vmid}/cloudinit
//! - /nodes/{node}/qemu/{vmid}/cloudinit/dump
//! - /nodes/{node}/qemu/{vmid}/config
//! - /nodes/{node}/qemu/{vmid}/feature
//! - /nodes/{node}/qemu/{vmid}/firewall
//! - /nodes/{node}/qemu/{vmid}/firewall/aliases
//! - /nodes/{node}/qemu/{vmid}/firewall/aliases/{name}
//! - /nodes/{node}/qemu/{vmid}/firewall/ipset
//! - /nodes/{node}/qemu/{vmid}/firewall/ipset/{name}
//! - /nodes/{node}/qemu/{vmid}/firewall/ipset/{name}/{cidr}
//! - /nodes/{node}/qemu/{vmid}/firewall/log
//! - /nodes/{node}/qemu/{vmid}/firewall/options
//! - /nodes/{node}/qemu/{vmid}/firewall/refs
//! - /nodes/{node}/qemu/{vmid}/firewall/rules
//! - /nodes/{node}/qemu/{vmid}/firewall/rules/{pos}
//! - /nodes/{node}/qemu/{vmid}/migrate
//! - /nodes/{node}/qemu/{vmid}/monitor
//! - /nodes/{node}/qemu/{vmid}/move_disk
//! - /nodes/{node}/qemu/{vmid}/mtunnel
//! - /nodes/{node}/qemu/{vmid}/mtunnelwebsocket
//! - /nodes/{node}/qemu/{vmid}/pending
//! - /nodes/{node}/qemu/{vmid}/remote_migrate
//! - /nodes/{node}/qemu/{vmid}/resize
//! - /nodes/{node}/qemu/{vmid}/rrd
//! - /nodes/{node}/qemu/{vmid}/rrddata
//! - /nodes/{node}/qemu/{vmid}/sendkey
//! - /nodes/{node}/qemu/{vmid}/snapshot
//! - /nodes/{node}/qemu/{vmid}/snapshot/{snapname}
//! - /nodes/{node}/qemu/{vmid}/snapshot/{snapname}/config
//! - /nodes/{node}/qemu/{vmid}/snapshot/{snapname}/rollback
//! - /nodes/{node}/qemu/{vmid}/spiceproxy
//! - /nodes/{node}/qemu/{vmid}/status
//! - /nodes/{node}/qemu/{vmid}/status/current
//! - /nodes/{node}/qemu/{vmid}/status/reboot
//! - /nodes/{node}/qemu/{vmid}/status/reset
//! - /nodes/{node}/qemu/{vmid}/status/resume
//! - /nodes/{node}/qemu/{vmid}/status/shutdown
//! - /nodes/{node}/qemu/{vmid}/status/start
//! - /nodes/{node}/qemu/{vmid}/status/stop
//! - /nodes/{node}/qemu/{vmid}/status/suspend
//! - /nodes/{node}/qemu/{vmid}/template
//! - /nodes/{node}/qemu/{vmid}/termproxy
//! - /nodes/{node}/qemu/{vmid}/unlink
//! - /nodes/{node}/qemu/{vmid}/vncproxy
//! - /nodes/{node}/qemu/{vmid}/vncwebsocket
//! - /nodes/{node}/query-url-metadata
//! - /nodes/{node}/replication
//! - /nodes/{node}/replication/{id}
//! - /nodes/{node}/replication/{id}/log
//! - /nodes/{node}/replication/{id}/schedule_now
//! - /nodes/{node}/replication/{id}/status
//! - /nodes/{node}/report
//! - /nodes/{node}/rrd
//! - /nodes/{node}/rrddata
//! - /nodes/{node}/scan
//! - /nodes/{node}/scan/cifs
//! - /nodes/{node}/scan/glusterfs
//! - /nodes/{node}/scan/iscsi
//! - /nodes/{node}/scan/lvm
//! - /nodes/{node}/scan/lvmthin
//! - /nodes/{node}/scan/nfs
//! - /nodes/{node}/scan/pbs
//! - /nodes/{node}/scan/zfs
//! - /nodes/{node}/services
//! - /nodes/{node}/services/{service}
//! - /nodes/{node}/services/{service}/reload
//! - /nodes/{node}/services/{service}/restart
//! - /nodes/{node}/services/{service}/start
//! - /nodes/{node}/services/{service}/state
//! - /nodes/{node}/services/{service}/stop
//! - /nodes/{node}/spiceshell
//! - /nodes/{node}/startall
//! - /nodes/{node}/status
//! - /nodes/{node}/stopall
//! - /nodes/{node}/storage
//! - /nodes/{node}/storage/{storage}
//! - /nodes/{node}/storage/{storage}/content
//! - /nodes/{node}/storage/{storage}/content/{volume}
//! - /nodes/{node}/storage/{storage}/download-url
//! - /nodes/{node}/storage/{storage}/file-restore
//! - /nodes/{node}/storage/{storage}/file-restore/download
//! - /nodes/{node}/storage/{storage}/file-restore/list
//! - /nodes/{node}/storage/{storage}/prunebackups
//! - /nodes/{node}/storage/{storage}/rrd
//! - /nodes/{node}/storage/{storage}/rrddata
//! - /nodes/{node}/storage/{storage}/status
//! - /nodes/{node}/storage/{storage}/upload
//! - /nodes/{node}/subscription
//! - /nodes/{node}/syslog
//! - /nodes/{node}/tasks
//! - /nodes/{node}/tasks/{upid}
//! - /nodes/{node}/tasks/{upid}/log
//! - /nodes/{node}/tasks/{upid}/status
//! - /nodes/{node}/termproxy
//! - /nodes/{node}/time
//! - /nodes/{node}/version
//! - /nodes/{node}/vncshell
//! - /nodes/{node}/vncwebsocket
//! - /nodes/{node}/vzdump
//! - /nodes/{node}/vzdump/defaults
//! - /nodes/{node}/vzdump/extractconfig
//! - /nodes/{node}/wakeonlan
//! - /pools
//! - /pools/{poolid}
//! - /storage
//! - /storage/{storage}
//! ```
use proxmox_client::{Environment, Error};

use crate::helpers::*;
use crate::types::*;
use crate::Client;

impl<E> Client<E>
where
    E: Environment,
    E::Error: From<anyhow::Error>,
    anyhow::Error: From<E::Error>,
{
    /// Resources index (cluster wide).
    pub async fn cluster_resources(
        &self,
        ty: Option<ClusterResourceKind>,
    ) -> Result<Vec<ClusterResource>, E::Error> {
        let (mut query, mut sep) = (String::new(), '?');
        add_query_arg(&mut query, &mut sep, "type", &ty);
        let url = format!("/api2/extjs/cluster/resources{query}");
        Ok(self
            .client
            .get(&url)
            .await?
            .data
            .ok_or_else(|| E::Error::bad_api("api returned no data"))?)
    }

    /// LXC container index (per node).
    pub async fn list_lxc(&self, node: &str) -> Result<Vec<LxcEntry>, E::Error> {
        let url = format!("/api2/extjs/nodes/{node}/lxc");
        Ok(self
            .client
            .get(&url)
            .await?
            .data
            .ok_or_else(|| E::Error::bad_api("api returned no data"))?)
    }

    /// Cluster node index.
    pub async fn list_nodes(&self) -> Result<Vec<ClusterNodeIndexResponse>, E::Error> {
        let url = format!("/api2/extjs/nodes");
        Ok(self
            .client
            .get(&url)
            .await?
            .data
            .ok_or_else(|| E::Error::bad_api("api returned no data"))?)
    }

    /// Virtual machine index (per node).
    pub async fn list_qemu(
        &self,
        node: &str,
        full: Option<bool>,
    ) -> Result<Vec<VmEntry>, E::Error> {
        let (mut query, mut sep) = (String::new(), '?');
        add_query_arg(&mut query, &mut sep, "full", &full);
        let url = format!("/api2/extjs/nodes/{node}/qemu{query}");
        Ok(self
            .client
            .get(&url)
            .await?
            .data
            .ok_or_else(|| E::Error::bad_api("api returned no data"))?)
    }

    /// API version details, including some parts of the global datacenter
    /// config.
    pub async fn version(&self) -> Result<VersionResponse, E::Error> {
        let url = format!("/api2/extjs/version");
        Ok(self
            .client
            .get(&url)
            .await?
            .data
            .ok_or_else(|| E::Error::bad_api("api returned no data"))?)
    }
}
