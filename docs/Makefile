include ../defines.mk

GENERATED_SYNOPSIS := \
	proxmox-datacenter-manager-admin/synopsis.rst \
	proxmox-datacenter-manager-client/synopsis.rst \
	config/remotes/config.rst \
#	config/views/config.rst \

MAN1_PAGES := \
	proxmox-datacenter-manager-admin.1 \
	proxmox-datacenter-manager-client.1 \
	proxmox-datacenter-api.1 \
	proxmox-datacenter-privileged-api.1 \

MAN5_PAGES := \
	remotes.cfg.5 \
#	views.cfg.5 \

API_VIEWER_SOURCES=				\
	api-viewer/index.html			\
	api-viewer/apidoc.js

API_VIEWER_FILES :=							\
	api-viewer/apidata.js						\
	/usr/share/javascript/proxmox-widget-toolkit-dev/APIViewer.js	\

# Sphinx documentation setup
SPHINXOPTS    =
SPHINXBUILD   = sphinx-build
BUILDDIR      = output

ifeq ($(BUILD_MODE), release)
COMPILEDIR := ../target/release
SPHINXOPTS    += -t release
else
COMPILEDIR := ../target/debug
SPHINXOPTS    += -t devbuild
endif

# Sphinx internal variables.
ALLSPHINXOPTS   = -d $(BUILDDIR)/doctrees $(SPHINXOPTS) .

all: $(MAN1_PAGES) $(MAN5_PAGES)

config/%/config.rst: $(COMPILEDIR)/docgen
	$(COMPILEDIR)/docgen $*.cfg >$@

%/synopsis.rst: $(COMPILEDIR)/%
	$< printdoc > $@

$(MAN1_PAGES) $(MAN5_PAGES): man-pages

.PHONY: man-pages
man-pages: $(GENERATED_SYNOPSIS)
	$(SPHINXBUILD) $(SPHINXOPTS) -b man ./ $(BUILDDIR)/man

api-viewer/apidata.js: $(COMPILEDIR)/docgen
	$(COMPILEDIR)/docgen apidata.js >$@

api-viewer/apidoc.js: $(API_VIEWER_FILES)
	cat $(API_VIEWER_FILES) >$@.tmp
	mv $@.tmp $@

.PHONY: html
html: $(GENERATED_SYNOPSIS) images/proxmox-logo.svg custom.css conf.py $(API_VIEWER_SOURCES)
	$(SPHINXBUILD) -b html $(ALLSPHINXOPTS) $(BUILDDIR)/html
	install -m 0644 _static/custom.js _static/custom.css images/proxmox-logo.svg $(BUILDDIR)/html/_static/
	install -dm 0755 $(BUILDDIR)/html/api-viewer
	install -m 0644 ${API_VIEWER_SOURCES} $(BUILDDIR)/html/api-viewer
	@echo
	@echo "Build finished. The HTML pages are in $(BUILDDIR)/html."

.PHONY: latexpdf
latexpdf: $(GENERATED_SYNOPSIS)
	@echo "Requires python3-sphinx, texlive-xetex, xindy and texlive-fonts-extra"
	$(SPHINXBUILD) -b latex $(ALLSPHINXOPTS) $(BUILDDIR)/latex
	@echo "Running LaTeX files through xelatex..."
	$(MAKE) -C $(BUILDDIR)/latex all-pdf
	@echo "xelatex finished; the PDF files are in $(BUILDDIR)/latex."

clean:
	rm -r -f *~ *.1 $(BUILDDIR) $(GENERATED_SYNOPSIS)

install_manual_pages: man-pages
	install -dm755 $(DESTDIR)$(MAN1DIR)
	for i in $(MAN1_PAGES); do install -m755 $(BUILDDIR)/man/$$i $(DESTDIR)$(MAN1DIR)/ ; done
	install -dm755 $(DESTDIR)$(MAN5DIR)
	for i in $(MAN5_PAGES); do install -m755 $(BUILDDIR)/man/$$i $(DESTDIR)$(MAN5DIR)/ ; done

install_html: html
	install -dm755 $(DESTDIR)$(DOCDIR)
	rsync -a $(BUILDDIR)/html $(DESTDIR)$(DOCDIR)

install_pdf: latexpdf
	install -dm755 $(DESTDIR)$(DOCDIR)
	install -m 0644 output/latex/ProxmoxDatacenterManager.pdf $(DESTDIR)$(DOCDIR)/proxmox-datacenter-manager.pdf

ifneq ($(filter nodoc,$(DEB_BUILD_PROFILES)),)

install: install_manual_pages

else

install: install_manual_pages install_html install_pdf

endif
