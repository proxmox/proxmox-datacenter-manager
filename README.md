# Proxmox Datacenter Manager

A stand-alone API + GUI product with the following main features for multiple instances of Proxmox
VE, Proxmox Backup Server and potentially also Proxmox Mail Gateway in one central place:

- Status and Health overview for the core resources. E.g., for PVE/PBS/PMG hosts, PVE guests or PBS
  backups
- *Basic* Management of the core resources.
- Connect those separate instances. E.g., cross-cluster VM live-migration.


## Feature Overview

The basic core features of the Datacenter Manager

- Connect & display an arbitrary amount of independent nodes or clusters ("Datacenters")
- View the resource usage of all nodes and their guests ("glorified dashboard")
  Automatic refresh with low frequency (range of ~1 - 30 minutes) combined with active refresh
  "button" for a specific node/cluster
- Basic management of the resources: shutdown, reboot, start, ...
- Management of some configurations

    - Backup jobs
    - Firewall
    - ..?

- Off-site replication copies of guest for manual recovery on DC failure (not HA!)
- Remote migration of virtual guests between different datacenters
  Advertising use of ZFS & Ceph backed replication for quicker transfer on actual migration
- View configuration health state (subscription, apt repositories, pending updates, backups done ...)
- Stand-alone daemon, possible to run in CT, VM, PVE host
- Support for complex TFA (like PBS), ACME/Let's Encrypt from the beginning
- For a later release:

    - master-slave architecture for standby managers to avoid single point of failure
    - PBS, PMG integration
    - ... to be determined from user feature requests

## Technology Stack

- release as simple .deb Package, one for the backend and one for the GUI
- backend: rust based, reusing PBS REST/API stack were possible

    - no privileged (root) operations required, so a single daemon is enough
    - TCP port 443 (default HTTPS one)

- fronted: start out with ExtJS; not ideal framework due to upstream being basically defunct, but we
  have lots of experience and a good widget base to create a prototype fast.

## User / Group / Permission System

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
