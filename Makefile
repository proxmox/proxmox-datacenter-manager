include /usr/share/dpkg/default.mk
include defines.mk

PACKAGE=proxmox-datacenter-manager
CRATENAME=proxmox-datacenter-manager

BUILDDIR ?= $(PACKAGE)-$(DEB_VERSION_UPSTREAM)
ORIG_SRC_TAR=$(PACKAGE)_$(DEB_VERSION_UPSTREAM).orig.tar.gz

DEB=$(PACKAGE)_$(DEB_VERSION)_$(DEB_HOST_ARCH).deb
DBG_DEB=$(PACKAGE)-dbgsym_$(DEB_VERSION)_$(DEB_HOST_ARCH).deb
DOC_DEB=$(PACKAGE)-docs_$(DEB_VERSION)_all.deb
DSC=$(PACKAGE)_$(DEB_VERSION).dsc

CARGO ?= cargo
ifeq ($(BUILD_MODE), release)
CARGO_BUILD_ARGS += --release
COMPILEDIR := target/release
else
COMPILEDIR := target/debug
endif

COMPLETION_DIR := cli/completions

DESTDIR =

UI_DIR = ui

# TODO: finalize naming of binaries/services, e.g.:
# – full proxmox-datacenter-manager-XYZ prefix for all?
# - pdm-XYZ, would not matter for service binaries, but for user facing though.
#   If only to avoid overly long executables, we could include a `pdmc` convenience symlink.
# currently it's using "proxmox-datacenter" (like we have "proxmox-backup" as base for PBS), which
# does not really works as well for PDM..

USR_BIN := \
	proxmox-datacenter-manager-client \

USR_SBIN := \
	proxmox-datacenter-manager-admin \
	pdmAtoB \

SERVICE_BIN := \
	proxmox-datacenter-api \
	proxmox-datacenter-privileged-api \

# for those binaries that are for internal use and e.g. have no command-line completion
INTERNAL_SERVICE_BIN := \
	proxmox-datacenter-manager-banner \
	proxmox-datacenter-manager-daily-update \

COMPILED_BINS := \
	$(addprefix $(COMPILEDIR)/,$(USR_BIN) $(USR_SBIN) $(SERVICE_BIN) $(INTERNAL_SERVICE_BIN))

# completion helper get generated on build
BASH_COMPLETIONS := $(addsuffix .bc,$(USR_BIN) $(USR_SBIN) $(SERVICE_BIN))
ZSH_COMPLETIONS := $(addprefix _,$(USR_BIN) $(USR_SBIN) $(SERVICE_BIN))
SHELL_COMPLETION_FILES := $(addprefix $(COMPLETION_DIR)/,$(BASH_COMPLETIONS) $(ZSH_COMPLETIONS))

tests ?= --workspace

all:

install: $(COMPILED_BINS) $(SHELL_COMPLETION_FILES)
	install -dm755 $(DESTDIR)$(BINDIR)
	$(foreach i,$(USR_BIN), \
	    install -m755 $(COMPILEDIR)/$(i) $(DESTDIR)$(BINDIR)/ ;)
	install -dm755 $(DESTDIR)$(SBINDIR)
	$(foreach i,$(USR_SBIN), \
	    install -m755 $(COMPILEDIR)/$(i) $(DESTDIR)$(SBINDIR)/ ;)
	install -dm755 $(DESTDIR)$(LIBEXECDIR)/proxmox
	$(foreach i,$(SERVICE_BIN) $(INTERNAL_SERVICE_BIN), \
	    install -m755 $(COMPILEDIR)/$(i) $(DESTDIR)$(LIBEXECDIR)/proxmox/ ;)
	install -dm755 $(DESTDIR)$(BASHCOMPDIR)
	$(foreach i,$(BASH_COMPLETIONS), \
	    install -m644 $(COMPLETION_DIR)/$(i) $(DESTDIR)$(BASHCOMPDIR)/ ;)
	install -dm755 $(DESTDIR)$(ZSHCOMPDIR)
	$(foreach i,$(ZSH_COMPLETIONS), \
	    install -m644 $(COMPLETION_DIR)/$(i) $(DESTDIR)$(ZSHCOMPDIR)/ ;)
	make -C services install
	$(MAKE) -C docs install

$(COMPILED_BINS) $(COMPILEDIR)/docgen &:
	$(CARGO) build $(CARGO_BUILD_ARGS)


$(SHELL_COMPLETION_FILES): create-shell-completions
.PHONY: create-shell-completions
create-shell-completions:
	$(MAKE) -C $(COMPLETION_DIR) $(BASH_COMPLETIONS) $(ZSH_COMPLETIONS)

# make sure we build binaries before docs
docs: $(COMPILEDIR)/docgen

.PHONY: cargo-build
cargo-build:
	$(MAKE) $(COMPILED_BINS)

$(BUILDDIR):
	rm -rf $@ $@.tmp
	mkdir $@.tmp
	cp -a debian/ server/ services/ cli/ lib/ docs/ ui/ defines.mk Makefile Cargo.toml $@.tmp
	echo "git clone git://git.proxmox.com/git/$(PACKAGE).git\\ngit checkout $$(git rev-parse HEAD)" \
	    > $@.tmp/debian/SOURCE
	mv $@.tmp $@

$(ORIG_SRC_TAR): $(BUILDDIR)
	tar czf $(ORIG_SRC_TAR) --exclude="$(BUILDDIR)/debian" $(BUILDDIR)

.PHONY: deb
deb: deb-api deb-ui
deb-api: $(DEB)
$(DEB) $(DBG_DEB) $(DOC_DEB) &: $(BUILDDIR)
	cd $(BUILDDIR); dpkg-buildpackage -b -uc -us
	lintian $(DEB) $(DOC_DEB)
	@echo $(DEB)

.PHONY: dsc
dsc:
	rm -rf $(DSC) $(BUILDDIR)
	$(MAKE) $(DSC)
	lintian $(DSC)

$(DSC): $(BUILDDIR) $(ORIG_SRC_TAR)
	cd $(BUILDDIR); dpkg-buildpackage -S -us -uc -d

sbuild: $(DSC)
	sbuild $(DSC)

.PHONY: upload
upload: UPLOAD_DIST ?= $(DEB_DISTRIBUTION)
upload: $(DEB) $(DBG_DEB)
	tar cf - $(DEB) $(DBG_DEB) |ssh -X repoman@repo.proxmox.com -- upload --product pdm --dist $(UPLOAD_DIST) --arch $(DEB_HOST_ARCH)

.PHONY: clean clean-deb distclean
distclean: clean
clean: clean-deb
	$(CARGO) clean
	$(MAKE) -C docs clean
	$(MAKE) -C $(COMPLETION_DIR) clean
	$(MAKE) -C $(UI_DIR) clean

clean-deb:
	rm -rf $(PACKAGE)-[0-9]*/ build/
	rm -f *.deb *.changes *.dsc *.tar.* *.buildinfo *.build .do-cargo-build

.PHONY: dinstall
dinstall: deb
	dpkg -i $(DEB)

.PHONY: deb-ui
deb-ui: $(UI_DIR)
	$(MAKE) -C $(UI_DIR) deb
	mv $(UI_DIR)/proxmox-datacenter-manager-ui*.deb .

.PHONY: dsc-ui
dsc-ui: $(UI_DIR)
	$(MAKE) -C $(UI_DIR) dsc
	dcmd mv $(UI_DIR)/proxmox-datacenter-manager-ui*.dsc .

test:
	$(CARGO) test $(tests) $(CARGO_BUILD_ARGS)
