:orphan:

======================
proxmox-datacenter-api
======================

Synopsis
========

This daemon is normally started and managed as ``systemd`` service::

 systemctl start proxmox-datacenter-api

 systemctl stop proxmox-datacenter-api

 systemctl status proxmox-datacenter-api

For debugging, you can start the daemon in foreground using::

 sudo -u www-data -g www-data proxmox-datacenter-api

.. NOTE:: You need to stop the service before starting the daemon in foreground.

Description
===========

.. include:: description.rst

.. include:: ../pdm-copyright.rst
