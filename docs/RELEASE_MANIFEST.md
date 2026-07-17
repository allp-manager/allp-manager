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
