Remotes
=======

Proxmox Datacenter Manager allows you to add arbitrary Proxmox VE nodes or clusters and Proxmox
Backup Server instances as remotes. This allows for a structured, unified overview of every host,
VM, container, and datastore across different locations.

Resource Operation
------------------

Through the Proxmox Datacenter Manager, administrators can manage the lifecycle of virtual workloads
at scale. Supported operations include starting, stopping, and rebooting guests across the inventory
without the need to log in to individual nodes.

Additionally, the platform supports live migration of guests. This capability extends to migrations
between independent clusters, facilitating load balancing and planned maintenance while maintaining
high availability.

Data Collection
---------------

Collecting data like RRD metrics, worker task status, logs, and other operational information is a
primary function of Proxmox Datacenter Manager. The system aggregates metrics to provide insight
into usage, performance, and infrastructure growth.

This allows for introspection into the server fleet, providing a central overview but also allowing
you to explore specific remotes or resources. Dashboards and RRD graphs visualize this data to
assist in detecting trends, optimizing resource allocation, and planning future capacity.

Proxmox VE Remote
-----------------

Proxmox VE remotes integrate virtualization clusters and independent nodes into the central
management view. Once added, the interface displays the hierarchy of hosts, virtual machines,
containers, and storage resources, searchable via the central interface.

Specific management capabilities available for Proxmox VE remotes include:

* **Update Management**: A centralized panel provides an overview of available updates across the
  infrastructure and allows for the rollout of patches directly from the Datacenter Manager
  interface.
* **SDN Capabilities**: Administrators can configure EVPN zones and VNets across multiple remotes to
  manage network overlays and administrative tasks.

Proxmox Backup Server Remote
----------------------------

Proxmox Backup Server instances can be managed as remotes to oversee backup infrastructure alongside
virtualization hosts. The interface provides a consolidated overview of different datastores,
displaying content and storage utilization.

Metrics from Proxmox Backup Server remotes are integrated directly into the central dashboard
widgets, including RRD graphs for performance and usage monitoring.
