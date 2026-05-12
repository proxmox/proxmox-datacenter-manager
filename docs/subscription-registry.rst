Subscription Registry
=====================

The subscription registry maintains a central pool of Proxmox VE and Proxmox Backup Server
subscription keys and lets an administrator assign them to remote nodes from a single place, without
having to select and configure a key for all remote nodes individually.

Key Pool
--------

The pool accepts Proxmox VE and Proxmox Backup Server keys; other key prefixes are rejected so that
a new product type is noticed instead of silently parking unusable entries. Each entry records its
origin and the optional remote node it has been assigned to.

Keys can be added in bulk from the web interface or with the ``proxmox-datacenter-client
subscriptions add-keys`` command. The Add dialog takes multiple keys, separated by newlines or
commas, and validates the whole batch atomically.

Node Subscription Status
------------------------

The Node Subscription Status panel shows the live subscription state of every node behind a
configured remote alongside any pending plan from the pool. Nodes that already hold a key the
registry assigned appear with the live level; nodes with a pending pool assignment show a clock
icon until the change is pushed to the remote.

From this view an operator can clear a pending assignment or remove the key from the pool entirely,
which is convenient when a node is known to be wrong without first having to find the matching entry
on the key list.

Assignment
----------

A key can be pinned to a single node manually.

The Auto-Assign action proposes a plan that fills unsubscribed nodes from free pool keys. For
Proxmox VE, the smallest covering key by socket count is chosen, so a 4-socket key is not used on a
2-socket host while a larger host stays unsubscribed.

The proposed plan can be inspected before it is applied. Apply Pending pushes the queued keys to
their target nodes; if a push fails the remaining queue is kept intact for retry. Discard Pending
drops the plan without touching any remote.

Permissions
-----------

Listing the pool and the node status view follows the regular audit privileges on each affected
remote. Pool entries pinned to a remote the operator has no audit privilege on are hidden from
the listing; unbound entries stay visible to anyone with the system-scope audit privilege.

Adding or removing pool entries requires the system-scope MODIFY privilege. Any action that
drives a change on a remote, such as assigning or clearing a key, adopting a live subscription,
or applying the pending plan, additionally requires the matching resource privilege on that
remote, so an operator with global system access alone cannot drive changes against remotes they
have no other authority on. Auto-Assign skips remotes the caller cannot modify, so a previewed
plan never silently commits an assignment on a remote the operator only had audit on.
