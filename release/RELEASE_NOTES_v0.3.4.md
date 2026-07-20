# Allp v0.3.4

Allp v0.3.4 establishes the cross-platform backend foundation, restores reliable Snap and Flatpak recovery paths, and adds a verified self-update and release pipeline.

## Highlights

- Snap now uses snapd REST for wide discovery, exact resolution, installation requests, and asynchronous change monitoring. An authoritative `404 snap-not-found` remains unavailable and never falls through to the Snap CLI, sudo, or install.
- Snap CLI fallback is limited to missing, denied, or unreachable sockets and unsupported or unrecognized REST responses. Diagnostics record the fallback reason.
- Exact Snap metadata carries the canonical name, publisher verification, confinement, architectures, channels, stable availability, and installed state. Classic packages produce the required classic install plan without package-specific hardcoding.
- Flatpak executable availability and configured remotes are modeled separately. Search uses native remote search, zero-remotes is explicit, and adding Flathub is a separately displayed user-scoped mutation.
- Alternative-installer routing performs a fresh search, excludes the failed backend, and never reuses a stale Snap candidate.

## Platform And Bootstrap

- A shared platform context detects OS, Linux distribution family, architecture, libc, WSL/container state, user/root context, executable ownership and writability, release target, and XDG paths.
- A central capability registry records resolved executables and reasons for unavailability.
- Structured requirements and bootstrap providers cover Flatpak and Snap prerequisites through APT, DNF, Pacman, Zypper, and APK where mappings are known.
- Prerequisite installation and Flatpak remote setup require a separate displayed plan. `--yes` alone does not approve bootstrap; non-interactive approval requires `--yes --allow-bootstrap`.

## Secure Self-Update

- `allp self-update` and the self-update phase of `allp update` trust only `allp-manager/allp-manager` release metadata.
- Stable/prerelease channels, strict three-part SemVer, ETags, offline state, minimum-updater versions, and exact platform target selection are supported.
- Downloads require HTTPS, an exact manifest entry, declared size, and SHA-256 verification before bounded archive extraction.
- Archive traversal, links, and special entries are rejected. Staged binaries must report the expected version.
- Unix replacement is atomic, preserves destination permissions and ownership, verifies the installed binary, and restores the previous binary on failure.
- Windows replacement is deferred to a helper process, then re-executes with a completion guard so updates do not loop.
- A failed or unsupported self-update can be reported without damaging the current installation, and ordinary backend updates can continue when the selected policy permits it.

## Diagnostics And Commands

- `allp doctor` reports platform, install-path ownership, release target, state paths, Snap socket reachability, Flatpak remotes, executable capabilities, backend readiness, and the trusted update source without printing credentials.
- Update controls include `--check-only`, `--skip-self-update`, `--self-only`, and `--offline`.
- Dry-run and JSON update output remain non-mutating and machine-readable.

## Release Assets

The tag-only GitHub workflow builds and verifies:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Each binary archive has a SHA-256 file. The release also includes the exact-tag source archive, its checksum, prepared notes, and `allp-release-manifest.json`. Every manifest asset is verified before publication.

## Test Coverage

- 176 Rust unit and integration tests pass, including fake snapd Unix sockets, fake package executables, prompt/input regressions, prerequisite safety, alternative routing, checksum verification, rollback, Windows deferral, offline state, and guarded re-execution.
- Formatting, all-target checking, Clippy with warnings denied, architecture boundaries, release build, Windows cross-check, release-asset tests, and temporary-repository release automation pass.
- Current-machine acceptance confirmed REST discovery, authoritative exact Snap 404 handling, zero Flatpak remotes, fresh alternatives with Snap excluded, clean cancellation, and official GitHub update checks.

## Known Limitations

- Stable Snap selection remains conservative when multiple tracks are available; Allp does not silently choose a track or a non-stable risk channel.
- A valid exact snapd not-found response is authoritative even when wide discovery returned a similarly named candidate.
- Flatpak catalog search requires at least one configured remote; Allp never adds Flathub automatically.
- Self-update requires a supported target entry in a valid release manifest and local archive tools. Unsupported targets remain on the installed version.
- Experimental Linux-family, Homebrew, Python, and Node paths still need broader validation on real hosts and project layouts.

## Local Release Workflow

After this prepared release commit is made with a subject beginning `release:`, the repository-local hook creates only the annotated local tag and exact-tag source outputs under ignored `dist/`. It never pushes or publishes. Only an explicitly pushed semantic-version tag can trigger the GitHub Release workflow.
