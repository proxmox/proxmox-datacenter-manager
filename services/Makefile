SERVICES := \
	proxmox-datacenter-api.service \
	proxmox-datacenter-privileged-api.service \
	proxmox-datacenter-manager-banner.service \
	proxmox-datacenter-manager-daily-update.service \
	proxmox-datacenter-manager-daily-update.timer \

.PHONY: all
all:

.PHONY: install
install: $(SERVICES)
	install -d $(DESTDIR)/lib/systemd/system
	install -m 0644 $(SERVICES) $(DESTDIR)/lib/systemd/system
