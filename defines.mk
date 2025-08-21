PREFIX = /usr
BINDIR = $(PREFIX)/bin
SBINDIR = $(PREFIX)/sbin
LIBDIR = $(PREFIX)/lib
LIBEXECDIR = $(PREFIX)/libexec
BASHCOMPDIR = $(PREFIX)/share/bash-completion/completions
ZSHCOMPDIR = $(PREFIX)/share/zsh/vendor-completions
MAN1DIR = $(PREFIX)/share/man/man1
MAN5DIR = $(PREFIX)/share/man/man5
DOCDIR = $(PREFIX)/share/doc/proxmox-datacenter-manager
SYSCONFDIR = /etc
ZSH_COMPL_DEST = $(PREFIX)/share/zsh/vendor-completions

# For local overrides
-include local.mak
