Roadmap
=======

For completed items, see the release notes for previous versions below.

The items below describe development directions and priorities. Not all are planned for immediate
delivery. Some are long-term efforts spanning multiple major releases, and items may be
reprioritized based on engineering capacity, enterprise customer requirements, and community
feedback.

* **Resource organization and access control:**

  - Folders as a first-class hierarchical entity: each remote has a single home folder, and access
    can be delegated per folder.
  - Tags as a flat labeling scheme, independent of the folder hierarchy, where a resource can carry
    several tags.
  - A finer-grained ACL schema: Resource\* roles that separate managing a resource from modifying
    its definition, folders acting as ACL anchors, and closing the remaining cross-cluster
    delegation gaps.

* **Guest and resource management:**

  - Bulk actions on virtual guests across the selection in the central guest list, building on the
    cluster-wide bulk-action endpoints that the underlying Proxmox products provide and degrading
    gracefully where they are missing; the same operations also through the admin CLI, with
    aggregated progress and per-item results.
  - More information in resource overviews and detail panels, and more options in the migration
    dialog, such as a bandwidth limit and an online/offline preference.
  - Management of further core configurations of remote resources: a backup-job overview with
    last-execution status, and basic guest configuration beyond snapshots, lifecycle and migration.
    Proxmox Datacenter Manager keeps linking out to the remote's full interface for anything complex.
  - A console for remote resources (nodes and guests).

* **Remotes and onboarding:**

  - Simplify adding remotes through copy-and-paste Quick-Add information, mirroring the Proxmox VE
    cluster-join flow but independent of cluster communication and working on single nodes too.
  - Handle multi-factor authentication for the initial Probe Remote connection.

* **Networking:**

  - First-class SDN integration, further phases: stretch EVPN VNets across clusters, support
    multiple VRFs across clusters, and automate route-target import and export.
  - Firewall management, likely landing together with the SDN work.

* **Notifications:**

  - Standard system, update and task notifications for the Proxmox Datacenter Manager node itself.
  - Evaluate whether Proxmox Datacenter Manager can act as a notification target for remotes.

* **Search and usability:**

  - More expressive search and filter syntax: wildcards for view filter values, richer expressions
    in the resource search bar, and additional category keywords.
  - Polish error messages and handling across the web interface, the CLI and the API, with richer
    presentation of API errors and more actionable guidance on common failure modes.

* **Further integrations and resilience:**

  - Integration of further Proxmox projects: Proxmox Mail Gateway as a remote, and deeper Proxmox
    Backup Server integration including job status, datastore content browsing and prune retention.
  - Off-site replication copies of virtual guests for manual recovery on a datacenter failure (not
    HA).
  - Evaluate an active-standby architecture for Proxmox Datacenter Manager itself, to avoid a single
    point of failure; two instances side-by-side already cover this in practice, at the cost of
    doubled metric collection.

.. _release_history:

Release History
---------------

.. _proxmox_datacenter_manager_1.1:

Proxmox Datacenter Manager 1.1
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

**Released 28. May 2026**

-  Based on Debian Trixie (13.5)
-  Latest 7.0 Kernel as stable default
-  ZFS 2.4.2

.. _features_highlights_1.1:

Features (Highlights)
^^^^^^^^^^^^^^^^^^^^^^

-  Integration of automated installation functionality.

      Proxmox Datacenter Manager can now manage answer file configurations and serve them to
      remotes, allowing for centralized management of installation parameters.

      Installation progress can be tracked from within Proxmox Datacenter Manager's web interface.

      A token system protects the installation process, enhancing overall security.

      Prepared answers can carry an optional Proxmox subscription key, so a new node registers its
      subscription automatically without an extra operator step.

-  Subscriptions can now be centrally managed through a subscription registry in Proxmox Datacenter
   Manager.

      Administrators can configure a central pool of subscription keys and assign them to specific
      remotes.

      Subscriptions can be assigned to or cleared from remotes.

      Assignments can also be suggested automatically to ease handling large numbers of remotes.

-  Ceph clusters on connected hyper-converged Proxmox VE remotes can now be monitored.

      The status of multiple Ceph clusters can be seen at a glance from a single panel.

      The health status, capacity and performance, as well as the status of monitors, managers,
      OSDs and flags, can be inspected.

-  Metrics are now collected for the Proxmox Datacenter Manager host itself.

      Administrators can tell at a glance how utilized their Proxmox Datacenter Manager host is.

-  New widgets visualize the location of remotes on a map and use gauges to show the utilization of
   resources.

      Locations should be set via the node or datacenter options on Proxmox VE remotes.

      For Proxmox Backup Server remotes, the location can be set under Configuration → Other →
      Location.

