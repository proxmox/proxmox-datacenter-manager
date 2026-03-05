# Experimental Yew GUI for Proxmox Datacenter Manager (PDM)

# Testing

For testing changes in the web UI with rapid development workflow, we recommend to:

1. run a PDM daemon, which can be done on a virtual machine or an existing instance; and
2. use [trunk](https://github.com/trunk-rs/trunk), which builds the web UI upon changes and proxies API calls to the PDM daemon, so the frontend can talk to the PDM during development.

Assuming PDM is reachable via 172.16.254.1:8443.

To test on http://localhost:8080, use:

    trunk serve --proxy-backend=https://172.16.254.1:8443/api2/ --proxy-insecure

To test elsewhere with a secure connection, say https://dev:8080/, generate a cert to obtain `api.key` and `api.pem`, and use:

    trunk serve --address 0.0.0.0 --serve-base / --proxy-backend https://172.16.254.1:8443/api2/ --public-url https://dev:8080/ --proxy-insecure --tls-key-path api.key --tls-cert-path api.pem
