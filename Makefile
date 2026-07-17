SHELL := /bin/sh
CARGO ?= cargo
BASH ?= bash
ARGS ?=
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
BINARY := allp
RELEASE_BINARY := target/release/$(BINARY)
DIST_DIR ?= dist
RELEASE_PREFIX ?= allp
BUMP ?= patch
VERSION ?=
CURRENT_VERSION := $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

.DEFAULT_GOAL := help

.PHONY: help fmt fmt-check check clippy test architecture build release quality clean run version git-status docs-check install uninstall reinstall install-user install-check release-prepare release-status release-notes release-archive release-checksum release-finalize release-clean hooks-install hooks-status release-workflow-test

help:
	@printf '%s\n' 'Allp development targets:'
	@printf '%s\n' '  make fmt              Format Rust code'
	@printf '%s\n' '  make fmt-check        Check Rust formatting'
	@printf '%s\n' '  make check            Run cargo check for all targets'
	@printf '%s\n' '  make clippy           Run Clippy with warnings denied'
	@printf '%s\n' '  make test             Run all Rust tests'
	@printf '%s\n' '  make architecture     Run architecture boundary checks'
	@printf '%s\n' '  make build            Build debug binary'
	@printf '%s\n' '  make release          Build release binary'
	@printf '%s\n' '  make quality          Run the full local quality gate'
	@printf '%s\n' '  make clean            Remove Cargo build output'
	@printf '%s\n' '  make run ARGS="..."   Run Allp through cargo'
	@printf '%s\n' '  make version          Print Allp version'
	@printf '%s\n' '  make git-status       Show short Git status'
	@printf '%s\n' '  make docs-check       Validate required documentation anchors'
	@printf '%s\n' ''
	@printf '%s\n' 'Install targets:'
	@printf '%s\n' '  make install          Build and install /usr/local/bin/allp'
	@printf '%s\n' '  make reinstall        Rebuild and replace the installed allp binary'
	@printf '%s\n' '  make uninstall        Remove the installed allp binary'
	@printf '%s\n' '  make install-user     Install allp to $$HOME/.local/bin without sudo'
	@printf '%s\n' '  make install-check    Show the allp binary resolved by the shell'
	@printf '%s\n' ''
	@printf '%s\n' 'Local release workflow:'
	@printf '%s\n' '  make hooks-install    Configure this repo to use .githooks/'
	@printf '%s\n' '  make release-prepare BUMP=patch|minor|major'
	@printf '%s\n' '  make release-prepare VERSION=x.y.z'
	@printf '%s\n' '  make release-status   Show pending local release state'
	@printf '%s\n' '  make release-clean    Remove ignored local release output'
	@printf '%s\n' '  make release-workflow-test'

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

check:
	$(CARGO) check --all-targets

clippy:
	$(CARGO) clippy --all-targets --all-features -- -D warnings

test:
	$(CARGO) test --all-targets

architecture:
	$(BASH) scripts/check-architecture.sh

build:
	$(CARGO) build

release:
	$(CARGO) build --release

quality: fmt-check check clippy test architecture release docs-check

clean:
	$(CARGO) clean

run:
	$(CARGO) run -- $(ARGS)

version:
	$(CARGO) run -- --version

git-status:
	git status --short

docs-check:
	test -f README.md
	test -f README.fa.md
	test -f CHANGELOG.md
	test -f ROADMAP.md
	test -f TODO.md
	test -f docs/CLI_CONTRACT.md
	test -n '$(CURRENT_VERSION)'
	grep -q '$(CURRENT_VERSION)' README.md
	grep -q '$(CURRENT_VERSION)' README.fa.md
	grep -q 'Snap' README.md
	grep -q 'Snap' README.fa.md
	grep -q 'make quality' README.md
	grep -q 'make quality' README.fa.md
	grep -q 'allp install pycharm' README.md
	grep -q 'allp install pycharm' README.fa.md

install: release
	sudo install -Dm755 "$(RELEASE_BINARY)" "$(BINDIR)/$(BINARY)"
	"$(BINDIR)/$(BINARY)" --version
	@printf 'Installed %s\n' "$(BINDIR)/$(BINARY)"

uninstall:
	sudo rm -f "$(BINDIR)/$(BINARY)"
	@printf 'Removed %s\n' "$(BINDIR)/$(BINARY)"

reinstall: release
	sudo install -Dm755 "$(RELEASE_BINARY)" "$(BINDIR)/$(BINARY)"
	"$(BINDIR)/$(BINARY)" --version
	@printf 'Reinstalled %s\n' "$(BINDIR)/$(BINARY)"

install-user: release
	mkdir -p "$$HOME/.local/bin"
	install -m755 "$(RELEASE_BINARY)" "$$HOME/.local/bin/$(BINARY)"
	"$$HOME/.local/bin/$(BINARY)" --version
	@case ":$$PATH:" in *":$$HOME/.local/bin:"*) : ;; *) printf '%s\n' 'Warning: $$HOME/.local/bin is not on PATH.' ;; esac
	@printf 'Installed %s\n' "$$HOME/.local/bin/$(BINARY)"

install-check:
	@resolved=$$(command -v "$(BINARY)" || true); \
	if [ -z "$$resolved" ]; then \
		printf '%s\n' 'allp was not found on PATH.'; \
		printf '%s\n' 'Run make install, then refresh your shell command cache with hash -r or rehash.'; \
		exit 1; \
	fi; \
	printf 'Resolved allp: %s\n' "$$resolved"; \
	"$$resolved" --version; \
	if [ "$$resolved" != "$(BINDIR)/$(BINARY)" ]; then \
		printf 'Warning: PATH resolves allp outside %s\n' "$(BINDIR)/$(BINARY)"; \
	fi; \
	printf '%s\n' 'If your shell still sees an older binary, run: hash -r'; \
	printf '%s\n' 'For zsh with command hashing, run: rehash'

release-prepare:
	BUMP="$(BUMP)" VERSION="$(VERSION)" DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-prepare.sh

release-status:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-status.sh

release-notes:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-finalize.sh --notes

release-archive:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-finalize.sh --archive

release-checksum:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-finalize.sh --checksum

release-finalize:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-finalize.sh

release-clean:
	rm -rf "$(DIST_DIR)" .release-state
	@printf '%s\n' 'Removed ignored local release output.'

hooks-install:
	git config core.hooksPath .githooks
	@printf '%s\n' 'Installed local Git hooks from .githooks/.'

hooks-status:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-status.sh --hooks-only

release-workflow-test:
	$(BASH) scripts/test-release-workflow.sh
