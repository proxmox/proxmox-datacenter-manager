# Proxmox Datacenter Manager

A stand-alone API + GUI product with the following main features for multiple instances of Proxmox
VE and Proxmox Backup Server in one central place.

## Feature Overview

- Connect & display an arbitrary amount of independent nodes or clusters ("Remotes")
- View the status and load of all resources, which includes nodes, virtual guests, storages,
  datastores and so on. Proxmox Datacenter Manager provides a dashboard that tries to present
  information such that potential problematic outliers can be found easily.
- Customizable dashboards ("views") showing a configurable subset of resources
- Basic management of the guest resources
  - Resource graphs
  - Basic power management (start, reboot, shutdown)
- Remote shell for Proxmox VE and Proxmox Backup Server remotes
- Global overview over available system updates for managed remotes
- Firewall overview for all managed remotes
- Basic SDN overview for all managed remotes
- Remote migration of virtual guests between different datacenters
  Advertising use of ZFS & Ceph backed replication for quicker transfer on actual migration
- View configuration health state (subscription, APT repositories, pending updates, ...)
- User management / access control
  - Users/API token
  - Support for LDAP and Active Directory
  - Support for OpenID Connect
  - Support for complex Two-Factor Authentication
- ACME/Let's Encrypt

- A non-exhaustive list of features planned for future releases is:
  - Management of more configuration (e.g. backup jobs, notification policies, package repos, HA)
  - Active-standby architecture for standby instances of PDM to avoid single point of failure.
  - Integration of other projects, like Proxmox Mail Gateway, and potentially also Proxmox Offline Mirror.
  - Off-site replication copies of guest for manual recovery on DC failure (not HA!)
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
