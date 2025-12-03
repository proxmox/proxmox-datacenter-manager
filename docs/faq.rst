FAQ
===

What distribution is Proxmox Datacenter Manager (PDM) based on?
---------------------------------------------------------------

Proxmox Datacenter Manager is based on `Debian GNU/Linux <https://www.debian.org/>`_.


Will Proxmox Datacenter Manager run on a 32-bit processor?
----------------------------------------------------------

Proxmox Datacenter Manager only supports 64-bit CPUs (AMD or Intel). There are no future plans to
support 32-bit processors.


.. _faq-support-table:

How long will my Proxmox Datacenter Manager version be supported?
-----------------------------------------------------------------

.. csv-table::
   :file: faq-release-support-table.csv
   :widths: 30 26 13 13 18
   :header-rows: 1

How can I upgrade Proxmox Datacenter Manager to the next point release?
-----------------------------------------------------------------------

Minor version upgrades, for example upgrading from Proxmox Datacenter Manager in rersion 1.0 to 1.1
or 1.3, can be done just like any normal update.

But, you should still check the `release notes <https://pdm.proxmox.com/roadmap.html>`_ for any
relevant notable, or breaking change.

For the update itself use either the Web UI *Administration -> Updates* panel or through the CLI
with:

.. code-block:: console

  apt update
  apt full-upgrade

.. note:: Always ensure you correctly setup the :ref:`package repositories
   <sysadmin_package_repositories>` and only continue with the actual upgrade if `apt update` did
   not hit any error.

..
 .. _faq-upgrade-major:
 
 How can I upgrade Proxmox Datacenter Manager to the next major release?
 -----------------------------------------------------------------------
 
 Major version upgrades, for example going from Proxmox Datacenter Manager 1.3 to 2.1, are also
 supported.
 They must be carefully planned and tested and should **never** be started without having successfully
 tested backups.
 
 Although the specific upgrade steps depend on your respective setup, we provide general instructions
 and advice of how a upgrade should be performed:
 
 * `Upgrade from Proxmox Datacenter Manager 1 to 2 <https://pdm.proxmox.com/docs/upgrade-todo>`_

.. _faq-subscription:

Is there a dedicated subscription for the Proxmox Datacenter Manager?
---------------------------------------------------------------------

No, there is not. However, your existing Basic or higher subscription for Proxmox VE and Proxmox
Backup Server remotes includes access to the Proxmox Datacenter Manager Enterprise Repository and
support at no extra cost.

.. _faq-enterprise-support:

How can I get Enterprise Support for the Proxmox Datacenter Manager?
--------------------------------------------------------------------

Existing customers with active Basic or higher subscriptions for their Proxmox remotes also gain
access to the Proxmox Datacenter Manager enterprise repository and support.

.. _faq-enterprise-repository:

How can I get access to the Proxmox Datacenter Manager Enterprise Repository?
-----------------------------------------------------------------------------

The Proxmox Datacenter Manager can use the enterprise repository if at least 80% of the configured
remote nodes have a valid Basic or higher subscription.
