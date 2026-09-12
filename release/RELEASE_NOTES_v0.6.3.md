# Allp v0.6.3 — Reliable Interactive Maintenance

Allp v0.6.3 fixes the interactive maintenance path that could leave APT or
another native package manager stopped behind the live progress display.

## Fixed

- Answering `y` at Allp's maintenance confirmation now finalizes the same
  backend-native noninteractive command as `--yes`. A confirmed APT upgrade is
  displayed and executed as `apt-get ... upgrade -y`, so APT cannot wait on a
  redundant hidden confirmation.
- Interactive native children remain in Allp's foreground process group, so
  reading from the controlling terminal no longer stops them with `SIGTTIN`.
- Native prompts without a trailing newline are shown immediately through the
  terminal-safe TUI projection. The footer stays suspended while the prompt is
  visible and terminal control sequences remain sanitized.

## Validation

- `make quality` passed on the release version.
- 327 automated tests passed, including PTY regressions for interactive APT
  approval, newline-less native prompts, foreground terminal input, footer
  placement, and terminal-control sanitization.
- Formatting, all-target checks, Clippy with warnings denied, architecture
  boundaries, optimized release build, and documentation checks passed.

## Upgrade

Run Allp as your normal user; it elevates only root-required child commands:

```console
allp upgrade
```

See `CHANGELOG.md` section `[0.6.3]` for the complete change record.

## Local Release Output

- Source archive: `dist/allp-v0.6.3-source.tar.gz`
- SHA-256 file: `dist/allp-v0.6.3-source.tar.gz.sha256`
- Finalized notes: `dist/RELEASE_NOTES_v0.6.3.md`

The archive is generated from the exact annotated tag `v0.6.3` after the release commit.

## Checksum

SHA256: _generated during finalization_
