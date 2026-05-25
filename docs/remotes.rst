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

Connection and Certificate Troubleshooting
-------------------------------------------

Proxmox Datacenter Manager validates a remote's TLS certificate against the system certificate
store. If the remote presents a publicly trusted certificate, for example one issued by Let's
Encrypt through ACME, no further trust configuration is needed and certificate renewals are handled
transparently.

When a remote's certificate is not in the system trust store, as with the default self-signed
Proxmox certificates, Proxmox Datacenter Manager instead pins the fingerprint you accepted when
adding the remote. If such a remote later renews or rotates its certificate, the pinned fingerprint
no longer matches the presented one and every connection to that remote fails. The web interface
and the command-line tools surface this as an error such as:

.. code-block:: text

    connection failed: Could not establish a TLS connection. Check whether the fingerprint matches
    or the certificate on the remote is valid. OpenSSL Error: error:0A000086:SSL routines:
    tls_post_process_server_certificate:certificate verify failed

The most common cause is a legitimate certificate renewal on the remote. It can also indicate an
expired or otherwise invalid certificate or, if the change is unexpected, a man-in-the-middle
attack, so confirm the new certificate through a trusted channel before accepting it.

To recover, re-probe the certificate that the remote currently presents and update the stored
fingerprint:

* In the web interface, open the affected remote and use the **Check Certificate** action. It
  contacts the node, shows the certificate presented now, and lets you update the pinned
  fingerprint once you have confirmed the change. If the remote now uses a certificate that the
  system trust store accepts, you can instead clear the stored fingerprint to rely on that trust.
* On the command line, inspect the presented certificate with
  ``proxmox-datacenter-manager-client remote probe-certificate <remote> <node>``, then store the
  verified value with ``proxmox-datacenter-manager-client remote set-fingerprint <remote> <node>
  <fingerprint>`` (omit the fingerprint to clear the pin).

The system journal on the Proxmox Datacenter Manager host records additional detail, including the
fingerprint that the remote presented and the one that was expected.
