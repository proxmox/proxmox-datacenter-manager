.. _views:

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


Widgets
^^^^^^^

The following widgets are available:

- The `nodes` widget shows a status overview of the Proxmox VE and Proxmox
  Backup Server nodes, and can be limited to a single remote type.
- The `guests` widget shows a status overview of the virtual guests, and can be
  limited to QEMU virtual machines or LXC containers.
- The `pbs-datastores` widget shows the usage and status of Proxmox Backup
  Server datastores.
- The `remotes` widget lists the configured remotes and their status. It can
  also show a wizard for adding a new remote.
- The `subscription` widget shows the subscription status of the remotes.
- The `sdn` widget shows the status of the Software-Defined Networking (SDN)
  zones.
- The `leaderboard` widget ranks resources by a metric, such as guest or node
  CPU or node memory usage, and lists the top consumers.
- The `task-summary` widget summarizes recent tasks, grouped by a chosen
  criterion.
- The `resource-tree` widget shows the selected resources in a hierarchical
  tree.
- The `node-resource-gauge` widget displays a single node resource, such as
  CPU, memory, or storage, as a gauge chart, and can be limited to a single
  remote type.
- The `map` widget plots the remotes on an interactive world map at their
  configured geographic location. Markers are colored by the remote's status,
  and markers that are close together are clustered as you zoom out.

Map Widget
^^^^^^^^^^

The locations shown by the `map` widget come from the remotes and cannot be
edited in Proxmox Datacenter Manager. Configure them on the remote side: on
Proxmox VE in the datacenter configuration as a cluster-wide default, optionally
overridden per node, and on Proxmox Backup Server in the node configuration.

The map background is drawn from public-domain vector data by `Natural Earth
<https://www.naturalearthdata.com/>`_, shipped in the `proxmox-geojson-data`
package.


Access Control
--------------

You can grant permissions on specific views. With such a permission the user can operate on the
view and all its selected resources.
