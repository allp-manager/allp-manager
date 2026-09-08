# Allp v0.6.2 — Reliable Package Profile Snapshots

Allp v0.6.2 is a focused hotfix for package-profile creation.

## Fixed

- A detected Python runtime without `pip` now contributes an empty
  pip-managed inventory instead of preventing `allp profile save` from
  completing.
- When another backend genuinely cannot list its installed packages,
  `profile save` now prints the backend name and exact native error.
- Allp continues to reject incomplete snapshots and explicitly confirms that
  no partial profile was written.

## Validation

- A real isolated profile snapshot completed successfully with 2,906 packages.
- 325 automated tests passed.
- Formatting, all-target checks, Clippy with warnings denied, architecture
  boundaries, release build, and documentation checks passed.

## Upgrade

After installing v0.6.2, the original command should work without installing
`pip` solely for profile discovery:

```console
allp profile save dev
allp profile show dev
```

See the `[0.6.2]` section in `CHANGELOG.md` for the complete change list.

## Local Release Output

- Source archive: `dist/allp-v0.6.2-source.tar.gz`
- SHA-256 file: `dist/allp-v0.6.2-source.tar.gz.sha256`
- Finalized notes: `dist/RELEASE_NOTES_v0.6.2.md`

The archive is generated from the exact annotated tag `v0.6.2` after the release commit.

## Checksum

SHA256: _generated during finalization_
