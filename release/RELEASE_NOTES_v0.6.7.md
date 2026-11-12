# Allp v0.6.7

Snap lookup timeouts can now recover through the native Snap CLI, and refresh
failures retain their actual Store error instead of an unexplained exit-status
summary. Install/remove failures are classified accurately. The native Snap
canary now tests installation, execution, upgrade and removal on a disposable
Ubuntu runner.

## Fixes

- Retry transient socket failures only for read-only discovery and exact
  metadata queries. Native Store queries have a bounded 45-second deadline.
- Keep authoritative missing-package responses and exact channel/confinement
  validation. A lost mutation response never authorizes replaying an install.
- Recognize refresh HTTP 408/429/5xx and Store response timeouts, preserve the
  native endpoint/error, and show appropriate read-only diagnostic checks.
- Retain both stdout and stderr from failed Snap operations. Only actual
  package-management locks produce Busy; other install/remove errors are Failed.

## Validation

The local full quality gate passed with 396 Rust tests, formatting, Clippy,
architecture checks, release compilation, documentation and installer checks.
Snap regression checks ran with temporary Unix sockets and interactive PTYs.
A live install preview for Canonical's hello-world succeeded on the affected host.
The native Snap canary now exercises the real application lifecycle before
Stable release promotion. It uses an explicit Ubuntu 24.04 runner, waits for
snapd seeding, checks native Store access, and saves native daemon diagnostics
when a check fails.

## Network and installation notes

A Snap Store refresh endpoint can time out even when general connectivity and
snapd's local socket are healthy. Allp reports the real failure and does not
change snapd proxy settings, remove locks, force-close applications, or label
an unsuccessful operation as updated.

When upgrading an existing Allp installation, check `command -v allp` and
`allp --version --verbose`. A user-local binary and a separate system binary
must be reconciled deliberately; publishing a GitHub release does not replace
those installed copies automatically.

The signed project APT feed is available at https://allp-manager.github.io/apt/.
It requires one-time source/key setup and is separate from official Debian/Ubuntu
archives. Official archive packaging request: https://bugs.launchpad.net/ubuntu/+bug/2174024.

## Source archive

Source and binaries are built from the exact annotated tag `v0.6.7`, with SHA-256
sidecars and the verified release manifest.

SHA256: _generated during finalization_
