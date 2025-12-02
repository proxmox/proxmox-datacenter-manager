:orphan:

=================================
proxmox-datacenter-privileged-api
=================================

Synopsis
========

This daemon is normally started and managed as ``systemd`` service::

 systemctl start proxmox-datacenter-privileged-api

 systemctl stop proxmox-datacenter-privileged-api

 systemctl status proxmox-datacenter-privileged-api

For debugging, you can start the daemon in foreground as root user through running::

 proxmox-datacenter-privileged-api

.. NOTE:: You need to stop the service before starting the daemon in foreground.

Description
===========

.. include:: description.rst

.. include:: ../pdm-copyright.rst
