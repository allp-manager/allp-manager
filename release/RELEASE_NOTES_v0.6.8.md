# Allp v0.6.8

Snap lookups recover from transient socket timeouts through the native CLI.
Refresh failures preserve the actual Snap Store error and endpoint. Installation
and removal failures are classified accurately. Read-only recovery never
replays a mutation with an uncertain result.

## Automatic distribution

- A successful Stable GitHub release triggers a native Arch build, package tests,
  installation and an upgrade-plan check before pushing allp-manager to AUR.
- A six-hour reconciliation retries temporary publication outages and skips
  versions already published or superseded. Install with `yay -S allp-manager`
  or `paru -S allp-manager`; the installed command is `allp`.
- Backend canaries run automatically on main and prerelease pushes. Tagged
  releases wait for pending exact-source CI, WinGet and Termux checks.
- Proven Snap Store failures before any mutation are reported explicitly in
  validation evidence and release notes. Application failures, started mutations
  and failures in another backend still block publication.
- GitHub CLI log reading supports both older clients and newer clients that
  require explicit permission to pipe ANSI output into a non-terminal parser.

## Validation

The patched application completed real Snap installation, native execution,
fixture-only refresh and Allp removal of Canonical hello-world on the affected
host. Both standalone host installations were reconciled with backups.
The release passes 396 Rust tests and 22 distribution-policy/CLI compatibility
checks, plus formatting, Clippy, architecture, documentation and installer checks.
The AUR pipeline also passed a native Arch build/install preview using v0.6.6;
the published v0.6.8 package is independently built and tested before its push.

The public v0.6.7 source tag is retained unchanged. Its release stopped before
asset publication because the runner's newer GitHub CLI rejected colored logs.
This release includes that candidate's application fixes and corrects the
publication compatibility issue.

## Distribution notes

Snap Store outages can still prevent native operations even with a healthy local
daemon. Allp preserves the real failure and does not alter proxy settings, delete
locks or force-close applications. A passing general connectivity check does
not verify every Store endpoint.

The signed project APT feed is https://allp-manager.github.io/apt/. It requires
one-time source/key setup and is separate from official Debian/Ubuntu archives.
Official packaging request: https://bugs.launchpad.net/ubuntu/+bug/2174024.

Source and binaries come from the exact annotated tag `v0.6.8`, with SHA-256
sidecars and the verified release manifest.

SHA256: _generated during finalization_
