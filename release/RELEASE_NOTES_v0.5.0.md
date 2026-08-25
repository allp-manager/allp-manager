# Allp v0.5.0

Allp v0.5.0 expands the existing Homebrew, Node, and Python orchestration with
Rust/Cargo binary-tool management and transactional Bazzite/rpm-ostree support.

## Highlights

- Search, inspect, install, remove, and list crates.io binary tools through the
  new `rust` backend and its `cargo`, `crates.io`, and related aliases.
- Upgrade globally installed Cargo binaries through optional `cargo-update`
  without modifying project manifests or lockfiles.
- Detect and probe rpm-ostree deployments, search RPM repositories, list
  requested layers, and stage package install/remove operations.
- Refresh rpm-md metadata with `allp update --from bazzite` and stage the next
  system image with `allp upgrade --from bazzite`.
- Disable DNF host mutations on Bazzite so executable helper tools cannot route
  host changes through the wrong package manager.
- Preserve Homebrew's validated owner-scoped execution and official bootstrap
  path alongside the new development and image-based Linux backends.

See `CHANGELOG.md` section `[0.5.0]` for the complete change record.

## Safety Boundaries

- Cargo operations run as the selected/original user and warn that crates can
  compile and execute build scripts.
- Cargo host maintenance never runs `cargo add` or project `cargo update`.
- rpm-ostree mutations are shown as root-required transactional plans, state
  their reboot requirement, and label host layering as a last resort.
- `ujust update` is not used because it would cross system, Flatpak, and
  container backend boundaries.

## Validation

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo test --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `bash scripts/check-architecture.sh`
- `cargo build --release`
- `make docs-check`

## Host Follow-up

- Validate Cargo search/info output and `cargo-update` upgrades on real Rust
  installations across Linux, macOS, and Windows.
- Validate rpm-ostree search/status output and staged deployment behavior on a
  physical Bazzite host before promoting the backend from experimental.

## Release Assets

The tag workflow builds the advertised Linux, macOS, and Windows archives,
checksums, source archive, and `allp-release-manifest.json` from the exact
annotated `v0.5.0` tag.
