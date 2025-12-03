Roadmap
=======


* Simplify adding remotes by implementing a remote-join information endpoint. This allows copying
  key data to provide the Proxmox Datacenter Manager with all initial information required to
  connect to a remote, ensuring trust and safety.

  - This concept is similar to the "Join Information" API and UI used in Proxmox VE clusters but
    functions independently, as Proxmox Datacenter Manager does not rely on cluster communication.

* Management of core configurations:

  - Backup jobs and their status.
  - Firewall management (building upon the existing visualization capabilities).

* Off-site replication of guests for manual recovery in case of datacenter failure.
* Evaluate an active-standby architecture for the Proxmox Datacenter Manager to avoid a single point
  of failure.

  - Currently, users can operate two instances, which results in doubled metric collection but
    minimal overhead otherwise.

* Integration of Proxmox Mail Gateway.
* Bulk actions, such as starting, stopping, or migrating multiple virtual guests at once.
* Implementation of a notification system:

  - Standard system notifications and update alerts.
  - Evaluation of Proxmox Datacenter Manager acting as a notification target for remotes.

* User Interface improvements:

  - Handling Multi-Factor Authentication (MFA) for the initial "Probe Remote" connection.
  - Evaluate a Pool View where hierarchical resource pools from all remotes are merged.

* Improvements for customizable Views:

  - Provide more card widgets that one can add, including ones that provide some direct control over
    the included resources.
  - Allow to add the pre-defined Updates, Firewall or Task tabs from the Remotes panel.
  - Allow to create tabs to organize complex views.
  - Evaluate other layouting options for rendering the card widgets.
  - Extending the filter capabillity by providing more types and evaluate more flexible comparission
    operators.


Please note that this list outlines general goals and potential ideas rather than fixed promises.
If you have a substantial use case you're willing to describe in detail, we encourage you to open
enhancement requests for items listed here, as your feedback helps us prioritize work and understand
specific needs.

.. _release_history:

Release History
---------------

.. _proxmox_datacenter_manager_1.0:

Proxmox Datacenter Manager 1.0
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

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

      With the release of Proxmox VE 9.1 and Proxmox Backup Server 4.0.20 API tokens can now request
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

    LDAP, Active Directory and OpenID connect realms can be added to allow easy authentication
    management.

    LDAP and Active Directory realms can also be synced using this panel.

    These realms can be configured as default realms. Default realms are used by the log in mask by
    default instead of the PAM realm.

- Add a panel that allows managing tokens and allow configuring ACL entries for tokens.
- Enable the documentation button in the top navigation bar.
- Link to proper in-build documentation instead of Beta documentation.
- A new tab under the “Administration” menu shows the status of the Proxmox Datacenter Manager host
  and allows shutting it off or rebooting it (`issue 6300
  <https://bugzilla.proxmox.com/show_bug.cgi?id=6300>`__).
- Add presentation of subscription status of remotes:

   - The remote subscription status can now be refreshed manually.
   - Remote subscriptions can now be inspected by clicking on the subscription status panel in the
     dashboard (`issue 6797 <https://bugzilla.proxmox.com/show_bug.cgi?id=6797>`__).
   - Add a “Details” button in the subscription panel to show the subscription status dialog.

- Tags of Proxmox VE guests are now shown in the resource tree.
- Add a panel displaying the notes of Proxmox VE nodes and datacenters.
- Feature-gate functionality for Proxmox VE guests depending on the version of the remote.
- Allow the UI to render components base on the user's privileges.
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
- A new panel show the Proxmox Datacenter Manager's subscription information.
- When adding a remote via the setup wizard, the token will now include the Proxmox Datacenter
  Manager host. This ensures multiple instances of Proxmox Datacenter Manager can be connected to
  the same remote.
- Mask remote shells if the remote version is too old to support the feature.
- Fix an issue that prevented realms from being deleted (`issue 6885
  <https://bugzilla.proxmox.com/show_bug.cgi?id=6885>`__).