-  First step toward central guest management across connected remotes.

      A new cross-remote view lists all QEMU and LXC guests as a flat sortable table or as a tree
      grouped by remote, with text filtering and the most common per-guest actions readily
      available.

      Snapshot management is centrally available for QEMU and LXC guests: list a guest's snapshots
      in a parent/child tree and create, roll back, delete or edit a snapshot's description, either
      from the central guest list or from the per-guest detail panel.

      An explicit Resume action is offered for paused or suspended QEMU guests, complementing the
      existing start, stop and shutdown actions.

      This is the initial iteration of central guest management; expect further day-to-day tasks to
      be integrated in upcoming releases.

.. _changelog_overview_1.1:

Changelog Overview
^^^^^^^^^^^^^^^^^^

.. _enhancements_in_the_web_interface_gui_1.1:

Enhancements in the Web Interface (GUI)
'''''''''''''''''''''''''''''''''''''''

- Add RRD graphs for the Proxmox Datacenter Manager host to the node status panel.
- Add gauge chart widgets for CPU, memory and storage utilization for views and the default
  dashboard.
- A new widget allows visualizing remotes on a map.
- Add a cross-remote guest list showing all QEMU and LXC guests across connected Proxmox VE
  remotes, either as a flat sortable table or as a tree grouped by remote, with text filtering and
  the common per-guest actions.
- Add central snapshot management for QEMU and LXC guests, accessible from the central guest list
  or as a Snapshots tab in the per-guest detail panels (`issue 7207
  <https://bugzilla.proxmox.com/show_bug.cgi?id=7207>`__).
- Offer an explicit Resume action for paused or suspended QEMU guests, complementing the existing
  start, stop and shutdown actions.
- Add a certificate check and re-pin dialog for remotes: after a TLS certificate rotation, the
  dialog re-probes the configured nodes, lets the operator accept the new fingerprint per node or
  clear the stored pin, and applies the changes as one batch. It is reachable from the remotes list
  and offered directly on a Proxmox VE or Proxmox Backup Server remote whose connection is
  currently failing.
- The notes field now supports a subset of `MathML
  <https://developer.mozilla.org/en-US/docs/Web/MathML>`__ to allow presenting calculations in
  proper mathematical notation.
- Derive IDs for headings in the notes field automatically to enable intra-document links.
- Add a button to download the system report on the node status page.
- Fix an issue where a shared storage in a cluster was counted multiple times toward the storage
  capacity calculations (`issue 7135 <https://bugzilla.proxmox.com/show_bug.cgi?id=7135>`__).
- Allow refreshing remote tasks in the task viewer.
- Use the IEC standard for showing drive space.
- Improve adding permissions by listing ACL paths for specific resources and views as well.
- Fix an issue that would prematurely log out users right after a fresh login.
- Fix an issue that would clear filter value fields when editing an existing view.
- Allow showing views even if they contain unknown widgets, which can happen during updates.
- Force a refresh after a view was edited to load potentially missing data immediately.
- Add a message to the top entities widget when it is empty, explaining why nothing can be
  displayed.
- Show an explanatory message when a view is empty.
- Names of views are now validated in the UI.
- Improve the subscription key pool experience: validate keys when adding them, allow per-row key
  overrides and row deselection in the Auto-Assign proposal, surface errors when removing a key
  fails, and filter the node status tree by status.
- When migrating a guest to a new cluster, query the remote cluster for its next free VMID and use
  it to pre-fill the migration dialog.
- Properly handle the OpenID redirection authorization, improving compatibility with certain OpenID
  providers, for example Google (`issue 7290
  <https://bugzilla.proxmox.com/show_bug.cgi?id=7290>`__).
- Allow the UI to render the add-user dialog properly depending on the realm.
- Avoid an issue that prevented users from being edited when the "expire" field was set.
- Use a password field for the OpenID client key field.
- Allow changing an LDAP realm to use anonymous search.
- Allow a realm sync dialog to be submitted even if the default values were not changed.
- Improve and extend UI routing to include tabs in a Proxmox VE remote panel and avoid unnecessary
  history entries.
- Improve the experience when refreshing remote update and subscription status by disabling buttons
  and showing a loading bar.
- Disable the datastore content view and show an appropriate message if the datastore is in the
  'offline' maintenance mode.
- Fix a flaw where an attacker could manipulate a panic display to run arbitrary code in a user's
  browser context.
- Harden the Markdown viewer's HTML sanitizer by also encoding the 'base' tag and fixing tag-name
  comparisons that previously did not match uppercase variants.
- Add proper descriptions for tasks native to Proxmox Datacenter Manager.
- Prevent the browser from reloading the page when adding a remote through the wizard.
- Fix an issue that displayed the wrong timezone for Kyiv (`issue 7141
  <https://bugzilla.proxmox.com/show_bug.cgi?id=7141>`__).
- Improve adding Proxmox Backup Server remotes by normalizing the hostname in the remote addition
  wizard.
- Properly display the endpoint URL when showing the automated installer preparation command.
- Fix an issue where longer combo-boxes could be overlapped by their picker.
- Use "System Log" instead of "Syslog" in the web interface.
- Improved and updated translations for many languages, including:

   - Arabic
   - Brazilian Portuguese
   - Croatian
   - Czech
   - French
   - German
   - Hungarian
   - Italian
   - Japanese
   - Korean
   - Polish
   - Russian
   - Simplified Chinese
   - Spanish
   - Swedish
   - Traditional Chinese
   - Turkish
   - Ukrainian

.. _resource_management_1.1:

Resource Management
'''''''''''''''''''

- Allow Proxmox Datacenter Manager to serve answer files for automated installations.

      Installations can be tracked through Proxmox Datacenter Manager.

      Options for the automated installation can be managed through the web interface.

      An additional token system lets new installations authenticate against Proxmox Datacenter
      Manager, improving security.

      Prepared answers can optionally carry a Proxmox subscription key, so a new node registers its
      subscription automatically on first boot.

- When migrating a resource without a specific target endpoint across clusters, Proxmox Datacenter
  Manager now prefers hosts that are known to be reachable.

      Previously the first configured host of the target cluster was chosen.

- Add a tab panel for tasks in the Proxmox Backup Server remote panel.
- Take the first step toward central guest management: list and operate on QEMU and LXC guests
  across connected remotes from a single panel, manage their snapshots centrally, and resume paused
  or suspended QEMU guests. See the highlights section for details.

.. _remotes_management_1.1:

Remotes Management
''''''''''''''''''

- Proxmox Datacenter Manager can now manage subscriptions for connected remotes.

      This allows administrators to configure a centrally managed pool of subscriptions.

      Subscriptions can be applied to and cleared from a remote via the web interface.

      Subsequent fixes ensure that key removal tolerates a corrupt shadow file and finishes the
      authoritative pool config removal first.

- Allow monitoring Ceph clusters on connected hyper-converged Proxmox VE remotes.

      A new panel tells the status of multiple Ceph clusters at a glance.

      Further details such as the current health status, capacity, performance, and the status of
      monitors, managers, OSDs or flags can be inspected.

- Re-probe and re-pin a remote's TLS certificate from the UI or CLI after a certificate rotation,
  instead of having to remove and re-add the remote.

      A new dialog re-probes the configured nodes, lets the operator accept the new fingerprint per
      node or clear the stored pin, and applies the changes as a single batch.

      This is a stop-gap until all Proxmox products support staged certificate rotation.

- Add proper support for different realm types when adding Proxmox Backup Server remotes.
- Allow removing the token generated for Proxmox Datacenter Manager when removing a remote (`issue
  6914 <https://bugzilla.proxmox.com/show_bug.cgi?id=6914>`__).
- Properly drop remotes from the cache if they have vanished (`issue 7120
  <https://bugzilla.proxmox.com/show_bug.cgi?id=7120>`__).

      This fixes an issue where a node could be removed from the cluster but still show up in
      Proxmox Datacenter Manager.

- When querying the snapshots of a datastore fails, more appropriate error messages are now passed
  through.

.. _backend_improvements_1.1:

Backend Improvements
''''''''''''''''''''

- Allow querying Proxmox Datacenter Manager host metrics.
- Return global CPU, memory and storage statistics when querying the status of a resource.
- Add an API endpoint for refreshing the task cache for a single remote or all remotes.
- Add support for OpenID audiences (`issue 5076
  <https://bugzilla.proxmox.com/show_bug.cgi?id=5076>`__).

      This is required to support certain OpenID providers like `Zitadel <https://zitadel.com/>`__.

- Fix a bug that prevented users from an OpenID realm from being added manually (`issue 7182
  <https://bugzilla.proxmox.com/show_bug.cgi?id=7182>`__).
- Ensure that freshly fetched subscription data is returned when the ``max_age`` parameter is set
  to ``0``.
- Add better ACME support for servers returning status code ``204`` when requesting a nonce (`issue
  6939 <https://bugzilla.proxmox.com/show_bug.cgi?id=6939>`__).
- Rework the resource, subscription and remote-update caches onto a shared, persistent key-value
  store, making the cached data self-healing and more consistent across these subsystems.
- Move the views API handlers into the proxy process and tighten their schema and error messages,
  including a clearer error for view layouts that contain unknown widgets.
- Relax the privilege required to list certificate information from ``Modify`` to ``Audit``,
  allowing read-only access to certificate state without granting modification rights.
- The backend package now recommends ``ifupdown2``.
- Querying the host's certificate information is now permitted for any logged-in user.

.. _command_line_interface_enhancements_1.1:

Command Line Interface Enhancements
'''''''''''''''''''''''''''''''''''

