Introduction
============

What is Proxmox Datacenter Manager?
-----------------------------------

Proxmox Datacenter Manager provides a central overview over all your Proxmox Virtual Environment and
Proxmox Backup Server instances and their resources (virtual machines, containers, storages,
datastores, ...) independent of where they reside geographically, supporting both single nodes and
clusters.

Instances (single node setup or a cluster) are called remotes.

The Proxmox Datacenter Manager is also including basic  management, focusing on core maintenance
work, with an quickly accessible escape hatch to directly open the target remotes web-based user
interface to allow managing all properties of the resources while neither crowding but, more
importantly, not add tight coupling between the Proxmox Datacenter Manager and the remotes.


Feature Overview
----------------

The current main features of the Proxmox Datacenter Manager include:

- Connect, display and manage an arbitrary amount of independent nodes or clusters ("Datacenters").
- View the status and load of all resources, which includes nodes, virtual guests, storages,
  datastores and so on.
- Dashboard with overview of remotes and resources, including potential outliers grouped by load
  (CPU & memory) or task type and result.
- Access all task logs centrally.
- Basic management of the resources: shutdown, reboot, start, ...
- Insights into available updates of nodes.
- Remote live-migration of virtual guests between any node, on the same remote (cluster) or to
  another node.
- Support for modern enterprise accesscontrol, including multi-factor authentication, LDAP/AD,
  OpenID Connect SSO, API tokens, paired with a simple but powerfull access control permission list
  system.
- ACME (e.g., through Let's Encrypt) integration.

Technology Stack
----------------

- Server at the backend that the frontend communicates through a REST and JSON based API.
- As much as possible written in the rust programming language.
- A for Proxmox projects standard dual-stack of API daemons. One as main API daemon running as
  unprivileged users and one privileged daemon running as root. Contrary to other projects the
  privileged daemon exclusively listens on a file based UNIX socket, thus restricting attack surface
  even further.
- backend: rust based, reusing PBS REST/API stack were possible

  - no privileged (root) operations required, so a single daemon is enough
  - TCP port 443 (default HTTPS one)

- fronted: the Yew and Rust based TODO:

.. _get_help:

Getting Help
------------

.. _get_help_enterprise_support:

Enterprise Support
^^^^^^^^^^^^^^^^^^

Existing customers with an active Basic or higher subscription for their Proxmox remotes also gain
access to the Proxmox Datacenter Manager Enterprise-Repository and support.

For more information please visit www.proxmox.com or contact sales@proxmox.com.

Community Support Forum
^^^^^^^^^^^^^^^^^^^^^^^

We always encourage our users to discuss and share their knowledge using the `Proxmox Community
Forum`_. The forum is moderated by the Proxmox support team.  The large user base is spread out all
over the world. Needless to say that such a large forum is a great place to get information.

Mailing Lists
^^^^^^^^^^^^^

Proxmox Datacenter Manager is fully open-source and contributions are welcome! The Proxmox
Datacenter Manager development mailing list acts as the primary communication channel for
developers:

:Mailing list for developers: `PDM Development List`_

Bug Tracker
^^^^^^^^^^^

Proxmox runs a public bug tracker at `<https://bugzilla.proxmox.com>`_. If an issue appears, file
your report there. An issue can be a bug, as well as a request for a new feature or enhancement. The
bug tracker helps to keep track of the issue and will send a notification once it has been solved.

License
-------

|pdm-copyright|

This software is written by Proxmox Server Solutions GmbH <support@proxmox.com>

Proxmox Datacenter Manager is free and open source software: you can use it,
redistribute it, and/or modify it under the terms of the GNU Affero General
Public License as published by the Free Software Foundation, either version 3
of the License, or (at your option) any later version.

This program is distributed in the hope that it will be useful, but
``WITHOUT ANY WARRANTY``; without even the implied warranty of
``MERCHANTABILITY`` or ``FITNESS FOR A PARTICULAR PURPOSE``.  See the GNU
Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see AGPL3_.
