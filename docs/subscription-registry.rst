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

From this view an operator can revert a pending change on the selected node (an unpushed
assignment or a queued Clear Key) or queue a new Clear Key. Clear Key frees the live
subscription key from a node so it can be reassigned elsewhere. The action is queued until it
is committed via Apply Pending or reverted on a per-node basis.

Assignment and Clearing
-----------------------

A key can be pinned to a single node manually.

The Auto-Assign action proposes a plan that fills unsubscribed nodes from free pool keys. For
Proxmox VE, the smallest covering key by socket count is chosen, so a 4-socket key is not used
on a 2-socket host while a larger host stays unsubscribed.

The Clear Key action queues the live subscription on the selected node for removal. The
action requires the (remote, node) to already be tracked by the pool. Apply Pending later
issues the removal on the remote and releases the pool binding so the key becomes available
for reassignment. Discard Pending drops the queued clear without touching the remote; the
binding stays intact and the operator can retry.

The Adopt Key action imports the live subscription on a remote node into the pool as a
bound entry, without touching the remote. Use it to bring a pre-existing subscription -- one
installed on a node before PDM took over its pool management -- under the registry so that
pool actions such as Clear Key and Auto-Assign can act on it. Nodes that are eligible for
adoption are highlighted with a download hint icon in the Node Subscription Status tree;
the pool grid carries a hidden-by-default Source column distinguishing manually-added from
adopted entries, which can be enabled via the column picker if the distinction matters.

The Adopt All action runs the same import across every remote the operator can audit in one
transaction. Use it after first connecting an existing fleet of nodes to PDM so the pool
catches up with the live subscriptions already deployed, without having to click through
Adopt Key for each node. Candidates the operator has no modify privilege on, whose key is
already bound elsewhere in the pool, whose (remote, node) target is already bound by another
pool entry, or whose key or node name fails schema validation are skipped silently.

The proposed plan can be inspected before it is applied. Apply Pending walks the queue in
order and attempts every entry; any that fail are reported and stay pending, so one unreachable
node does not strand the rest and a later Apply Pending retries only the failures. Discard
Pending drops the plan without touching any remote.

The Check Subscription action triggers a fresh shop-side verification of the live subscription
on the selected node, equivalent to the per-product "Check" button on PVE / PBS. Useful for
promoting a stale ``Invalid`` or ``Expired`` verdict to ``Active`` once the underlying issue is
fixed at the shop, without having to wait for the next periodic check. The Status column tooltip
surfaces the last-checked timestamp and the next-due-date as reported by the remote, where
available.

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
