.. _automated_installations:

Automated Installations
=======================

The Proxmox Datacenter Manager provides integration with the automated
installer for all Proxmox products.

A detailed documentation of all available options can be found on `our dedicated
wiki page <https://pve.proxmox.com/wiki/Automated_Installation>`_.

.. _autoinst_overview:

Overview
~~~~~~~~

The overview shows all past and ongoing installations done using the Proxmox
Datacenter Manager. It allows access to the raw system information data as sent
by the automated installer before the actual installation, and (if configured)
post-installation notification hook data, containing extensive information about
the newly installed system.

.. _autoinst_answers:

Prepared Answers
~~~~~~~~~~~~~~~~

This view provides an overview over all defined answer files and allows editing,
copying into new answers and deleting them. For a quick overview, it shows
whether an answer is the default and what target filters have been defined for
that particular configuration.

Target filter
^^^^^^^^^^^^^

Target filter allow you to control what systems should match.

`Filters`_ are key-value pairs in the format ``key=format``, with keys being
`JSON Pointers`_, and match systems based the identifying information sent by
the installer as JSON document. An example of such a document is provided `on
the wiki
<https://pve.proxmox.com/wiki/Automated_Installation#System_information_POST_data>`_.

JSON Pointers allow for identifying specific values within a JSON document. For
example, to match only Proxmox VE installations by the product name, a filter
entry like ``/product/product=pve`` can be used.

Values are *globs* and use the same syntax as the automated installer itself.
The following special characters can be used in filters:

* ``?`` -- matches any single character
* ``*`` -- matches any number of characters, can be none
* ``[a]``, ``[abc]``, ``[0-9]`` -- matches any single character inside the
  brackets, ranges are possible

* ``[!a]`` -- negate the filter, any single character but the ones specified

A prepared answer can be also set as default, in which case it will be used if
no other more specific answer matches based on its configured target filters.

.. _autoinst_templating:

Templating
^^^^^^^^^^

Certain fields support templating via `MiniJinja`_ (a Jinja2-inspired templating
engine) and sequential *counters*.
Counters are automatically incremented each time an answer file is served to a
client, allowing for easy provisioning of unique fields, such as per-system
hostnames.

The following counter is automatically defined when creating a new prepared
answer configuration:

* ``installation_nr`` - Counter of the number of installations done with this
  particular answer configuration.

This mechanism allows templating on the following fields for prepared answer
configurations:

* **Administrator email address**
* **Hostname/FQDN**
* **Network IP address (CIDR)**
* **Network gateway**
* **DNS Server address**

The templating context provided for each field contains the `system information
data`_ as sent by the automated installer on answer retrieval, as well as all
template counters.

For example, to provide a unique hostname to each target system, the following
template can be used for the **Hostname/FQDN** field:

.. code-block::

    {{ product.product }}{{ installation_nr }}.example.com

MiniJinja features a wide range of `built-in filters`_, which are enabled by
default, similar to Jinja2.

.. _autoinst_token:

Authentication token management
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

To use the automated installer integration of Proxmox Datacenter Manager, an
installation process must authenticate itself. This also provides for an
additional scoping mechanism for prepared answer configurations.

The automated installer integration uses a dedicated token mechanism, separate
from the normal API tokens. See the example under :ref:`autoinst_preparing_iso`
on how to include such a token in the ISO when preparing it.

.. _autoinst_preparing_iso:

Preparing an ISO
~~~~~~~~~~~~~~~~

To use an installation ISO of a Proxmox product with the Proxmox Datacenter
Manager functionality, the ISO must be appropriately prepared to `fetch an
answer via HTTP`_ from the Proxmox Datacenter Manager using the
``proxmox-auto-install-assistant`` tool, available from the Proxmox VE package
repositories.

The `target URL`_ for the automated installer must point to
``https://<pdm>/api2/json/auto-install/answer``, where ``<pdm>`` is the address
under which the Proxmox Datacenter Manager is reachable from the systems to be
installed.

For example:

.. code-block:: shell

   proxmox-auto-install-assistant prepare-iso /path/to/source.iso \
     --fetch-from http \
     --url 'https://datacenter.example.com/api2/json/auto-install/answer' \
     --cert-fingerprint 'ab:cd:ef:12:34:56:78:90:a1:b2:c3:d4:e5:f6:7a:8b:9c:0d:aa:bb:cc:dd:ee:ff:21:43:65:87:09:af:bd:ce' \
     --answer-auth-token 'mytoken:ee2a5901-1910-4eb0-b0a2-c914f4adbb75'

.. _JSON Pointers: https://www.rfc-editor.org/rfc/rfc6901
.. _fetch an answer via HTTP: https://pve.proxmox.com/wiki/Automated_Installation#Answer_Fetched_via_HTTP
.. _Filters: https://pve.proxmox.com/wiki/Automated_Installation#Filters
.. _target URL: https://pve.proxmox.com/wiki/Automated_Installation#Answer_Fetched_via_HTTP
.. _system information data: https://pve.proxmox.com/wiki/Automated_Installation#System_information_POST_data
.. _MiniJinja: https://docs.rs/minijinja/latest/minijinja/index.html
.. _built-in filters: https://docs.rs/minijinja/latest/minijinja/filters/index.html#built-in-filters
