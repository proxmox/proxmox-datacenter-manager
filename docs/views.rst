Views
=====

Views allow you to add an interactive view on a selected set of resources.

Resource Selection
------------------

The resource selection is controlled by an include-exclude filter system.

You define what resources to consider for including which then get passed through an exclude list to
single specific types out again.

This way you can, for example, easily configure to include all virtual machine resources, but then
exclude any such VM that resides on a specific remote.

Filter Types
^^^^^^^^^^^^

 .. todo auto-generate below list

The following lists of filter types are available to be used in include or exclude lists.

- The `resource-type` filter allows you to filter by a specific resource type.
  The following types are available:

  - `datastore`: A Proxmox Backup Server datastore.
  - `lxc`: A LXC container.
  - `node`: A Proxmox VE or Proxmox Backup Server node.
  - `qemu`: A QEMU virtual machine.
  - `sdn-zone`: A SDN zone.
  - `storage`: A Proxmox VE storage

- The `resource-pool` filter allows you to include or exclude only resources that are located in a
  specific resource pool-name.
- The `tag` filter allows you to filter resources that are tagged with a specific tag-name.
- The `remote` filter allows you to filter resources located on a specific remote.
- The `resource-id` filter allows you to filter resources with a specific ID.


Each filter can be prefixed with an optional `<match-behavior>:` prefix. Currently there is only
the `exact` matching behavior available. This behavior is the default if no prefix is provided.


Customizable Dashboard
----------------------

You can create customizable dashboards for a views from a set of pre-defined widgets.
Only resources matching your include minus the ones matching your exclude filters will be displayed
in these widgets.


Access Control
--------------

You can grant permissions on specific views. With such a permission the user can operate on the
view and all its selected resources.