- Allow managing guest snapshots via the CLI.
- Add a sub-command to query the support status of a Proxmox Datacenter Manager host and include it
  in the system report.
- Allow managing ACME settings through the ``proxmox-datacenter-manager-admin`` CLI (`issue 7179
  <https://bugzilla.proxmox.com/show_bug.cgi?id=7179>`__).
- Allow specifying the target ID when migrating a VM or container between Proxmox VE remotes via the
  CLI.
- Improve CLI completion in bash.
- Allow (re-)probing and managing the fingerprint of remotes via the CLI.
- Fix an issue that prevented zsh completions from being generated properly.
- Improve FIDO authenticator support when multiple devices are connected via the CLI client.

.. _known_issues_breaking_changes_1.1:

Known Issues & Breaking Changes
'''''''''''''''''''''''''''''''

- Removed the ``/nodes/localhost/rrdata`` API handler. The impact should be minimal, as it always
  failed previously.

.. _proxmox_datacenter_manager_1.0:

Proxmox Datacenter Manager 1.0
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

**Released 04. December 2025**

-  Based on Debian Trixie (13.2)
-  Latest 6.17.2-2 Kernel as stable default
-  ZFS 2.3.4

.. _features_highlights_1.0:

Features (Highlights)
^^^^^^^^^^^^^^^^^^^^^

