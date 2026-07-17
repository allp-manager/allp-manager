# Allp Roadmap

The roadmap describes intended direction, not a guarantee. Allp will keep native package managers as the source of truth.

## v0.3.3

- APT
- Pacman
- DNF / DNF5
- Zypper
- APK
- XBPS
- Portage / emerge
- eopkg
- swupd
- Flatpak
- Snap
- Homebrew / Linuxbrew
- Python: PyPI with pip, pipx, uv
- Node.js: npm registry with npm, pnpm, Yarn
- `detect`
- `search`
- `install`
- `remove`
- `update`
- `upgrade`
- `list`
- `info`
- command-first CLI
- bounded search UX
- `Exact`, `Related`, and `Fuzzy` ranking
- dry run
- JSON envelopes
- stable alpha exit codes
- architecture guardrails
- fake-PATH integration tests
- public alpha documentation
- mandatory final confirmation for all real mutating operations
- `--yes` as an Allp-only final-confirmation bypass
- Python and Node update/upgrade targets with precise skip reasons
- official software identity resolution for known package-manager names
- Homebrew official bootstrap candidate
- conflicting registry package-name warnings

## Next

- `doctor`
- diagnostics and support bundles
- deeper backend probes
- broader real-distro validation for experimental backends
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
