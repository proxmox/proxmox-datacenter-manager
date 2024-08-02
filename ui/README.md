# Expermimental Yew GUI for Proxmox Datacenter Manager

# Testing

1.) start datacenter manager

sudo ./target/debug/proxmox-datacenter-privileged-api
sudo -u www-data ./target/debug/proxmox-datacenter-api 

2.) either: start local trunk server

 trunk serve --proxy-backend=https://localhost:8443/api2/ --proxy-insecure

 then test with url: http://localhost:8080

2.) or: copy files into pdm js folder:

    make all
    make install

And test with url https://localhost:8443
