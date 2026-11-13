# Allp v0.6.11

Long downloads no longer leave remaining system operations blocked on sudo-rs
hosts solely because its credential-expiry diagnostic differs from classic sudo.
After a completed operation, interactive Allp runs safely hand the terminal back
to native sudo for renewed authentication, then resume the pending queue without
replaying completed upgrades. Noninteractive runs remain blocked without prompting.

Snap refresh progress shows the active package, native phase and real download
percentage, for example `Snap · Downloading code 91%`. Temporary snapd read
failures show a retry status and monitoring resumes without replaying refresh.
When the initial snapshot is unavailable, observations are explicitly labeled
as system activity rather than attributed to Allp's own change.

Homebrew metadata download errors now override a successful exit status from
its auto-update wrapper. Allp reports the failure, keeps the cache unverified,
and stops dependent outdated checks and upgrades instead of claiming packages
are current. Snap DNS failures include system resolver/VPN and partial-change
inspection guidance while preserving native errors.

## Release evidence

Native Snap preflight timeouts count as a proven external outage only when the
native search exits 124, Allp discovery was skipped, and an independent HTTPS
probe to the exact Snap Store host also times out with HTTP 000. Unknown failures,
application/mutation failures and failures in another backend still block release.
The failed canary remains visible and its evidence is recorded in release notes.
The first-install fixture deadline permits native snapd/base bootstrap on fresh
runners; every installation/execution/refresh/removal assertion remains in place.

The v0.6.9 and v0.6.10 source tags are retained unchanged. Their workflows stopped
before binary publication after external Snap service failures/cold-start delays.
This release includes their application repairs plus the sudo-rs fix.

## Validation

- Full local quality gate: 409 Rust tests and 27 distribution-policy tests,
  formatting, checks, Clippy, architecture, release build, docs and installer checks.
- PTY tests cover live package details, startup/polling failure recovery,
  classic/TUI sudo-rs expiry after APT, noninteractive blocking and native prompts.
- Regression tests reproduced the old failures before the repairs. The local
  development repair was installed into both affected standalone copies with
  verified backups and matching checksums.
- Source CI and native canary results are checked for this exact release commit.

## Distribution

- AUR: `yay -S allp-manager` or `paru -S allp-manager`. The Stable release triggers
  a native Arch build, install test and upgrade-plan check before publication.
- Signed project APT feed: https://allp-manager.github.io/apt/ (one-time setup).
- Ubuntu packaging request: https://bugs.launchpad.net/ubuntu/+bug/2174024.
- Debian RFP: https://bugs.debian.org/1153782.

The independent APT feed is separate from official Debian/Ubuntu archives;
those requests are still awaiting packaging review and maintainership.

## Limitations

Working system DNS/network access is still required for native downloads.
Progress monitoring does not change proxy/VPN settings, stop applications or
restart native refreshes. A passing general connectivity check cannot verify
every Store endpoint. The updated display applies to subsequent executions.

Source and binary archives are built from annotated tag `v0.6.11`, with checksum
sidecars and the release manifest.

SHA256: _generated during finalization_