-  First stable release.
-  Add Support for Proxmox Backup Server.

      Allows managing Proxmox Backup Server remotes similarly to Proxmox VE remotes.

      An overview shows the contents of different datastores alongside RRD graphs.

      The dashboard now includes metrics from Proxmox Backup Server
      remotes in its widgets.

-  Custom Views allow creating custom overviews of all remotes.

      Views allow providing an overview similar to the dashboard but with custom layouts and
      filters.

      The data a view has access to can be filtered by remotes, resources, resource type, or tags.

      Users can be granted access to specific views without granting them access to the underlying
      remotes or resources directly.

-  Add support for accessing a remote's shell.

      With the release of Proxmox VE 9.1 and Proxmox Backup Server 4.1, API tokens can now request
      shell access.

      Proxmox Datacenter Manager leverages this capability to allow accessing shells of supported
      remotes through one unified control plane.

-  Global package repository and pending updates status.

      A new panel offers an overview of the status of all package repositories and available updates
      from remotes.

      Updates can be applied from within Proxmox Datacenter Manager by leveraging the new remote
      shell features.

-  Improved authentication functionality allows easier user management.

      Proxmox Datacenter Manager now supports LDAP, Active Directory and OpenID Connect realms for
      authentication.

      Tokens allow granting more fine-grained access to other applications that want to use the API.

.. _changelog_overview_1.0:

Changelog Overview
^^^^^^^^^^^^^^^^^^

.. _enhancements_in_the_web_interface_gui_1.0:

