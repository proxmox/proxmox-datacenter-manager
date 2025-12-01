DOCDIR = /usr/share/doc/proxmox-datacenter-manager

API_VIEWER_SOURCES= \
	index.html \
	apidoc.js

API_VIEWER_FILES := \
	apidata.js \
	/usr/share/javascript/proxmox-widget-toolkit-dev/APIViewer.js

ifeq ($(BUILD_MODE), release)
COMPILEDIR := ../../target/release
else
COMPILEDIR := ../../target/debug
endif

.PHONY: install
install: ${API_VIEWER_SOURCES}
	install -dm 0755 $(DESTDIR)$(DOCDIR)/html/api-viewer
	install -m 0644 ${API_VIEWER_SOURCES} $(DESTDIR)$(DOCDIR)/html/api-viewer

apidata.js: ${COMPILEDIR}/docgen
	${COMPILEDIR}/docgen apidata.js >$@

apidoc.js: ${API_VIEWER_FILES}
	cat ${API_VIEWER_FILES} >$@.tmp
	mv $@.tmp $@
