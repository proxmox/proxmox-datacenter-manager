Graphical User Interface
========================

Proxmox Datacenter Management offers an integrated, web-based interface to manage the server.
This means that you can carry out all administration tasks through your web browser, and that you
don't have to worry about installing extra management tools. The web interface also provides a
built-in console, so if you prefer the command line or need some extra control, you have this
option.

The web interface can be accessed via https://youripaddress:8443.
The default login is `root`, and the password is either the one specified during the installation
process or the password of the root user, in case of installation on top of Debian.


Features
--------

* Modern management interface for Proxmox Datacenter Manager
* Customizable Views.
* Management of remotes, resources, users, permissions, etc.
* Secure HTML5 console
* Support for multiple authentication sources
* Support for multiple languages
* Based on Yew, a modern Rust framework for creating multi-threaded, front-end web apps with
  WebAssembly.

Login
-----

..
  .. image:: images/screenshots/pdm-gui-login-window.png
  :target: _images/pdm-gui-login-window.png
  :align: right
  :alt: Proxmox Datacenter Manager login window

When you connect to the web interface, you will first see the login window.  Proxmox Datacenter
Manager supports various languages and authentication back ends (*Realms*), both of which can be
selected here.

.. note:: For convenience, you can save the username on the client side, by selecting the "Save User
   name" checkbox at the bottom of the window.

User Interface Overview
-----------------------

.. todo::
   describe basic PDM UI overview with all panels and views listed, see PBS for a basic idea.
