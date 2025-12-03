DOCDIR = /usr/share/doc/proxmox-datacenter-manager

API_VIEWER_SOURCES= \
	index.html \
	apidoc.css \
	apidoc-dark.css \
	apidoc.js \

API_VIEWER_JS_FILES := \
	apidata.js \
	/usr/share/javascript/extjs/ext-all.js \
	/usr/share/javascript/proxmox-widget-toolkit-dev/APIViewer.js

API_VIEWER_CSS_FILES := \
	/usr/share/javascript/extjs/theme-crisp/resources/theme-crisp-all_1.css \
	/usr/share/javascript/extjs/theme-crisp/resources/theme-crisp-all_2.css \
	/usr/share/javascript/proxmox-widget-toolkit/css/ext6-pmx.css \

API_VIEWER_DARK_CSS_FILES := /usr/share/javascript/proxmox-widget-toolkit/themes/theme-proxmox-dark.css

IMAGES := \
	dd/drop-no.png \
	grid/col-move-bottom.png \
	grid/col-move-top.png \
	grid/columns.png \
	grid/group-collapse.png \
	grid/group-expand.png \
	grid/hd-pop.png \
	grid/hmenu-asc.png \
	grid/hmenu-desc.png \
	grid/sort_asc.png \
	grid/sort_desc.png \
	menu/default-checked.png \
	menu/default-menu-parent.png \
	menu/default-unchecked.png \
	tools/tool-sprites.png \
	tree/elbow-end-minus.png \
	tree/elbow-end-plus.png \
	tree/elbow-end.png \
	tree/elbow-line.png \
	tree/elbow-minus.png \
	tree/elbow-plus.png \
	tree/elbow.png \
	tree/folder-open.png \
	tree/folder.png \
	tree/leaf.png \

IMAGES_DIR := grid tools tree dd menu col

API_VIEWER_IMAGES_FILES := $(addprefix /usr/share/javascript/extjs/theme-crisp/resources/images/, $(IMAGES))

ifeq ($(BUILD_MODE), release)
COMPILEDIR := ../../target/release
else
COMPILEDIR := ../../target/debug
endif

all:

.PHONY: install
install: $(API_VIEWER_SOURCES)
	install -dm 0755 $(DESTDIR)$(DOCDIR)/html/api-viewer
	install -m 0644 $(API_VIEWER_SOURCES) $(DESTDIR)$(DOCDIR)/html/api-viewer
	install -dm 0755 $(addprefix $(DESTDIR)$(DOCDIR)/html/api-viewer/images/, $(IMAGES_DIR))
	install -m 0644 $(API_VIEWER_IMAGES_FILES) $(DESTDIR)$(DOCDIR)/html/api-viewer/images/
	cd $(DESTDIR)$(DOCDIR)/html/api-viewer/images; for f in $(IMAGES); do \
	    mv $$(basename $$f) $$f; \
	  done

apidata.js: $(COMPILEDIR)/docgen
	$(COMPILEDIR)/docgen apidata.js >$@

apidoc.js: $(API_VIEWER_JS_FILES)
	cat $(API_VIEWER_JS_FILES) >$@.tmp
	mv $@.tmp $@

apidoc.css: $(API_VIEWER_CSS_FILES)
	cat $(API_VIEWER_CSS_FILES) >$@.tmp
	mv $@.tmp $@

apidoc-dark.css: $(API_VIEWER_DARK_CSS_FILES)
	cat $(API_VIEWER_DARK_CSS_FILES) >$@.tmp
	mv $@.tmp $@

.PHONY: clean
clean:
	rm -f apidata.js apidoc.js apidoc.css apidoc-dark.css *.tmp
