# Package Profiles

[← Wiki home](Home.md) · [فارسی](Package-Profiles.fa.md)

Profiles are experimental, versioned TOML inventories that preserve both the
package ID and owning backend.

```bash
allp profile save workstation
allp profile verify workstation
allp profile list
allp profile show workstation
allp profile export workstation workstation.toml
allp profile import workstation.toml --name laptop
allp profile install laptop --dry-run
allp profile install laptop
```

```toml
version = 1
name = "developer-tools"

[[packages]]
backend = "apt"
package = "git"

[[packages]]
backend = "rust"
package = "ripgrep"
version = "14.1.1"
```

Version is observed inventory metadata, not a pin. Explicit conversion uses
`allp profile convert workstation --to arch` (also Debian/Ubuntu and Fedora).
It preserves the original and produces a new profile and JSON report for every
source package. `--verify` checks exact availability on the destination;
`--map FILE` supplies reviewed mappings. Unknown packages remain in the report.
See [the migration guide](../docs/PROFILE_MIGRATION.md).
Before applying, every backend is verified so a
missing backend cannot stop the run after earlier packages already changed the
host. Installation then uses the normal per-package search, planning and
confirmation path.

Missing Snap/Flatpak prerequisites are offered before application installation.
Use `--yes --allow-bootstrap` for explicit noninteractive setup, or
`profile install NAME --without snap,flatpak` to skip those optional apps.

System inventory can include dependencies. Review the exported file before
moving it to another machine. Imports reject unknown fields, duplicate entries,
unsafe names, unsupported profile versions and excessive sizes. Writes are
atomic and use a rollback file when replacing an existing profile.

On Linux, profiles normally live under `~/.config/allp/profiles/`; confirm the
effective platform-specific path with `allp doctor`.
