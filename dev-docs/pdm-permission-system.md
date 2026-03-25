# ACL-Object path & Privileges


## `/system/{network,updates,disks,...}`

For basic PDM system management.

Privileges:

- System.Audit
- System.Modify

## `/resource/{remote-id}/{resource-type=guest,storage}/{resource-id}`

To see, manage or modify specific resources. Keep resource-types rather minimal for now, e.g., no
SDN or the node (host) for now, require the rest.

- Resource.Audit -> read-only
- Resource.Manage -> Migrate, Start, Stop, ...
- Resource.Modify -> Change config or state of resource
- Resource.Migrate -> Remote Migration
- Resource.Delete -> Delete guests

In the future we might extend this to something like:

- Resource.Guest.Modify -> limited to guest related API calls and parameters on privilege level
- Resource.Storage.Modify -> limited to storage related API calls and parameters on privilege level
- Resource.User.Modify (once we integrated user and access control management of remotes, something
  for the mid/long-term future)

The no-subtype ones, e.g. Resource.Modify, are seen as super-set of the per-resource type one.
Should only be really evaluated after public feedback about the beta.

## `/access/{user,realm,acl}`

To see or modify specific resources. Keep resource-types rather minimal for now, e.g., no
SDN or the node (host) for now, require the rest.

- Access.Audit -> read-only
- Access.Modify -> Change config or state of resource

We could also create sub-types to provide more flexibility, like:
- Access.ACL.Modify
- Access.User.Modify

The biggest value from having a separate ACL and User modification privilege would be the ability to
ensure on role-level that a user cannot give themselves more permissions.

While that would speak for having this from the beginning, it's not a must from a technical POV, it
could be still added later on, as it's an extension.

# Roles

- Administrator -> all, ideally only to allow permission modifications by default.
- Auditor
- SystemAdministrator
- SystemAuditor
- ResourceAdministrator
- ResourceAuditor
- AccessAuditor
- ... can be extended in the future.

## Use cases to Support (TODO: is this still relevant?)

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
