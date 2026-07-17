# Changelog

All notable changes to Allp will be documented in this file.

## [Unreleased]

## [0.3.3] - 2026-07-15

### Release Title

Allp v0.3.3 - Official Software Resolution

### Added

- Software identity metadata for candidates: name match kind, identity confidence, distribution relationship, software type, canonical identity, official-source flag, and warnings.
- Curated canonical identity catalog for Homebrew, system package managers, universal app managers, Python tools, and Node tools.
- Official Homebrew bootstrap candidate with an explicit native plan that downloads the official installer to a temp file before running it with `/bin/bash`.
- Documentation for software identity, official bootstrap behavior, name collisions, Homebrew bootstrap, and the v0.3.3 test plan.

### Changed

- Official installer candidates rank before registry package-name collisions.
- Search and install output now labels identity relationships such as `Official installer`, `Exact package name`, and `Conflicting name`.
- `allp install Homebrew` no longer treats the unrelated npm package named `homebrew` as the Homebrew package manager.
- npm global installs preflight the configured global prefix for current-user writability before real execution.

### Fixed

- npm `homebrew` is labeled as a conflicting exact-name match and requires separate default-No identity confirmation for real installation.
- `--yes` does not bypass conflicting-identity confirmation.
- DNF rpmdb failures and missing pip failures now produce targeted Allp diagnostics.

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
