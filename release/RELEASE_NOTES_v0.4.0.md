# Allp v0.4.0

Allp v0.4.0 adds a safe, inline live dashboard to real interactive maintenance
runs while hardening self-update and Homebrew execution behavior.

## Highlights

- `allp update` and `allp upgrade` now keep native logs in normal terminal
  scrollback, render colored state/error cards, and maintain a progress footer
  with the active backend, exact action, elapsed time, and queue progress.
- `--no-tui` restores the classic stream. JSON, dry runs, redirects,
  non-interactive runs, and `TERM=dumb` retain their existing safe behavior.
- The dashboard is only a presentation observer: native argv, confirmation,
  privilege boundaries, inherited stdin, status classification, and exit codes
  stay in the central process runner.
- Homebrew remains owner-scoped under sudo, now has macOS Directory Services
  user resolution, and no longer repeats its update-report environment flag in
  command previews.
- Local `make reinstall` builds can take a newer verified continuous build even
  when local and CI revision markers collide.
- System reinstall now warns when a PATH-shadowing user-local binary remains
  selected, rather than implying that both installation locations were updated.

See `CHANGELOG.md` section `[0.4.0]` for the complete change record.

## Validation

- `make quality` — 161 library tests, 102 CLI fake-path/PTY tests, architecture
  checks, release build, and documentation checks.
- `cargo check --all-targets --target x86_64-apple-darwin`
- `cargo check --all-targets --target aarch64-apple-darwin`
- `cargo check --all-targets --target x86_64-pc-windows-msvc`
- Live Linuxbrew doctor and read-only Homebrew update-plan validation.

## Host follow-up

- Run the documented Homebrew smoke checks on a physical macOS host before
  relying on the original-user sudo path there.
- Exercise native package-manager prompts, Ctrl+C, and terminal resizing on
  the supported terminal environments before calling the dashboard behavior
  fully host-validated.

## Recent Commits

a0836c6 self update

## Local Release Output

- Source archive: `dist/allp-v0.4.0-source.tar.gz`
- SHA-256 file: `dist/allp-v0.4.0-source.tar.gz.sha256`
- Finalized notes: `dist/RELEASE_NOTES_v0.4.0.md`

The archive is generated from the exact annotated tag `v0.4.0` after the release commit.

## Checksum

SHA256: _generated during finalization_
