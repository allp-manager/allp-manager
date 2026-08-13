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
CURRENT_GIT_SHA := $(shell git rev-parse --verify HEAD 2>/dev/null || printf unknown)

.DEFAULT_GOAL := help

.PHONY: help fmt fmt-check check clippy test architecture build release quality clean run doctor version git-status docs-check install uninstall reinstall install-user install-check install-resolution-warning release-prepare release-status release-notes release-archive release-checksum release-finalize release-push release-clean hooks-install hooks-status release-workflow-test release-assets-test

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
	@printf '%s\n' '  make doctor           Run platform and capability diagnostics'
	@printf '%s\n' '  make version          Print Allp version'
	@printf '%s\n' '  make git-status       Show short Git status'
	@printf '%s\n' '  make docs-check       Validate required documentation anchors'
	@printf '%s\n' ''
	@printf '%s\n' 'Install targets:'
	@printf '%s\n' '  make install          Build and install /usr/local/bin/allp'
	@printf '%s\n' '  make reinstall        Rebuild and replace /usr/local/bin/allp (warns if PATH shadows it)'
	@printf '%s\n' '  make uninstall        Remove the installed allp binary'
	@printf '%s\n' '  make install-user     Install allp to $$HOME/.local/bin without sudo'
	@printf '%s\n' '  make install-check    Show the allp binary resolved by the shell'
	@printf '%s\n' ''
	@printf '%s\n' 'Local release workflow:'
	@printf '%s\n' '  make hooks-install    Configure this repo to use .githooks/'
	@printf '%s\n' '  make release-prepare BUMP=patch|minor|major'
	@printf '%s\n' '  make release-prepare VERSION=x.y.z'
	@printf '%s\n' '  make release-status   Show pending local release state'
	@printf '%s\n' '  make release-finalize Finalize a prepared local release'
	@printf '%s\n' '  make release-push     Push current branch and matching release tag'
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
	ALLP_GIT_SHA="$(CURRENT_GIT_SHA)" $(CARGO) build

release:
	ALLP_GIT_SHA="$(CURRENT_GIT_SHA)" $(CARGO) build --release

quality: fmt-check check clippy test architecture release docs-check

clean:
	$(CARGO) clean

run:
	$(CARGO) run -- $(ARGS)

doctor:
	$(CARGO) run -- doctor

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
	test -f docs/SNAP_BACKEND.md
	test -f docs/FLATPAK_BACKEND.md
	test -f docs/PREREQUISITES.md
	test -f docs/ALTERNATIVE_INSTALLERS.md
	test -f docs/REGRESSION_GUARDRAILS.md
	test -f docs/TERMINAL_UI.md
	test -f docs/HOMEBREW_BACKEND.md
	test -f docs/assets/tui-maintenance.svg
	test -f docs/SELF_UPDATE.md
	test -f docs/RELEASE_MANIFEST.md
	test -n '$(CURRENT_VERSION)'
	grep -q '$(CURRENT_VERSION)' README.md
	grep -q '$(CURRENT_VERSION)' README.fa.md
	grep -q 'Snap' README.md
	grep -q 'Snap' README.fa.md
	grep -q 'make quality' README.md
	grep -q 'make quality' README.fa.md
	grep -q 'allp self-update' README.md
	grep -q 'allp self-update' README.fa.md
	grep -q 'allp install pycharm' README.md
	grep -q 'allp install pycharm' README.fa.md
	grep -q -- '--no-tui' README.md
	grep -q -- '--no-tui' README.fa.md
	grep -q -- '--no-tui' docs/CLI_CONTRACT.md
	grep -q 'Live Maintenance Dashboard' docs/TERMINAL_UI.md
	grep -q 'tui-maintenance.svg' README.md
	grep -q 'tui-maintenance.svg' README.fa.md

install: release
	sudo install -Dm755 "$(RELEASE_BINARY)" "$(BINDIR)/$(BINARY)"
	@printf 'Installed build identity (expected commit %s):\n' "$(CURRENT_GIT_SHA)"
	"$(BINDIR)/$(BINARY)" --version --verbose
	@$(MAKE) --no-print-directory install-resolution-warning
	@printf 'Installed %s\n' "$(BINDIR)/$(BINARY)"

uninstall:
	sudo rm -f "$(BINDIR)/$(BINARY)"
	@printf 'Removed %s\n' "$(BINDIR)/$(BINARY)"

reinstall: release
	sudo install -Dm755 "$(RELEASE_BINARY)" "$(BINDIR)/$(BINARY)"
	@printf 'Installed build identity (expected commit %s):\n' "$(CURRENT_GIT_SHA)"
	"$(BINDIR)/$(BINARY)" --version --verbose
	@$(MAKE) --no-print-directory install-resolution-warning
	@printf 'Reinstalled %s\n' "$(BINDIR)/$(BINARY)"

install-user: release
	mkdir -p "$$HOME/.local/bin"
	install -m755 "$(RELEASE_BINARY)" "$$HOME/.local/bin/$(BINARY)"
	@printf 'Installed build identity (expected commit %s):\n' "$(CURRENT_GIT_SHA)"
	"$$HOME/.local/bin/$(BINARY)" --version --verbose
	@case ":$$PATH:" in *":$$HOME/.local/bin:"*) : ;; *) printf '%s\n' 'Warning: $$HOME/.local/bin is not on PATH.' ;; esac
	@printf 'Installed %s\n' "$$HOME/.local/bin/$(BINARY)"

install-resolution-warning:
	@resolved=$$(command -v "$(BINARY)" || true); \
	if [ -n "$$resolved" ] && [ "$$resolved" != "$(BINDIR)/$(BINARY)" ]; then \
		printf '\nWarning: your shell resolves %s to %s, not %s\n' "$(BINARY)" "$$resolved" "$(BINDIR)/$(BINARY)"; \
		printf '%s\n' 'Run make install-check to inspect the resolved build identity.'; \
		printf '%s\n' 'If you intend to use the user-local copy, rebuild it explicitly with: make install-user'; \
	fi

install-check:
	@resolved=$$(command -v "$(BINARY)" || true); \
	if [ -z "$$resolved" ]; then \
		printf '%s\n' 'allp was not found on PATH.'; \
		printf '%s\n' 'Run make install, then refresh your shell command cache with hash -r or rehash.'; \
		exit 1; \
	fi; \
	printf 'Resolved allp: %s\n' "$$resolved"; \
	"$$resolved" --version --verbose; \
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

release-push:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-push.sh

release-clean:
	rm -rf "$(DIST_DIR)" .release-state
	@printf '%s\n' 'Removed ignored local release output.'

hooks-install:
	git config core.hooksPath .githooks
	git config push.followTags true
	@printf '%s\n' 'Installed local Git hooks from .githooks/.'
	@printf '%s\n' 'Configured push.followTags=true for this repository.'

hooks-status:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-status.sh --hooks-only

release-workflow-test:
	$(BASH) scripts/test-release-workflow.sh
	$(BASH) scripts/test-release-assets.sh

release-assets-test:
	$(BASH) scripts/test-release-assets.sh
