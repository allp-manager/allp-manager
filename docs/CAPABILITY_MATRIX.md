# Capability Matrix

Legend:

- `Stable`: implemented for public-alpha testing
- `Experimental`: implemented but behavior needs wider distro validation
- `Detection only`: backend can be detected but the operation is not supported
- `Unsupported`: not advertised

Stable rows have the strongest test and parser coverage. Experimental rows are implemented and tested with fake executables, but still need broader real-system validation before being called stable.

| Backend | Detect | Search | Install | Remove | Update | Upgrade | List | Info |
|---|---|---|---|---|---|---|---|---|
| APT | Stable | Stable | Stable | Stable | Stable | Stable | Stable | Stable |
| Pacman | Stable | Stable | Stable | Stable | Stable | Stable | Stable | Stable |
| DNF / DNF5 | Stable | Stable | Stable | Stable | Stable | Stable | Stable | Stable |
| Zypper | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental |
| APK | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental |
| XBPS | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental |
| Portage / emerge | Experimental | Experimental | Experimental | Unsupported | Experimental | Experimental | Unsupported | Experimental |
| eopkg | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental |
| swupd | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental |
| Flatpak | Stable | Stable | Stable | Stable | Stable | Stable | Stable | Stable |
| Snap | Stable | Stable | Stable | Stable | Stable | Stable | Stable | Stable |
| Homebrew / Linuxbrew | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental |
| PyPI + pip/pipx/uv | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental |
| npm registry + npm/pnpm/Yarn | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental | Experimental |
| Nix / NixOS | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| rpm-ostree | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| transactional-update | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Guix | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Slackware tools | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Cargo | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Composer | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |
| Go | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported | Unsupported |

Unsupported rows are roadmap markers or explicitly deferred ecosystems, not hidden features.

Snap `Install` uses snapd REST wide discovery plus separate exact name resolution when the local socket is reachable. A valid `404 snap-not-found` is authoritative stale metadata and cannot trigger CLI fallback. CLI fallback is restricted to concrete socket/connect/endpoint/response failures. Classic confinement, canonical names, publisher verification, stable channels, architecture, and installed state are resolved before any install plan; REST changes are monitored to a terminal state.

Flatpak `Search` requires both an available executable and at least one configured remote. Installed-without-remotes is a distinct supported diagnostic state, not a no-match result. Flathub setup is an explicit separately confirmed bootstrap action.
