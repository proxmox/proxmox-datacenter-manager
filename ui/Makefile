include /usr/share/dpkg/default.mk

PACKAGE=proxmox-datacenter-manager-ui
CRATENAME=proxmox-datacenter-manager-ui

BUILDDIR ?= $(PACKAGE)-$(DEB_VERSION_UPSTREAM)
ORIG_SRC_TAR=$(PACKAGE)_$(DEB_VERSION_UPSTREAM).orig.tar.gz

DEB=$(PACKAGE)_$(DEB_VERSION)_$(DEB_HOST_ARCH).deb
DSC=$(PACKAGE)_$(DEB_VERSION).dsc

CARGO ?= cargo
ifeq ($(BUILD_MODE), release)
CARGO_BUILD_ARGS += --release
COMPILEDIR := target/release
else
COMPILEDIR := target/debug
endif

DESTDIR =
PREFIX = /usr
# FIXME: drop the /javascript/ part, it's just wrong.
UIDIR = $(PREFIX)/share/javascript/proxmox-datacenter-manager

COMPILED_OUTPUT := \
	dist/pdm-ui_bundle.js \
	dist/pdm-ui_bg.wasm.gz \
	dist/crisp-yew-style.css \
	dist/material-yew-style.css \
	dist/desktop-yew-style.css

all: $(COMPILED_OUTPUT)

dist:
	mkdir dist

dist/pdm-ui.js dist/pdm-ui_bg.wasm &: $(shell find src -name '*.rs')
	proxmox-wasm-builder build -n pdm-ui --release --optimize

.PHONY: rebuild
rebuild:
	proxmox-wasm-builder build -n pdm-ui --release

dist/pdm-ui_bundle.js: dist/pdm-ui.js dist/pdm-ui_bg.wasm
	esbuild --bundle dist/pdm-ui.js --format=esm >dist/pdm-ui_bundle.js.tmp
	mv dist/pdm-ui_bundle.js.tmp dist/pdm-ui_bundle.js

dist/pdm-ui_bg.wasm.gz: dist/pdm-ui_bg.wasm
	gzip -c9 $^ > $@

dist/%.css: css/%.scss dist
	rust-grass $< $@

install: $(COMPILED_OUTPUT) index.hbs
	install -dm0755 $(DESTDIR)$(UIDIR)
	install -dm0755 $(DESTDIR)$(UIDIR)/js

	install -dm0755 $(DESTDIR)$(UIDIR)/images
	install -m0644 images/favicon.svg $(DESTDIR)$(UIDIR)/images
	install -m0644 images/icon-cpu.svg $(DESTDIR)$(UIDIR)/images
	install -m0644 images/icon-memory.svg $(DESTDIR)$(UIDIR)/images
	install -m0644 images/icon-sdn-vnet.svg $(DESTDIR)$(UIDIR)/images
	install -m0644 images/icon-sdn.svg $(DESTDIR)$(UIDIR)/images
	install -m0644 images/proxmox_logo.svg $(DESTDIR)$(UIDIR)/images
	install -m0644 images/proxmox_logo_white.svg $(DESTDIR)$(UIDIR)/images

	install -dm0755 $(DESTDIR)$(UIDIR)/fonts
	install -m0644 pwt-assets/assets/fonts/RobotoFlexVariableFont.ttf $(DESTDIR)$(UIDIR)/fonts
	install -m0644 pwt-assets/assets/fonts/RobotoFlexVariableFont.woff2 $(DESTDIR)$(UIDIR)/fonts

	install -m0644 dist/pdm-ui_bundle.js $(DESTDIR)$(UIDIR)/js
	install -m0644 dist/pdm-ui_bg.wasm.gz $(DESTDIR)$(UIDIR)/js
	install -m0644 index.hbs $(DESTDIR)$(UIDIR)
	install -m0644 dist/crisp-yew-style.css $(DESTDIR)$(UIDIR)
	install -m0644 dist/material-yew-style.css $(DESTDIR)$(UIDIR)
	install -m0644 dist/desktop-yew-style.css $(DESTDIR)$(UIDIR)

.PHONY: submodule
submodule:
	test -f "pwt-assets/README.md" || git submodule update --init

$(BUILDDIR): submodule
	rm -rf $@ $@.tmp
	mkdir -p $@.tmp/ui
	cp -a debian/ src/ pwt-assets/ images/ css/ index.hbs Makefile Cargo.toml $@.tmp/ui
	cp -a ../Cargo.toml ../lib $@.tmp/
	echo "git clone git://git.proxmox.com/git/$(PACKAGE).git\\ngit checkout $$(git rev-parse HEAD)" \
	    > $@.tmp/ui/debian/SOURCE
	mv $@.tmp $@

$(ORIG_SRC_TAR): $(BUILDDIR)
	tar czf $(ORIG_SRC_TAR) --exclude="$(BUILDDIR)/debian" $(BUILDDIR)

.PHONY: deb
deb: $(DEB)
$(DEB): $(BUILDDIR)
	cd $(BUILDDIR)/ui; dpkg-buildpackage -b -uc -us
	mv $(BUILDDIR)/*.deb ./
	lintian $(DEB)
	@echo $(DEB)

.PHONY: dsc
dsc: $(BUILDDIR)
	rm -rf $(DSC) $(BUILDDIR)
	$(MAKE) $(DSC)
	lintian $(DSC)

$(DSC): $(BUILDDIR) $(ORIG_SRC_TAR)
	cd $(BUILDDIR)/ui; dpkg-buildpackage -S -us -uc -d
	dcmd mv $(BUILDDIR)/*.dsc ./

sbuild: $(DSC)
	sbuild $(DSC)

.PHONY: upload
upload: UPLOAD_DIST ?= $(DEB_DISTRIBUTION)
upload: $(DEB)
	tar cf - $(DEB) |ssh -X repoman@repo.proxmox.com -- upload --product pdm --dist $(UPLOAD_DIST) --arch $(DEB_HOST_ARCH)

.PHONY: clean distclean
distclean: clean
clean:
	$(CARGO) clean
	rm -rf $(PACKAGE)-[0-9]*/ build/ dist/
	rm -f *.deb *.changes *.dsc *.tar.* *.buildinfo *.build .do-cargo-build

.PHONY: dinstall
dinstall: deb
	dpkg -i $(DEB)
