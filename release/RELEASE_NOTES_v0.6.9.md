# Allp v0.6.9

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

## Validation

- Full local quality gate: 406 Rust tests and 22 distribution-policy tests,
  formatting, checks, Clippy, architecture, release build, docs and installer checks.
- Ten PTY tests executed with real Unix sockets and terminals, including live
  package details, startup/polling failure recovery and sudo/native prompts.
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

Source and binary archives are built from annotated tag `v0.6.9`, with checksum
sidecars and the release manifest.

SHA256: _generated during finalization_
