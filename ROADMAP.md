# Allp Roadmap

This roadmap describes direction, not a promise. Allp keeps native package managers as the source of truth.

## Current Release: v0.3.4

v0.3.4 is a public-alpha foundation and recovery release with:

- command-first CLI for `detect`, `search`, `install`, `remove`, `update`, `upgrade`, `list`, and `info`;
- interactive search-scope selection;
- paged and numbered install/search result selection;
- `Exact`, `Related`, and `Fuzzy` ranking;
- bounded default search visibility;
- mandatory final confirmation for real mutating operations;
- `--yes` as an Allp-only final-confirmation bypass;
- dry-run execution planning;
- JSON envelopes for supported commands;
- centralized normal-user, root, and original-sudo-user privilege handling;
- live execution progress and native output streaming;
- stable alpha system/universal backends: APT, Pacman, DNF/DNF5, Flatpak, Snap;
- experimental system-family backends: Zypper, APK, XBPS, Portage/emerge, eopkg, swupd;
- experimental Homebrew/Linuxbrew support and official bootstrap planning;
- experimental Python support for PyPI with pip, pipx, and uv;
- experimental Node support for the npm registry with npm, pnpm, and Yarn;
- software identity warnings for known name collisions;
- Snap metadata revalidation before install planning, including canonical names, verified publishers, confinement, stable channels, architecture checks, and installed-state preflight;
- snapd REST as the primary Snap transport, authoritative exact not-found handling, REST installation, and terminal change monitoring;
- explicit Flatpak no-remotes state and separately confirmed Flathub setup;
- structured prerequisite requirements, distro-family bootstrap providers, and `--allow-bootstrap` safety;
- fresh alternative-installer routing with failed-backend exclusions;
- secure official GitHub self-update with strict manifests, checksums, target selection, rollback, Windows deferral, and guarded continuation;
- cross-platform platform/capability diagnostics through `allp doctor`;
- target-specific Linux, macOS, and Windows release assets and manifest generation;
- source installation through the root Makefile;
- local release preparation/finalization with tracked release titles and notes, ignored local artifacts, annotated local tags, tag-triggered GitHub Releases, and temp-repository automation tests;
- repository hygiene, Makefile workflow, and refreshed English/Persian documentation.

## Next Stabilization

- Broaden parser fixtures with real native outputs for stable and experimental backends.
- Validate experimental Linux-family backends on real distributions.
- Validate Homebrew on macOS and Linuxbrew hosts.
- Expand Python PEP 668 and virtual-environment safety coverage.
- Harden Node project/workspace mutation policy.
- Add a richer interactive Snap channel chooser for multi-track and non-stable choices.
- Improve `detect --verbose` diagnostics for remotes, versions, and configuration.
- Add signal forwarding and Ctrl+C process-group tests.
- Keep tightening trusted-path checks before root elevation.

## Future Commands

- redacted diagnostics/support bundle export
- safe cleanup where native backends support it

## Future Development Ecosystems

- Cargo
- Composer
- Go
- RubyGems
- Maven/Gradle

Development package managers need explicit scopes:

- global tools
- user tools
- active virtual environments
- current projects
- lockfile-owned dependencies

Allp must not silently modify project lockfiles through generic install, update, or upgrade flows.

## Later

- export/import
- history/replay
- configuration
- external backend protocol
- TUI
- GUI
- API/SDK

Allp will never promise universal undo.