Enhancements in the Web Interface (GUI)
'''''''''''''''''''''''''''''''''''''''

- Views allow for custom overviews of all or a specific set of remotes and resources.

    A drag and drop editor allows easy adjustment of any widget.

    The data that a view displays can be easily tweaked via filters.

    The default dashboard is provided as an initial view.

- Add a panel for adding and managing new realms.

    LDAP, Active Directory, and OpenID connect realms can be added to allow easy authentication
    management.

    LDAP and Active Directory realms can also be synced using this panel.

    These realms can be configured as default realms. Default realms are used by the login mask by
    default instead of the PAM realm.

- Add a panel that allows managing tokens and allow configuring ACL entries for tokens.
- Enable the documentation button in the top navigation bar.
- Link to proper builtin documentation instead of Beta documentation.
- A new tab under the “Administration” menu shows the status of the Proxmox Datacenter Manager host
  and allows shutting it down or rebooting it (`issue 6300
  <https://bugzilla.proxmox.com/show_bug.cgi?id=6300>`__).
- Add presentation of subscription status of remotes:

   - The remote subscription status can now be refreshed manually.
   - Remote subscriptions can now be inspected by clicking on the subscription status panel in the
     dashboard (`issue 6797 <https://bugzilla.proxmox.com/show_bug.cgi?id=6797>`__).
   - Add a “Details” button in the subscription panel to show the subscription status dialog.

- Tags of Proxmox VE guests are now shown in the resource tree.
- Add a panel displaying the notes of Proxmox VE nodes and datacenters.
- Align available functionality for Proxmox VE guests with the version of the remote.
- Allow the UI to render components based on the user's privileges.
- Remove a duplicate entry from the permission path selector.
- Improve Proxmox Backup Server datastore panel by making the labels translatable.
- Proxmox Backup Server remote tasks are handled correctly now.
- The remote setup wizard now validates the remote's ID.
- Add a title to the Proxmox VE remote tree toolbar.
- Remove unnecessary “enabled” status line for Proxmox VE storages.
- Do not show storage entries in the Proxmox VE resource tree unconditionally.
- Add a button to allow navigating to a Proxmox VE guest directly in their respective details views.
- Tabs for Proxmox VE and Proxmox Backup Server remotes now properly support history navigation.
- Add a window to display and copy the system report.
- A new panel shows the Proxmox Datacenter Manager's subscription information.
- When adding a remote via the setup wizard, the token name will now include the Proxmox Datacenter
  Manager host. This ensures multiple instances of Proxmox Datacenter Manager can be connected to
  the same remote.
- Mask remote shells if the remote version is too old to support the feature.
- Fix an issue that prevented realms from being deleted (`issue 6885
  <https://bugzilla.proxmox.com/show_bug.cgi?id=6885>`__).
- Fix an issue where updating a storage's status did not trigger correctly.
- Fix an issue that prevented users in the PAM realm from being added as Proxmox Datacenter Manager
  users (`issue 6787 <https://bugzilla.proxmox.com/show_bug.cgi?id=6787>`__).
- The UI now properly respects the text direction for Arabic, Persian (Farsi) and Hebrew.
- Fix an issue where the resource tree for a search was not loaded correctly.
- Make navigating to network resources work properly again.
- Updated translations, among others:

   -  Czech
   -  French
   -  Georgian
   -  German
   -  Italian
   -  Japanese
   -  Korean
   -  Polish
   -  Spanish
   -  Swedish
   -  Traditional Chinese
   -  Ukrainian

.. _resource_management_1.0:

Resource Management
'''''''''''''''''''

- Remote shells for Proxmox VE and Proxmox Datacenter Manager can be accessed directly from the UI.

    Proxmox VE remotes make this shell available through a new tab in a node's details panel.

    For Proxmox Backup Server remotes, a button was added in the top bar of the overview to open a
    new window with the shell.

- A new panel shows hardware and options configuration for Proxmox VE remote's guests.
- Make search terms case-insensitive.
- Allow searching for resources by remote type.
- Extend matching to properties of resources.
- Views can now be searched for.

    Resources can specify a list of properties that can then be searched for.

- Add support for new Proxmox VE network resource type.
- Allow searching for resources by network type.
- Fix an issue that needlessly kept polling the API when users were logged out.

    This could trigger a bug where users were instantly logged out again after a fresh login.

- Show VMs and CTs overviews in a tab panel for Proxmox VE remotes.

.. _remotes_management_1.0:

Remotes Management
''''''''''''''''''

- Proxmox Backup Server remotes can now be added similarly to Proxmox VE remotes.

   A wizard can be used to add new Proxmox Backup Server remotes.

   This includes the ability to inspect the TLS certificate of the remote from within the wizard,
   enabling trust-on-first-use.

   An overview panel shows the status of a datastore, such as usage and I/O information, and its
   contents as a tree of backup snapshots.

   The content of datastores can be inspected, including namespaces and backup snapshots they
   contain.

   The dashboard has also been improved to include new functionality for Proxmox Backup Server
   remotes:

   - Proxmox Backup Server remotes can be added directly from the dashboard.
   - The status of all Proxmox Backup Server remotes can be inspected from a dedicated panel.
   - A new panel shows datastores and their statistics.

- Implement a view that displays a global overview of all available updates for all remotes.

      This includes version information as well as repository status information.

- Add an update panel for Proxmox Backup Server remotes.
- The subscription status endpoint now marks clusters with nodes that all have an unknown
  subscription status as "unknown" instead of "mixed" subscription status.
- Top entities now include Proxmox Backup Server remotes.
- Show more status information on Proxmox VE nodes in the node overview panel.

.. _firewall_and_software_defined_network_1.0:

Firewall and Software Defined Network
'''''''''''''''''''''''''''''''''''''

- Add basic support to gather information on a Proxmox VE remote's firewall setup.

   An overview panel shows which remote nodes and remote guests have an active firewall and how many
   rules are enabled.

   Detailed rules can be inspected by selecting an entity from the overview panel.

- The IP-VRF and MAC-VRF of a EVPN VNet can now be queried.
- Show the status of an IP-VRF and MAC-VRF in new panels in the EVPN panel.
- Show unknown zones if there are any.
- Show fabrics on Proxmox VE remotes in addition to zones.
- Show SDN zones with pending changes as status “pending” instead of “unknown”.

.. _backend_improvements_1.0:

Backend Improvements
''''''''''''''''''''

- Allow filtering API responses based on a ``view`` parameter.

    A view can filter the results of an API endpoint based on resource ID, resource pool, resource
    type, remote, and tags.

    By granting a user permissions to a view, users can query an API endpoint based on the view's
    filter regardless of their own permissions.

    Currently, views can be used when listing resources, querying top entities, status of resources,
    subscription status of remotes, and remote tasks.

- Add endpoints that allow proxying a remote's shell via a web socket.
- Backend support for Proxmox Backup Server remotes:

   - Add TLS probing for Proxmox Backup Server remotes.
   - Allow scanning Proxmox Backup Server remotes analogous to Proxmox VE remotes.
   - Assign an ACL with admin role on “/” for newly created Proxmox Backup Server tokens when adding
     them as a remote.
   - Allow querying a Proxmox Backup Server's remote status.
   - Add a new API endpoint that returns the namespaces of a remote datastore.
   - Add API endpoints to query Proxmox Backup Server tasks.
   - Improve information collection on Proxmox Backup Server datastores by including configuration
     properties and more status types.
   - Support Proxmox Backup Server remote update information collection.
   - Request latest metrics for Proxmox Backup Server when using hourly timeframe.
   - Fix an issue where some Proxmox Backup Server remotes wrongly signaled HttpOnly cookie support,
     leading to an issue when querying them.

- Add an endpoint for listing Proxmox VE and Proxmox Backup Server remotes under ``/pve/remotes``
  and ``/pbs/remotes`` respectively.
- Add an API endpoint for retrieving and refreshing the remote update summary.
- Cache results for remote update availability.
- Poll the remote update status via a periodic task.
- Implement LDAP and Active Directory realm support.
- Add support for OpenID Connect realms.
- When collecting the remote status, keep track of all remotes that collection has failed for.
- Allow non-root users to access several endpoints, such as:

   - Querying top entities (`issue 6794 <https://bugzilla.proxmox.com/show_bug.cgi?id=6794>`__).

   - Proxmox Backup Server RRD endpoints and overview (`issue 6901
     <https://bugzilla.proxmox.com/show_bug.cgi?id=6901>`__).

   - Listing SDN controllers, VNets and zones for all configured Proxmox VE hosts (`issue 6901
     <https://bugzilla.proxmox.com/show_bug.cgi?id=6901>`__).

- Improve permissions on the remote tasks endpoint.
- The node update summary now includes information for package version and repository status.
- Add an endpoint that allows querying remote APT repository status.
- Remove entries of a user in the ACL tree when the user is removed.
- Logs will now include the API path when an API call fails. Unknown errors will be logged too.
- Add endpoints for querying the Proxmox Datacenter Manager host's status and shutting it down or
  rebooting it.
- Fix an issue where only active tasks were included in the remote task list instead of all other
  tasks.
- Fix an issue that broke migration of remote guests.
- Improve documentation of API endpoints and their return type.
- Task, auth, and access logs will now be rotated.
- Split remote configuration and token storage into separate files.
- Add endpoints for querying the subscription status of Proxmox Datacenter Manager and connected
  Proxmox VE and Proxmox Backup Server remotes.
- New endpoints allows querying the configuration of a Proxmox VE node and cluster options.
- Add an API endpoint to get the cached version information of a remote.

.. _command_line_interface_enhancements_1.0:

Command Line Interface Enhancements
'''''''''''''''''''''''''''''''''''