- Fix an issue where updating a storage's status did not trigger correctly.
- Fix an issue that prevented users in the PAM realm to be added as Proxmox Datacenter Manager users
  (`issue 6787 <https://bugzilla.proxmox.com/show_bug.cgi?id=6787>`__).
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

    For Proxmox Backup Server remotes a button was added in the top bar of the overview to open a
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

    This could trigger a bug where users were instantly logged out again after a fresh log in.

- Show VMs and CTs overviews in a tab panel for Proxmox VE remotes.

.. _remotes_management_1.0:

Remotes Management
''''''''''''''''''

- Proxmox Backup Server remotes can now be added similarly to Proxmox VE remotes.

   A wizard can be used to add new Proxmox Backup Server remotes.

   This includes the ability to inspect the TLS certificate of the remote from within the wizard,
   enabling trust-on-first-use.

   An overview panel shows the status of a datastore, such as usage and I/O information, and its
   contents as a tree of snapshots.

   The content of datastores can be inspected, including namespaces and snapshots they contain.

   The dashboard has also been improved to include new functionality for Proxmox Backup Server
   remotes:

   - Proxmox Backup Server remotes can be added directly from the dashboard.
   - The status of all Proxmox Backup Server remotes can be inspected from a dedicated panel.
   - A new panel shows datastores and their statistics.

- Implement a view that displays a global overview of all available updates for all remotes.

      This includes version information as well as repository status information.

- Add an update panel for Proxmox Backup Server remotes.
- The subscription status endpoint now marks clusters with nodes that all have an unknown
  subscription status as unknown not mixed subscription status.
- Top entities now include Proxmox Backup Server remotes.
- Show more status information on Proxmox VE nodes in the node overview panel.

.. _firewall_and_software_defined_network_1.0:

Firewall and Software Defined Network
'''''''''''''''''''''''''''''''''''''

- Add basic support to gather information on a Proxmox VE remote's firewall setup.

   An overview panel shows which remote nodes and remote guest have an active firewall and how many
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
    subscription status of remotes and remote tasks.

- Add endpoints that allow proxying a remote's shell via a web socket.
- Backend support for Proxmox Backup Server remotes:

   - Add TLS probing for Proxmox Backup Server remotes.
   - Allow scanning Proxmox Backup Server remotes analogous to Proxmox VE remotes.
   - Assign an ACL with admin role on “/” for newly created Proxmox Backup Server tokens when adding
     them as a remote.
   - Allow querying a Proxmox Backup Server's remote status.
   - New endpoint that returns the namespaces of a remote datastore.
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
- Node update summary includes information for package version and repository status.
- Add an endpoint that allows querying remote APT repository status.
- Remove entries of a user in the ACL tree when the user is removed.
- Logs will now include the API path when an API call fails. Unknown errors will be logged too.
- Add endpoints for querying the Proxmox Datacenter Host's status and shutting it down or rebooting
  it.
- Fix an issue where only active tasks were included in the remote task list instead of all other
  tasks.
- Fix an issue that broke migration of remote guests.
- Improve documentation of API endpoints and their return type.
- Task, auth, and access logs will now be rotated.
- Split remote configuration and token storage into separate files.
- Add endpoints for querying the Proxmox Datacenter Manager's and connected Proxmox VE and Proxmox
  Backup Server remotes.
- New endpoints allows querying the configuration of a Proxmox VE node and cluster options.
- Add an API endpoint to get the cached version info of a remote.

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

- Some API endpoints will now correctly return 403 FORBIDDEN error codes when a user has
  insufficient permissions instead of 401 UNAUTHORIZED.

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
- Move the file storing LDAP password from ``/etc/proxmox-datacenter-manager/ldap_passwords.json``
  to ``/etc/proxmox-datacenter-manager/access/ldap-passwords.json``

.. _proxmox_datacenter_manager_0.9_beta:

Proxmox Datacenter Manager 0.9 BETA
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

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
