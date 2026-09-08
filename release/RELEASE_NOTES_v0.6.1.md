# Allp v0.6.1 — Safer Search, Updates, and Package Profiles

Allp v0.6.1 adds experimental, portable package profiles and completes the
search, identity, and self-update reliability work prepared for the 0.6 line.

## Highlights

- Save the installed package inventory as a named profile, then list, inspect,
  export, import, dry-run, and install it on another system.
- Keep profiles reviewable and portable with a versioned TOML format containing
  explicit backend/package pairs.
- Validate imported package identifiers and preflight every required backend
  before any profile package is installed.
- Distinguish real no-match responses from unrecognized or partially parsed
  native output across APT, DNF, Pacman, and Flatpak.
- Group verified cross-backend identities while keeping probable, conflicting,
  and unverified results visibly separate; meaningful source choice remains
  with the user.
- Default fresh installations to the stable update channel and preserve the
  origin of explicit and migrated channel choices.
- Detect dpkg-, rpm-, and Pacman-owned binaries so self-update never overwrites
  files managed by the native package manager.
- Move CLI JSON to schema version 2 with effective search scope, candidate
  groups, backend summaries, and structured parser issues.

## Package Profile Quick Start

```console
allp profile save dev
allp profile show dev
allp profile export dev dev.toml
allp profile import dev.toml
allp profile install dev --dry-run
allp profile install dev
```

Profile support is experimental. It preserves backend-qualified package IDs
and does not translate them across distributions. Saved versions describe the
captured inventory but are not installation pins. System package inventories
may also include automatically installed dependencies.

## Known Limitations

- Profile installation is sequential after backend preflight. A package missing
  from an available backend can still stop a run after earlier packages have
  been installed.
- Interactive APT upgrades in the live TUI can hide the native confirmation
  prompt. Use `allp upgrade --yes` for an approved unattended run or
  `allp upgrade --no-tui` to keep the native prompt visible.

## Validation

- 323 automated tests passed.
- Formatting, all-target checks, Clippy with warnings denied, architecture
  boundaries, release build, and documentation checks passed.

See the `[0.6.1]` section in `CHANGELOG.md` for the complete change list.

## Local Release Output

- Source archive: `dist/allp-v0.6.1-source.tar.gz`
- SHA-256 file: `dist/allp-v0.6.1-source.tar.gz.sha256`
- Finalized notes: `dist/RELEASE_NOTES_v0.6.1.md`

The archive is generated from the exact annotated tag `v0.6.1` after the release commit.

## Checksum

SHA256: _generated during finalization_
