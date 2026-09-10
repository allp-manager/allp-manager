# Maintenance and Self-Update

[← Wiki home](Home.md) · [فارسی](Maintenance-and-Self-Update.fa.md)

Allp plans every selected backend before executing a maintenance batch. APT
metadata refresh is a dependency of APT upgrade; failure defers that upgrade
unless the user explicitly accepts stale metadata.

```bash
allp update --dry-run
allp upgrade --dry-run
allp upgrade --allow-stale-metadata  # recovery only
```

Backends define semantics: APT/DNF refresh metadata, Pacman combines sync and
upgrade, Flatpak/Snap refresh installed apps, Homebrew separates metadata and
package upgrade, and rpm-ostree stages transactional changes.

## Self-update controls

```bash
allp self-update --check-only
allp self-update --update-channel stable
allp self-update --update-channel continuous
allp self-update --update-channel prerelease
allp update --skip-self-update
allp update --self-only
allp update --offline
```

Fresh installs default to stable. Explicit channel choices persist. Offline
contacts neither GitHub nor package sources. A newer local build is never
downgraded and is reported as `LocalAhead`.

Official assets are selected by OS, architecture, libc and target from a signed
identity chain of manifest metadata and SHA-256 evidence. Replacement keeps a
rollback backup until the new binary passes verification. Successful `allp
update` replacement re-executes once and continues backend maintenance without
looping.
