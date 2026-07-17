# Allp TODO

This file tracks implementation work. Product direction lives in `ROADMAP.md`.

## v0.1 Alpha Readiness

- [x] Command-first CLI options
- [x] Fresh discovery on every invocation
- [x] Explicit detection states
- [x] Capability-based backend contract
- [x] APT backend
- [x] Pacman backend
- [x] DNF / DNF5 backend
- [x] Flatpak backend
- [x] Snap backend
- [x] Experimental Zypper backend
- [x] Experimental APK backend
- [x] Experimental XBPS backend
- [x] Experimental Portage/emerge backend
- [x] Experimental eopkg backend
- [x] Experimental swupd backend
- [x] Experimental Homebrew/Linuxbrew backend
- [x] Experimental Python backend with PyPI, pip, pipx, and uv
- [x] Experimental Node backend with npm registry, npm, pnpm, and Yarn
- [x] `detect`
- [x] `search`
- [x] `install`
- [x] `remove`
- [x] `update`
- [x] `upgrade`
- [x] `list`
- [x] `info`
- [x] `Exact` / `Related` / `Fuzzy` search classification
- [x] bounded default search output
- [x] incomplete-search uniqueness prevention
- [x] non-interactive ambiguity exit code
- [x] dry-run execution skip
- [x] privilege explanation before sudo can prompt
- [x] confirmation before real update/upgrade execution
- [x] JSON envelopes
- [x] stable alpha exit-code map
- [x] backend-diverse search limiting
- [x] automatic pager for large list output
- [x] curated info output with `--full` and `--raw`
- [x] colored terminal status icons with color opt-outs
- [x] unified privilege model for normal, root, and sudo-invoked execution
- [x] architecture boundary check
- [x] fake-PATH integration tests
- [x] release build command

## Remaining v0.1 Alpha Limitations

- [ ] Validate parsers across more real distro/package-manager versions
- [ ] Add richer backend-owned parser fixtures for stable and experimental backends
- [ ] Add deeper `detect --verbose` probes for Flatpak remotes and backend versions
- [ ] Validate Homebrew on macOS and Linuxbrew hosts
- [ ] Validate Python PEP 668 and virtual environment edge cases
- [ ] Harden Node project/workspace install policy before project-scope mutation support
- [ ] Add signal forwarding and Ctrl+C process-group tests
- [ ] Harden trusted-path validation before root elevation
- [ ] Replace placeholder repository URL in `Cargo.toml`
- [ ] Publish installation instructions

## v0.2

- [ ] `doctor`
- [ ] diagnostics support bundle
- [ ] safe cleanup where supported

## v0.3

- [ ] Cargo backend
- [ ] Composer backend
- [ ] Go backend

Development package managers require explicit scope handling and must not silently modify lockfiles.

## Explicit Non-Goals

- GUI in v0.1
- TUI in v0.1
- plugin marketplace
- telemetry
- recommendation engine
- background daemon
- Allp-owned package cache
- automatic confirmation flags
- universal undo
