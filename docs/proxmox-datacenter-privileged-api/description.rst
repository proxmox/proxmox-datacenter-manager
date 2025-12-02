This daemon exposes the Proxmox Datacenter Manager management API through a restricted UNIX socket
at ``/run/proxmox-datacenter-manager/priv.sock``.
The daemon runs as ``root`` and has permission to do all privileged operations.

NOTE: The daemon listens to a local UNIX socket address only, so you cannot access it from outside.
The ``proxmox-datacenter-api`` daemon exposes the API to the outside world.
