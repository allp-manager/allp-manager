# Changelog

All notable changes to Allp will be documented in this file.

## [Unreleased]

## [0.3.4] - 2026-07-18

### Release Title

Allp v0.3.4 — Modular Backend Recovery and Secure Self-Update

### Added

- Cross-platform platform context, shared capability registry, structured backend requirements, and APT/DNF/Pacman/Zypper/APK prerequisite providers.
- Primary snapd REST discovery, exact resolution, installation requests, and asynchronous change monitoring, with reasoned CLI fallback.
- Explicit Flatpak installed-without-remotes state and separately confirmed user-scoped Flathub setup.
- Fresh alternative-installer routing that excludes failed backends and discards cached candidates.
- Trusted GitHub self-update with stable/prerelease channels, strict SemVer, release manifests, target selection, ETags, SHA-256 verification, safe extraction, atomic rollback, Windows deferral, and guarded continuation.
- `allp doctor`, platform-aware state paths, Linux/macOS/Windows CI coverage, target binary archives, and release-manifest generation.

### Changed

- `allp update` now has explicit self-update, platform refresh, planning, confirmation, execution, and summary phases, with skip/self-only/check-only/offline controls.
- Prerequisite and remote mutations require separate approval; `--yes` alone cannot bootstrap them, while `--yes --allow-bootstrap` can approve a displayed plan.
- Release CI now builds only advertised native targets and verifies every binary checksum and manifest entry before tag-only publication.

### Fixed

- A valid snapd `404 snap-not-found` is authoritative unavailable and cannot fall through to `snap info`, sudo, or installation.
- Flatpak without remotes is no longer misreported as a package no-match state.
- `Try another installer` no longer reuses or redisplays the failed Snap result.
- Root-owned executable writability uses effective UID/group permissions, and replacement preserves mode/ownership with rollback.

### Known Limitations

- Snap stable-track selection remains conservative when multiple tracks require an explicit choice.
- Existing GitHub releases without a valid target manifest cannot be consumed by automatic self-update.

## [0.3.3] - 2026-07-17

### Release Title

Allp v0.3.3 - Snap Validation and Repository Stabilization

### Added

- Software identity metadata for candidates: name match kind, identity confidence, distribution relationship, software type, canonical identity, official-source flag, and warnings.
- Curated canonical identity catalog for Homebrew, system package managers, universal app managers, Python tools, and Node tools.
- Official Homebrew bootstrap candidate with an explicit native plan that downloads the official installer to a temp file before running it with `/bin/bash`.
- Documentation for software identity, official bootstrap behavior, name collisions, Homebrew bootstrap, and the v0.3.3 test plan.
- Snap install metadata validation through `snap info` after candidate selection.
- Explicit Snap discovery and resolution states so `snap find` rows are treated as unverified discovery candidates until exact metadata resolves.
- Canonical Snap package-name resolution, publisher verification normalization, channel metadata, architecture checks, and installed-state preflight.
- Separate Snap diagnostics for wide discovery and exact resolution commands, including candidate state, stdout, stderr, and exit codes.
- Classic-confinement Snap install plans, including `snap install <package> --classic` when metadata requires it.
- Safe root `Makefile` targets for formatting, checking, testing, architecture checks, release build, quality gate, docs check, running, version, and Git status.
- Source installation targets for `/usr/local/bin/allp`, user-local installs, reinstall/uninstall checks, and PATH diagnostics.
- A local-only release workflow with `make hooks-install`, `make release-prepare`, post-commit release finalization, annotated local tags, source archives, checksums, and finalized local release notes.
- Release title files, `make release-push`, and a tag-triggered GitHub Actions workflow for creating GitHub Releases only from pushed semantic-version tags.
- Temporary-repository release automation tests that avoid creating tags or artifacts in the developer repository.
- Repository-specific `.gitignore` coverage for build output, logs, temp files, editor state, local env files, secrets, caches, and generated packages.
- Complete English and Persian README files for v0.3.3.

### Changed

- Official installer candidates rank before registry package-name collisions.
- Search and install output now labels identity relationships such as `Official installer`, `Exact package name`, and `Conflicting name`.
- `allp install Homebrew` no longer treats the unrelated npm package named `homebrew` as the Homebrew package manager.
- npm global installs preflight the configured global prefix for current-user writability before real execution.
- Snap search publishers such as `jetbrains**` are normalized into publisher name plus verification state instead of storing decoration as part of the publisher.
- Snap search output now labels wide search results as discovery/search matches with availability not yet verified instead of installable exact package names.
- Snap install plans now include release-relevant details such as software title, publisher, channel, confinement, and architectures when available.
- Generated `cargo-check.log` is no longer tracked.
- The crate repository URL now matches the real project remote.
- Local release output under `dist/` and readiness markers under `.release-state/` are ignored while `.githooks/`, release titles, release-note drafts, scripts, documentation, and release metadata remain trackable.

### Fixed

- npm `homebrew` is labeled as a conflicting exact-name match and requires separate default-No identity confirmation for real installation.
- `--yes` does not bypass conflicting-identity confirmation.
- DNF rpmdb failures and missing pip failures now produce targeted Allp diagnostics.
- Raw `snap find` rows no longer become install plans directly.
- Snap search rows that fail exact resolution are reported as unavailable candidates with search status, install status, and native error text.
- PyCharm-like classic Snap packages now plan `snap install <package> --classic` after metadata validation.
- Strict Snap packages no longer receive `--classic`.
- Stale or unavailable Snap search results fail before execution with a targeted diagnostic.
- Snap packages without stable availability, unsupported architecture metadata, or ambiguous stable tracks are blocked before normal install planning.

