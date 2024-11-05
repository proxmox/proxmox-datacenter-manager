# Debug features

## Faked remotes

To enable an alternative implementation for reading remote configurations (`/etc/proxmox-datacenter-manager/remotes.cfg`) 
and creating API clients for these remotes, compile the project using:

```bash
RUSTFLAGS='--cfg remote_config="faked"'
```

This option is helpful for troubleshooting performance issues, especially when managing a large number of remotes
or resources per remote. 

To use this feature, set the `PDM_FAKED_REMOTE_CONFIG` environment variable to the path of a valid 
JSON configuration file. For the expected structure, refer to the `FakeRemoteConfig` struct
in `server/src/test_support/fake_remote.rs`.
