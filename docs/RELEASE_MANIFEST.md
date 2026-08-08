# Release Manifest

Tag-triggered GitHub Releases include `allp-release-manifest.json` with schema version 1, release identity, channel, publication time, minimum updater version, and one entry per built/tested binary target.

Each asset records target triple, OS, architecture, optional libc, archive name, binary name, SHA-256, and byte size. Archive and binary fields are safe basenames; duplicate targets, zero sizes, malformed checksums, mismatched tag/version, and unsupported channels are rejected.

Current release workflow targets are:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

`scripts/generate-release-manifest.py` derives entries only from target archives with valid adjacent checksum files and verifies the generated manifest against `dist/`. The GitHub workflow runs only for semantic-version tag pushes and refuses to silently overwrite an existing Release.

## Continuous manifest

Main-branch builds use the separate `allp-continuous-manifest.json` schema. It records base version, build revision, display version, an exact 40-character SHA-1 or 64-character SHA-256 Git commit ID, build/run IDs, trusted workflow name/path, UTC build time, minimum updater version, and the same target/size/SHA-256 asset identity as stable releases. `scripts/generate-continuous-manifest.py` generates and verifies it; `scripts/test-continuous-manifest.sh` exercises valid, invalid-commit, and tampered-asset cases.

The continuous workflow names its authoritative Actions artifact `allp-continuous-<display>-<manifest-sha256>`, requiring the artifact identity to bind the exact mirror manifest bytes. Allp also requires GitHub's non-empty, unexpired artifact and SHA-256 digest metadata before trusting the prerelease transport. The exact verified files are mirrored to a non-stable prerelease for anonymous downloads. An existing continuous tag can be updated only by a rerun attempt with the same workflow run ID and commit; identity reuse by another run fails publication. Tagged `vX.Y.Z` release manifests and `.github/workflows/release.yml` remain independent.
