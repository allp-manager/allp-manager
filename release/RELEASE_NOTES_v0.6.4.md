# Allp v0.6.4 — Correct Native Command Identity

This patch fixes a root-execution regression affecting multicall package-manager
symlinks and aligns ordinary APT upgrades with new-dependency upgrades.

## Fixes

- Preserve the validated invocation path through direct-root and sudo execution.
  On systems where `/usr/bin/snap` links to `/usr/lib/snapd/snapd`, `snap refresh`
  now remains a CLI invocation instead of starting a competing daemon.
- Continue validating root ownership and permissions of the canonical executable
  and its directories, and additionally validate the invocation symlink chain.
  User-controlled aliases to trusted executables remain rejected.
- Add `--with-new-pkgs` to APT upgrades so required new dependencies can be
  installed without automatic package removals or full/dist-upgrade routing.
- Classify Snap state-lock timeouts as busy, retaining lock diagnostics and the
  native command. Merely mentioning `snapd` or `daemon` no longer implies that
  the daemon is unavailable.

## Validation

The full quality gate covers formatting, all-target checking, Clippy with
warnings denied, 332 tests, architecture boundaries, optimized builds, and
documentation checks. Regression coverage includes repeated Snap symlink runs,
classic/TUI output, root-direct/sudo preparation, untrusted aliases, new APT
dependencies, and a real state-lock-timeout fixture.

## Installation note

A user-local Allp can shadow a separate system installation. Update both if
ordinary `allp` and `sudo allp` resolve to different installed binaries. Do not
delete Snap state locks or stop its healthy daemon to work around this bug.

## Local source release output

- Source archive: `dist/allp-v0.6.4-source.tar.gz`
- SHA-256 file: `dist/allp-v0.6.4-source.tar.gz.sha256`
- Finalized notes: `dist/RELEASE_NOTES_v0.6.4.md`

The source archive is generated from the annotated `v0.6.4` tag.
