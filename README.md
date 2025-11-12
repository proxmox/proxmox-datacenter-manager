# Proxmox Datacenter Manager

A stand-alone API + GUI product with the following main features for multiple instances of Proxmox
VE, Proxmox Backup Server and potentially also Proxmox Mail Gateway in one central place:

- Status and Health overview for the core resources. E.g., for PVE/PBS/PMG hosts, PVE guests or PBS
  backups
- *Basic* Management of the core resources.
- Connect those separate instances. E.g., cross-cluster VM live-migration and also SDN in the long
  term.

## Feature Overview

The basic core features of the Proxmox Datacenter Manager planned for the long
run (not everything will be finished for the initial 1.0):

- Connect & display an arbitrary amount of independent nodes or clusters ("Datacenters")
- View the status and load of all resources, which includes nodes, virtual guests, storages, datastores and so on.
  This part basically will be a "glorified dashboard" that tries to present information such that
  potential problematic outliers can be found more easily. Automatic refresh with low frequency
  (range of ~1 - 30 minutes) combined with active refresh "button" for a specific node/cluster
- Basic management of the resources: shutdown, reboot, start, ...
- Management of some configurations

  - Backup jobs
  - Firewall
  - SDN
  - ..?

- Off-site replication copies of guest for manual recovery on DC failure (not HA!)
- Remote migration of virtual guests between different datacenters
  Advertising use of ZFS & Ceph backed replication for quicker transfer on actual migration
- View configuration health state (subscription, apt repositories, pending updates, backups done ...)
- Stand-alone daemon, possible to run in CT, VM, PVE host
- Support for complex TFA (like PBS), ACME/Let's Encrypt from the beginning
- For a later release:

  - active-stanby architecture for standby managers to avoid single point of failure.
  - integration of other projects, like Proxmox Mail Gateway, and potentially also some form of
    Proxmox Offline Mirror.
  - ... to be determined from user feedback and feature requests.

## Technology Overview

### Backend
- Implemented in Rust, reusing code from Proxmox Backup Server where possible
- A for Proxmox projects standard dual-stack of API daemons. One as main API daemon running as
  unprivileged users and one privileged daemon running as root. Contrary to other projects the
  privileged daemon exclusively listens on a file based UNIX socket, thus restricting attack surface
  even further.
- The backend listens on port 8443 (TLS only)
- The code for the backend server is located in the `server/` directory.

### Frontend

- The Web UI communicates with the backend server via a JSON-based REST API.
- The UI is implemented in Rust, using [Yew](https://yew.rs/) and the 
  [proxmox-yew-widget-toolkit](https://git.proxmox.com/?p=ui/proxmox-yew-widget-toolkit.git;a=summary).
  The Rust code is compiled to WebAssembly.
- The code for the UI is located in the `ui/` directory.

### CLI tools

There are two CLI tools to manage Proxmox Datacenter Manager.
- `proxmox-datacenter-manager-client`: client using the PDM API, can be used to
  control local or remote PDM instances
- `proxmox-datacenter-manager-admin`: root-only, local administration tool

Their implementation can be found in `cli/admin` and `cli/client`, respectively.

## User and Permission System

### Use cases to Support

- Simplest, one or more admin working on equal terms and using PDM to manage resources owned by the
  same entity (e.g., company)
  They want a simple way to add one API-token per Proxmox product to PDM
- More complex admin hierarchy, or (support) staff involved, where some need to manage parts of
  their Proxmox infra, and some need to only audit part of the Proxmox infra, possibly on partially
  overlapping hosts sets.
  Flexible groups are required, some way to distinguish between admin/audit user while not blowing
  up complexity of different credentials to add for each Proxmox project

IOW., we want to have a somewhat flexible system while not blowing out (potential) complexity out of
proportions.

### Proposed mechanism:

- Simplified privilege-roles:

  - Audit: only access to GET calls
  - Manage: only access to GET and some POST calls, to alter the state (e.g., start/stop) of a
    resource, but not to alter its configuration

    POST allows creation too, so this may require some annotations in the API schema.

  - Admin: can do everything

- On PDM one configures remotes, a remote includes (at least) the following config properties:

  - ID with a name (not auto-generated)
  - A host, i.e., for a PVE (cluster), PBS, PMG instance

    For PVE we may want to allow configuring fallback hosts, or do some round-robin in general

  - A '''list''' of API-Token (recommended, but "plain" user also allowed)

-  Then there are groups of remotes, they contain a list of entries with (at least):

  - the remote ID
  - the actual user to use from the list configured for that remote
    This allows to partition privs that a PDM group can actually enact on the managed Proxmox
    Products.

    e.g, one can add two API tokens, one for full access on a PVE cluster one for limited to a
    pool, then one PDM admin-group can use the fully privileged and one user-group can use the
    limited one while still requiring only a single remote entry for that PVE cluster.

- PDM Users get access to that group via a priv. role on it:

  - Audit/Manage/Admin -> /groups/${ID}