- The CLI client can now list the status and task list for Proxmox Backup Server remotes.
- The type of remote UPID can be inferred by the client instead of having to be explicitly
  specified.
- Add a command for getting all remote subscriptions to ``proxmox-datacenter-manager-admin``.
- A new sub-command to show the subscription status of all remotes was added.
- Fix a bug that prevented the ``proxmox-datacenter-manager-admin`` to function as intended.

.. _documentation_and_support_for_troubleshooting_1.0:

Documentation and Support for Troubleshooting
'''''''''''''''''''''''''''''''''''''''''''''

- Add initial Proxmox Datacenter Manager documentation.
- Add a system report to make supporting Proxmox Datacenter Manager setups easier.
- Include an API viewer.

.. _known_issues_breaking_changes_1.0:

Known Issues & Breaking Changes
'''''''''''''''''''''''''''''''

- The API was restructured:

   - Endpoints under ``/remotes/{id}`` were moved to ``/remotes/remote/{id}``.
   - API Endpoints for ``remote-tasks``, ``remote-update``, and ``metrics-collection`` were moved
     under ``/remotes``.

- Some API endpoints will now correctly return 403 Forbidden error codes when a user has
  insufficient permissions instead of 401 Unauthorized.

     API users relying on the previous erroneous return code may break.  Affected are the following
     endpoints:

     - ``POST /api2/json/pve/remotes/remote/{remote}/lxc/{vmid}/remote-migrate``
     - ``GET /api2/json/pve/remotes/remote/{remote}/resources``
     - ``GET /api2/json/pve/remotes/remote/{remote}/lxc``
     - ``GET /api2/json/pve/remotes/remote/{remote}/qemu``
     - ``POST /api2/json/pve/remotes/remote/{remote}/qemu/{vmid}/remote-migrate``
     - ``GET /api2/json/resources/list``
     - ``GET /api2/json/resources/status``

- Some Alpha releases did not ship with the new HttpOnly authentication flow, API users that relied
  on it may need to adapt.

     Ideally new API users would be switched to use tokens wherever
     possible.

- A minimum password length of eight characters is now enforced on users of the “pdm” realm.
- Move the file storing the LDAP password from
  ``/etc/proxmox-datacenter-manager/ldap_passwords.json`` to
  ``/etc/proxmox-datacenter-manager/access/ldap-passwords.json``

.. _proxmox_datacenter_manager_0.9_beta:

Proxmox Datacenter Manager 0.9 BETA
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

**Released 11. September 2025**

-  Based on Debian Trixie (13)
-  Latest 6.14.11-1 Kernel as stable default
-  ZFS 2.3.4

.. _features_highlights_0.9_beta:

Features (Highlights)
'''''''''''''''''''''

