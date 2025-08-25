.. _install_pdm:

Installation
============

`Proxmox Datacenter Manager`_ can either be installed with a graphical
installer or on top of Debian_ from the provided package repository.

.. include:: system-requirements.rst

.. todo
  .. include:: installation-media.rst

Using our provided disk image (ISO file) is the recommended installation
method, as it includes a convenient installer, a complete Debian system as well
as all necessary packages for the Proxmox Datacenter Manager.

Once you have created an :ref:`installation_medium`, the booted :ref:`installer
<using_the_installer>` will guide you through the setup process. It will help
you to partition your disks, apply basic settings such as the language, time
zone and network configuration, and finally install all required packages
within minutes.

As an alternative to the interactive installer, advanced users may wish to
install Proxmox Datacenter Manager :ref:`unattended <install_pdm_unattended>`.

With sufficient Debian knowledge, you can also install Proxmox Datacenter
Manager :ref:`on top of Debian <install_pdm_on_debian>` yourself.

.. todo
  .. include:: using-the-installer.rst

.. _install_pdm_unattended:

Install Proxmox Datacenter Manager Unattended
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

It is possible to install Proxmox Datacenter Manager automatically in an
unattended manner. This enables you to fully automate the setup process on
bare-metal. Once the installation is complete and the host has booted up,
automation tools like Ansible can be used to further configure the installation.

The necessary options for the installer must be provided in an answer file.
This file allows the use of filter rules to determine which disks and network
cards should be used.

To use the automated installation, it is first necessary to prepare an
installation ISO.  For more details and information on the unattended
installation see `our wiki
<https://pve.proxmox.com/wiki/Automated_Installation>`_.

.. _install_pdm_on_debian:

Install Proxmox Datacenter Manager on Debian
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Proxmox ships as a set of Debian packages which can be installed on top of a
standard Debian installation. After configuring the
:ref:`sysadmin_package_repositories`, you need to run:

.. code-block:: console

  # apt update
  # apt install proxmox-datacenter-manager proxmox-datacenter-manager-ui

The above commands keep the current (Debian) kernel and install a minimal set
of required packages.

You can install the Proxmox default kernel with ZFS support by using:

.. code-block:: console

  # apt update
  # apt install proxmox-default-kernel

..
  add meta package

.. caution:: Installing Proxmox Datacenter on top of an existing Debian_
  installation looks easy, but it assumes that the base system and local
  storage have been set up correctly. In general this is not trivial, especially
  when LVM_ or ZFS_ is used. The network configuration is completely up to you
  as well.

.. Note:: You can access the web interface of the Proxmox Datacenter Manager with
   your web browser, using HTTPS on port 8443. For example at
   ``https://<ip-or-dns-name>:8443``
