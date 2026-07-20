# Allp v0.3.5

Allp v0.3.5 adds Pacman participation in `allp update` while keeping the native command visible and separately confirmed.

## Changes

- Pacman now advertises `Update` in backend capability reporting.
- `allp update --from pacman` plans `pacman -Sy` as a package-database synchronization step.
- The Pacman update plan prints a policy detail warning that `pacman -Sy` refreshes databases only and that users should run a full upgrade before installing packages to avoid partial upgrades.
- English and Persian README capability tables, the capability matrix, Linux coverage notes, and command docs now describe Pacman update support.

## Validation

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-targets`
- `bash scripts/check-architecture.sh`
- `cargo build --release`
- `make docs-check`

## Local Release Output

- Source archive: `dist/allp-v0.3.5-source.tar.gz`
- SHA-256 file: `dist/allp-v0.3.5-source.tar.gz.sha256`
- Finalized notes: `dist/RELEASE_NOTES_v0.3.5.md`

The archive is generated from the exact annotated tag `v0.3.5` after the release commit.

## Checksum

SHA256: _generated during finalization_
