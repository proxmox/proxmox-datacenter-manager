.. _sysadmin_package_repositories:

Debian Package Repositories
---------------------------

All Debian based systems use APT_ as a package management tool. The lists of repositories are
defined in ``/etc/apt/sources.list`` and the ``.list`` or ``.sources`` files found in the
``/etc/apt/sources.d/`` directory. Updates can be installed directly with the ``apt`` command-line
tool, or via the GUI.

.. _package_repos_repository_formats:

Repository Formats
~~~~~~~~~~~~~~~~~~

APT_ repositories can be configured in two distinct formats, the old single line format and the
newer deb822 format. No matter what format you choose, ``apt update`` will fetch the information
from all configured sources.

Single Line
^^^^^^^^^^^

Single line repositories are defined in ``.list`` files list one package repository per line, with
the most preferred source listed first. Empty lines are ignored and a ``#`` character anywhere on a
line marks the remainder of that line as a comment.

deb822 Style
^^^^^^^^^^^^

The newer deb822 multiline format is used in ``.sources`` files. Each repository consists of a
stanza with multiple key value pairs. A stanza is simply a group of lines. One file can contain
multiple stanzas by separating them with a blank line. You can still use ``#`` to comment out lines.

.. note:: Modernizing your repositories is recommended under Debian Trixie, as ``apt`` will complain
   about older repository definitions otherwise. You can run the command ``apt modernize-sources``
   to modernize your existing repositories automatically.

.. _package_repos_debian_base_repositories:

Debian Base Repositories
~~~~~~~~~~~~~~~~~~~~~~~~

You will need a Debian base repository as a minimum to get updates for all packages provided by
Debian itself:

.. code-block:: debian.sources
  :caption: File: ``/etc/apt/sources.list.d/debian.sources``

  Types: deb
  URIs: http://deb.debian.org/debian/
  Suites: trixie trixie-updates
  Components: main contrib non-free-firmware
  Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg

  Types: deb
  URIs: http://security.debian.org/debian-security/
  Suites: trixie-security
  Components: main contrib non-free-firmware
  Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg

In addition, you need a package repository from Proxmox to get Proxmox Datacenter Manager updates.

..
 .. image:: images/screenshots/pdm-gui-administration-apt-repos.png
  :target: _images/pdm-gui-administration-apt-repos.png
  :align: right
  :alt: APT Repository Management in the Web Interface

.. _sysadmin_package_repos_enterprise:

`Proxmox Datacenter Manager`_ Enterprise Repository
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

This is the stable, recommended repository. It is available for all users fullfilling the Proxmox
Datacenter Manager :ref:`subscription requirements <faq-enterprise-repository>`. 
It contains the most stable packages, and is suitable for production use. The ``pdm-enterprise``
repository is enabled by default:

.. code-block:: debian.sources
  :caption: File: ``/etc/apt/sources.list.d/pdm-enterprise.sources``

  Types: deb
  URIs: https://enterprise.proxmox.com/debian/pdm
  Suites: trixie
  Components: pdm-enterprise
  Signed-By: /usr/share/keyrings/proxmox-archive-keyring.gpg

  To never miss important security fixes, the superuser (``root@pam`` user) is notified via email
  about new packages as soon as they are available. The change-log and details of each package can
  be viewed in the GUI (if available).

Please note that you need a valid subscription key to access this repository. More information
regarding enterprise repository access can be found at
https://www.proxmox.com/en/proxmox-datacenter-manager/pricing

.. note:: You can disable this repository by adding the line ``Enabled: false`` to the stanza.

`Proxmox Datacenter Manager`_ No-Subscription Repository
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

As the name suggests, you do not need a subscription key to access this repository. It can be used
for testing and non-production use. It is not recommended to use it on production servers, because
these packages are not always heavily tested and validated.

We recommend to configure this repository in ``/etc/apt/sources.list.d/proxmox.sources``.

.. code-block:: debian.sources
  :caption: File: ``/etc/apt/sources.list.d/proxmox.sources``

  Types: deb
  URIs: http://download.proxmox.com/debian/pdm
  Suites: trixie
  Components: pdm-no-subscription
  Signed-By: /usr/share/keyrings/proxmox-archive-keyring.gpg

`Proxmox Datacenter Manager`_ Test Repository
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

This repository contains the latest packages and is heavily used by developers to test new features.

.. .. warning:: the ``pdm-test`` repository should (as the name implies) only be used to test new
   features or bug fixes.

You can access this repository by adding the following stanza to
``/etc/apt/sources.list.d/proxmox.sources``:

.. code-block:: debian.sources
  :caption: sources.list entry for ``pdm-test``

  Types: deb
  URIs: http://download.proxmox.com/debian/pdm
  Suites: trixie
  Components: pdm-test
  Signed-By: /usr/share/keyrings/proxmox-archive-keyring.gpg

.. _package_repos_secure_apt:

SecureApt
~~~~~~~~~

The `Release` files in the repositories are signed with GnuPG. APT is using these signatures to
verify that all packages are from a trusted source.

If you install Proxmox Datacenter Manager from an official ISO image, the verification key is already
installed.

If you install Proxmox Datacenter Manager on top of Debian, download and install the key with the
following commands:

.. code-block:: console

 # wget https://enterprise.proxmox.com/debian/proxmox-archive-keyring-trixie.gpg -O /usr/share/keyrings/proxmox-archive-keyring.gpg

.. note:: The `wget` command above adds the keyring for Proxmox releases based on Debian Trixie.
   Once the `proxmox-archive-keyring` package is installed, it will manage this file. At that point,
   the hashes below may no longer match the hashes of this file, as keys for new Proxmox releases
   get added or removed. This is intended, `apt` will ensure that only trusted keys are being used.
   **Modifying this file is discouraged once `proxmox-archive-keyring` is installed.**

Verify the SHA256 checksum afterwards with the expected output below:

.. code-block:: console

 # sha256sum /usr/share/keyrings/proxmox-archive-keyring.gpg
 136673be77aba35dcce385b28737689ad64fd785a797e57897589aed08db6e45 /usr/share/keyrings/proxmox-archive-keyring.gpg

and the md5sum, with the expected output below:

.. code-block:: console

 # md5sum /usr/share/keyrings/proxmox-archive-keyring.gpg
 77c8b1166d15ce8350102ab1bca2fcbf /usr/share/keyrings/proxmox-archive-keyring.gpg

.. note:: Make sure that the path that you download the key to, matches the path specified in the
   ``Signed-By:`` lines in your repository stanzas from above.


.. todo
 .. _node_options_http_proxy:
 
 Repository Access Behind HTTP Proxy
 ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
 
 Some setups have restricted access to the internet, sometimes only through a central proxy. You can
 setup a HTTP proxy through the Proxmox Datacenter Manager's web-interface in the `Configuration ->
 Authentication` tab.
 
 Once configured this proxy will be used for apt network requests and for checking a Proxmox Datacenter Manager
 Server support subscription.
 
 Standard HTTP proxy configurations are accepted, `[http://]<host>[:port]` where the `<host>` part
 may include an authorization, for example: `http://user:pass@proxy.example.org:12345`