-  New release based on the great Debian Trixie.

-  Seamless upgrade from Proxmox Datacenter Manager Alpha, see `Proxmox
   Datacenter Manager Upgrade from Alpha to
   Beta <Proxmox_Datacenter_Manager_Upgrade_from_Alpha_to_Beta>`__.

-  EVPN configuration for Software-Defined Networking between clusters.

      A new panel provides an overview of the state of all EVPN zones
      across all remotes.

      Create EVPN Zones and VNets across multiple remotes from a single
      interface.

      A more detailed explanation of Proxmox Datacenter Manager's SDN
      capabilities can be found in the
      `documentation <Proxmox_Datacenter_Manager_Beta_Documentation#SDN_Integration>`__.

-  Improved search functionality to find resources quicker.

      Allows filtering by resource type (remote, virtual machine,
      container…), status (stopped, running…) and much more.

      The query syntax is inspired by Elasticsearch and GitHub's query
      language.

      Please refer to the
      `documentation <Proxmox_Datacenter_Manager_Beta_Documentation#Search_Syntax>`__
      for a more thorough explanation of the syntax.

-  More efficient metric collection logic.

      Metrics are now collected concurrently.

-  Integrate privilege management in the access control UI.

      Allow managing the permissions of Proxmox Datacenter Manager
      users.

.. _changelog_overview_0.9_beta:

Changelog Overview
''''''''''''''''''

.. _enhancements_in_the_web_interface_gui_0.9_beta:

Enhancements in the Web Interface (GUI)


-  Add a time frame selector for RRD graphs to allow users to select the
   displayed time frame.

-  Display new metrics such as Pressure Stall Information (PSI) for
   Proxmox VE 9 hosts.

-  Improve the remote URL list of a remote by adding a placeholder,
   clear trigger and clearer column headers.

-  Enhancements to the Proxmox VE remote setup wizard.

      Probe hosts for fingerprint settings, to verify a provided
      fingerprint or to enable trust on first use (TOFU).

      Try matching the provided host against the host list that was
      queried from the remote to avoid duplicates.

      Reset later pages when previous pages have been changed, as they
      need to be revisited.

-  Make the “remote loading” icon nicer.

-  Correctly show a “cube” icon for container guests.

-  Add a panel that allows adding and editing permissions.

-  Move the node overview to a tab and add a tab that displays available
   updates.

-  Add a button linking the user to a remote's upgrade page.

-  Add descriptions for Software Defined Networking tasks.

-  Provide an EVPN overview panel for displaying EVPN Zones and Vnets.

-  Add a view for showing EVPN VRF instances across all remotes.

-  Allow creating EVPN VNets.

-  Open the search panel when clicking different panels in the dashboard
   and pre-fill it with appropriate filters.

-  Add a clear trigger to the search bar.

-  Provide a search icon in the guest panel for better discoverability
   of the search function.

-  Include a summary of all tasks in the dashboard.

-  Render status icons with a shadow instead of a solid background for a
   cleaner look.

-  Enhance the reloading logic for the dashboard.

-  Show tasks from the last 48 hours in the dashboard's task summary.

-  Close the search box if a user navigated to an entry.

-  Display a list of storages and their status in the resource tree of a
   Proxmox VE remote.

-  Change the warning and critical thresholds to 90% and 97.5%
   respectively.

-  Don't show a start or shutdown button for templates (`issue
   6782 <https://bugzilla.proxmox.com/show_bug.cgi?id=6782>`__).

-  The dashboard now includes a panel showing the SDN status report.

-  Show an overview of all SDN zones and their status as a tree.

      The EVPN section is now moved below the SDN menu to mimic Proxmox
      VE's menu structure.

-  Route to correct panels when navigating between components.

-  Allow filtering tasks in the task list by remote.

-  Show the remote tasks when selecting the root node of the resource
   tree for a Proxmox VE remote.

-  Allow navigating to an SDN zone and SDN panel of a remote from the
   zone tree overview.

-  Show failed tasks only in task summary.

-  Add support for initial translations:

   -  Arabic
   -  Bulgarian
   -  Catalan
   -  Chinese (Simplified)
   -  Chinese (Traditional)
   -  Croatian
   -  Czech
   -  Danish
   -  Dutch
   -  Euskera (Basque)
   -  French
   -  Georgian
   -  German
   -  Hebrew
   -  Italian
   -  Japanese
   -  Korean
   -  Norwegian (Bokmal)
   -  Norwegian (Nynorsk)
   -  Persian (Farsi)
   -  Polish
   -  Portuguese (Brazil)
   -  Russian
   -  Slovenian
   -  Spanish
   -  Swedish
   -  Turkish
   -  Ukrainian

