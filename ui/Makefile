include /usr/share/dpkg/default.mk

PACKAGE=proxmox-datacenter-manager-ui
CRATENAME=proxmox-datacenter-manager-ui

BUILDDIR ?= $(PACKAGE)-$(DEB_VERSION_UPSTREAM)
ORIG_SRC_TAR=$(PACKAGE)_$(DEB_VERSION_UPSTREAM).orig.tar.gz

DEB=$(PACKAGE)_$(DEB_VERSION)_$(DEB_HOST_ARCH).deb
DBG_DEB=$(PACKAGE)-dbgsym_$(DEB_VERSION)_$(DEB_HOST_ARCH).deb
DSC=$(PACKAGE)_$(DEB_VERSION).dsc

# TODO: adapt for yew ui
CARGO ?= cargo
ifeq ($(BUILD_MODE), release)
CARGO_BUILD_ARGS += --release
COMPILEDIR := target/release
else
COMPILEDIR := target/debug
endif

DESTDIR =
PREFIX = /usr
UIDIR = $(PREFIX)/share/javascript/proxmox-datacenter-manager

COMPILED_OUTPUT := \
	dist/pdm_bundle.js \
	dist/pdm_bg.wasm \
	dist/crisp-yew-style.css \
	dist/material-yew-style.css \
	dist/desktop-yew-style.css

all: $(COMPILED_OUTPUT)

dist:
	mkdir dist

dist/pdm.js dist/pdm_bg.wasm &: $(shell find src -name '*.rs')
	proxmox-wasm-builder build -n pdm --release

.PHONY: rebuild
rebuild:
	proxmox-wasm-builder build -n pdm --release

dist/pdm_bundle.js: dist/pdm.js dist/pdm_bg.wasm
	esbuild --bundle dist/pdm.js --format=esm >dist/pdm_bundle.js.tmp
	mv dist/pdm_bundle.js.tmp dist/pdm_bundle.js

dist/%.css: pwt-assets/scss/%.scss | dist
	rust-grass $< $@

install: $(COMPILED_OUTPUT) index.hbs
	install -dm0755 $(DESTDIR)$(UIDIR)
	install -dm0755 $(DESTDIR)$(UIDIR)/js

	install -dm0755 $(DESTDIR)$(UIDIR)/images
	install -m0644 images/proxmox_logo.png $(DESTDIR)$(UIDIR)/images
	install -m0644 images/proxmox_logo_white.png $(DESTDIR)$(UIDIR)/images

	install -dm0755 $(DESTDIR)$(UIDIR)/fonts
	install -m0644 pwt-assets/assets/fonts/RobotoFlexVariableFont.ttf $(DESTDIR)$(UIDIR)/fonts

	install -m0644 dist/pdm_bundle.js $(DESTDIR)$(UIDIR)/js
	install -m0644 dist/pdm_bg.wasm $(DESTDIR)$(UIDIR)/js
	install -m0644 index.hbs $(DESTDIR)$(UIDIR)
	install -m0644 dist/crisp-yew-style.css $(DESTDIR)$(UIDIR)
	install -m0644 dist/material-yew-style.css $(DESTDIR)$(UIDIR)
	install -m0644 dist/desktop-yew-style.css $(DESTDIR)$(UIDIR)


$(BUILDDIR):
	rm -rf $@ $@.tmp
	mkdir $@.tmp
	cp -a debian/ src/ pwt-assets/ index.hbs Makefile Cargo.toml $@.tmp
	echo "git clone git://git.proxmox.com/git/$(PACKAGE).git\\ngit checkout $$(git rev-parse HEAD)" \
	    > $@.tmp/debian/SOURCE
	mv $@.tmp $@

$(ORIG_SRC_TAR): $(BUILDDIR)
	tar czf $(ORIG_SRC_TAR) --exclude="$(BUILDDIR)/debian" $(BUILDDIR)

.PHONY: deb
deb: $(DEB)
$(DBG_DEB): $(DEB)
$(DEB): $(BUILDDIR)
	cd $(BUILDDIR); dpkg-buildpackage -b -uc -us
	lintian $(DEB)
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

.PHONY: clean distclean
distclean: clean
clean:
	$(CARGO) clean
	rm -rf $(PACKAGE)-[0-9]*/ build/ dist/
	rm -f *.deb *.changes *.dsc *.tar.* *.buildinfo *.build .do-cargo-build

.PHONY: dinstall
dinstall: deb
	dpkg -i $(DEB)
