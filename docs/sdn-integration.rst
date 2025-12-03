.. _sdn-integration:

SDN Integration
---------------

The Proxmox Datacenter allows managing SDN zones and vnets across multiple remotes and provides an
overview of the current state of SDN entities.

.. _status_overview:

Status Overview
~~~~~~~~~~~~~~~

The status overview shows the current status (available / error / unknown) of all zones on all
remotes. This is equivalent to the status shown in the SDN overview of the Proxmox VE Web UI. A
summary is also shown on the dashboard, allowing users to quickly identify if there are any
erroneous SDN zones on any remote.

.. _evpn_integration:

EVPN Integration
~~~~~~~~~~~~~~~~

The EVPN overview shows an aggregated overview of the contents of EVPN zones / routing table
instances of all configured clusters.


.. note:: Currently, the integration operates under the assumption that EVPN controllers with the
   same ASN are interconnected and part of the same overlay network. Zones and Vnets with the same
   ASN:VNI tag will get automatically merged in the overview trees.}}

The EVPN integration respects the ‘Route Target Import’ field of an EVPN zone and assumes any Zones
/ Vnets with that Route Target are imported as well.

Defintions
^^^^^^^^^^

Currently, the SDN stack in Proxmox VE uses the terms Zones and VNets, which are specific to the
Proxmox VE stack. The following defintions try to make the relationship of those entities to the
more commonly used definitions in RFC 7432 and RFC 9136 clearer:

A EVPN zone represents a routing table instance (identified by its ASN:VNI tag). This is also known
as an IP-VRF It is associated with a VXLAN VNI (the VRF-VXLAN tag of a zone) and also referred to as
L3VNI.

A vnet in an EVPN zone represents a bridging table (identified by its ASN:VNI tag). This is also
known as a MAC-VRF. One IP-VRF can contain multiple MAC-VRFs. Analogous to a EVPN zone it is
associated with a VXLAN VNI (the tag of a vnet) and also referred to as L2VNI.

Remotes
'''''''

This view provides an overview of which zones are available on a remote and which vnets it contains.
It shows the vnets that are locally configured on that remote, as well as the vnets that get
imported either automatically (due to matching ASN:VNI tags) or manually (due to being specified in
the ‘Route Target Import’ setting). Vnets that are not local to a remote are shown slightly greyed
out, so they can be distinguished easily.

It contains the following columns:

*  Name: The name of the remote / zone / vnet
*  L3VNI: The VRF-VXLAN tag configured in the zone
*  L2VNI: The tag configured in the vnet
*  External: Whether this VNet is locally configured or from another remote
*  Imported: Whether this VNet was manually imported, due to a respective ‘Route Target Import’
   entry

.. _ip_vrf:

IP-VRF
''''''

This view provides an overview of all available IP-VRFs and their contents. This view shows only
VNets that are naturally part of an IP-VRF due to their zone having the same ASN:VNI combination. It
can be used to see which VNets would get imported when specifying the respective ASN:VNI in the
‘Route Target Import’ field.

It contains the following columns:

*  Name: The name of the remote / zone / vnet
*  ASN: The VRF-VXLAN tag configured in the zone
*  VNI: The L3VNI (for zones) or L2VNI (for vnets)
*  Zone: The name of the zone that contains the vnet
*  Remote: The name of the remote that contains the zone (and therefore vnet).