.. _remotes_management_0.9_beta:

Remotes Management


-  Enable Proxmox Backup Server Integration, CLI only for now.

-  Enable connection tracking when live migrating VMs on remotes.

      Whether connection tracking actually persists after migration also
      depends on the environment and especially on whether third party
      firewalls are used.

-  Enable trust on first use (TOFU) prompts when adding remotes.

-  Include templates in status counts.

-  Add an API endpoint that allows querying updates and changelogs from
   remotes.

-  Add the API infrastructure for the initial Software Defined
   Networking integration.

.. _backend_improvements_0.9_beta:

Backend Improvements


-  Improve robustness of incoming connection handling.

-  Improve size requirements and performance for remote tasks cache.

-  More intelligently query remote tasks.

-  Fix an issue where the ACME configuration would not be constructed
   properly for the default account.

-  Collect metrics from remotes concurrently to improve performance.

-  Persist metric collection state after a run to allow reusing it after
   a daemon restart.

      This should allow more efficient metric collection runs after
      restarts.

-  Metrics that should have been collected already, but were not due to
   collection timing changes, will now be collected.

-  Keep track of the time it took to collect metrics from each single
   remote and all remotes together.

      This provides better insights into the performance of metric
      collection runs.

-  Add an API endpoint to trigger metric collection.

-  Trigger immediate metric collection when a remote is added.

-  When a metric collection task is delayed skip it instead of
   triggering it quicker.

-  Add a more complex filter and search syntax inspired by Elasticsearch
   and GitHub query language.

-  When querying the remote task list treat a limit of “0” as unbounded
   and return the entire list.

-  Allow filtering remote tasks by remote.

-  Add an API endpoint that allows querying remote task statistics.

-  Add API endpoints for querying Proxmox VE storage's RRD data and
   status.

-  Add a ``resource-type`` parameter to the resources API endpoints.

      This allows more efficient filtering when querying the API for
      tasks and resource statuses.

-  Don't match templates when searching by remote.

-  Improve search when searching by remotes.

      For example, searching for all VMs of a specific remote is now
      possible.

-  When encountering an error, return the root cause not the top level
   error when fetching remotes.

      This makes the reported error messages more specific.

.. _command_line_interface_enhancements_0.9_beta:

Command Line Interface Enhancements


-  Allow query the status and RRD data from remotes via
   ``proxmox-datacenter-manager-client``.
-  Add an upgrade checking script (``pdmAtoB``) to make upgrades more
   seamless.
-  The utility ``proxmox-datacenter-manager-admin`` can now display the
   currently running version.

.. _miscellaneous_improvements_0.9_beta:

Miscellaneous Improvements


-  Log an error when a task to query remote tasks fails instead of
   cancelling all tasks.
-  Fix the order filters are applied when requesting a filtered task
   list.
-  Use the new deb822 format for package repositories.
-  Add a CLI command to allow querying the metric collection status and
   triggering a metric collection run.
-  Handle a missing journal file error more gracefully when querying the
   task list.

.. _known_issues_breaking_changes_0.9_beta:

Known Issues & Breaking Changes
'''''''''''''''''''''''''''''''

-  The API endpoint for listing realms was changed from a ``POST`` to a
   ``GET`` request.

.. _proxmox_datacenter_manager_0.1_alpha:

Proxmox Datacenter Manager 0.1 ALPHA
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

**Released 19. December 2024**

-  Based on Debian Bookworm (12.8)
-  Latest 6.8.12-5 Kernel as stable default
-  Newer 6.11 Kernel as opt-in
-  ZFS: 2.2.6 (with compatibility patches for Kernel 6.11)

.. _features_highlights_0.1_alpha:

Features (Highlights)
'''''''''''''''''''''

-  Connect to and view any number of independent nodes or clusters
   ("Datacenters")

-  View the basic resource usage of all nodes and their guests.

      Saves and caches the list of resources (mainly guests and storage)
      and their usage metrics to provide a quick overview of all
      resources and the last-seen state for offline/unresponsive ones.

-  Basic management of the resources: shutdown, reboot, start, …

      For more complex management tasks, it provides a direct link to
      the full web interface of Proxmox VE/Proxmox Backup Server/…

-  Remote migration of virtual guests between different datacenters.

-  Support for the standard Proxmox feature set including complex
   Multi-Factor Authentication or ACME/Let's Encrypt from the beginning.

.. _changelog_overview_0.1_alpha:

Changelog Overview
''''''''''''''''''

Not applicable for the first alpha release.

.. _known_issues_breaking_changes_0.1_alpha:

Known Issues & Breaking Changes
'''''''''''''''''''''''''''''''

This is an alpha release, there might be lots of stuff that is broken,
gets reworked and fixed somewhat frequently.