### Known Limitations

- Snap channel selection is conservative in v0.3.3; when multiple stable tracks need a human choice, Allp stops instead of silently selecting a channel.
- Broader real Snap Store output coverage is still needed beyond the fake fixtures added for this stabilization pass.

## [0.3.2] - 2026-07-15

### Release Title

Allp v0.3.2 — Confirmed Operations and Developer Ecosystem Updates

### Added

- Mandatory final Allp confirmation for every real mutating operation, including one exact install result.
- `--yes` / `-y` to bypass only Allp's final confirmation after choices are fully resolved.
- `--target` for development update and upgrade targets: `project`, `workspace`, `global`, `environment`, `tools`, and `all`.
- Python and Node participation in `allp update` and `allp upgrade` with target-level plans or precise skip reasons.
- npm project/global inspection through native outdated JSON before planning `npm update` or `npm update --global`.
- pnpm project, workspace, global, and latest-upgrade plans using native pnpm commands.
- Yarn major-version detection with Yarn 1 and modern Yarn update command mapping.
- pip active-environment outdated inspection through structured JSON and `python -m pip install --upgrade ...` plans.
- `pipx upgrade-all` and `uv tool upgrade --all` plans for isolated Python tools.

### Changed

- Remove confirmation now defaults to No.
- Upgrade batch confirmation defaults to No for riskier operations.
- Dry runs build real plans but never ask for execution confirmation or invoke sudo.
- Maintenance summaries show skip reasons by default.
- Original-user execution now restores the original user's HOME when de-escalating from sudo.

### Fixed

- Node and Python no longer appear as generic silent skipped backends during update/upgrade.
- Allp running as root still requires operation confirmation but never adds nested sudo.
- `--yes` never adds native package-manager auto-confirm flags.

### Known Limitations

- pip package selection is currently represented in the generated plan from inspected outdated packages; a richer interactive per-package selector remains planned.
- npm latest-crossing project upgrades are conservative and do not invoke external updaters such as npm-check-updates automatically.
- Yarn modern project support uses version-aware native commands, but deeper workspace package selection remains experimental.

### Added

- Command-specific Clap option structs and command-first examples.
- Explicit detection states and capability data in detection reports.
- `Exact`, `Related`, and `Fuzzy` search ranking.
- Bounded default search visibility with per-backend related limits.
- Versioned JSON envelopes for detect, search, list, info, and dry-run maintenance commands.
- Stable alpha exit-code model.
- Backend action descriptions in execution plans and maintenance summaries.
- Per-query timeout handling for captured native query commands.
- Fake-PATH integration tests for discovery, search, install, remove, update dry run, partial failure, JSON purity, and shell-injection resistance.
- Documentation for CLI, JSON, backend contract, capability matrix, and security model.
- Privilege explanation before root-required child execution.
- Interactive confirmation before real update/upgrade execution.
- Snap usability probe during discovery.
- Backend-diverse search limiting.
- Automatic pager support for large human-readable list output.
- `list --filter`, `list --limit`, and `list --no-pager`.
- Curated `info` output with `--full` and `--raw`.
- Experimental Linux package-manager family coverage for Zypper, APK, XBPS, Portage/emerge, eopkg, and swupd.
- Experimental Homebrew/Linuxbrew backend.
- Experimental Python ecosystem backend with PyPI source and pip/pipx/uv installer choices.
- Experimental Node ecosystem backend with npm registry source and npm/pnpm/Yarn installer choices.
- Package-domain grouping for system, universal, Homebrew, Python, and Node candidates.
- Central `PrivilegeRequirement` and runtime privilege context model.
- Original-user de-escalation for sudo-invoked user-scoped plans.
- Colored terminal status icons with `NO_COLOR`, `--no-color`, non-TTY, `TERM=dumb`, and JSON safeguards.
- Documentation for privilege, terminal UI, Linux coverage, Homebrew, Python, and Node.

### Changed

- `install` no longer discards meaningful related matches when one exact match exists across backends.
- `remove` keeps related installed copies visible instead of stopping at the first exact installed match.
- Pacman no longer advertises APT-style `Update`; `Upgrade` uses `pacman -Syu`.
- Human output now uses labels such as `Exact`, `Related`, `Dry run`, and `Failed` instead of raw Rust enum debug forms.
- Multi-backend update/upgrade return exit code `8` on partial failure.
- Update/upgrade output now separates detected-ready backends from selected backends and avoids repeating full commands in the final summary.
- Root-required plans no longer use a backend-local sudo flag; the central runner handles root, normal-user, and original-user execution.
- Python and Node fuzzy registry matches are not automatically installed.

### Known Limitations

- Parser behavior still needs broader validation across real distributions and package-manager versions.
- `detect --verbose` still has limited probes beyond Snap usability.
- Trusted-path validation before root elevation needs hardening before stable release.
- Signal forwarding and process-group cancellation need deeper tests.
- Experimental backend validation is still needed on real Zypper, APK, XBPS, Portage, eopkg, swupd, Homebrew, Python, and Node hosts.
- Python/Node registry search and project-scope policy need deeper hardening before stable release.

## [0.1.0-alpha.2] - 2026-07-13

### Fixed

- Fixed duplicate `Renderer::info` method names.
- Allowed JSON serialization of slices.
- Fixed Pacman parser mutable-borrow conflict.
- Fixed partial moves when reporting install and remove failures.

## [0.1.0-alpha.1] - Unreleased

Initial public alpha target.
