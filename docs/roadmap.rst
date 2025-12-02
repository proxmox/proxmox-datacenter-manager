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


Please note that this list outlines general goals and potential ideas rather than fixed promises.
If you have a substantial use case you're willing to describe in detail, we encourage you to open
enhancement requests for items listed here, as your feedback helps us prioritize work and understand
specific needs.

.. _release_history:

Release History
---------------

...
