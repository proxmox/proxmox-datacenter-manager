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
- Implemented in the Rust programming language, reusing code from Proxmox Backup Server where possible
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


## Documentation

Documentation (user-facing as well as developer-facing) can be found in `docs/`.
