# Profile inventory and migration

```bash
# On the source host; all three commands are inventory/file operations.
allp profile save workstation
allp profile verify workstation
allp profile export workstation workstation.toml

# Offline conversion; source profile and package managers are unchanged.
allp profile convert workstation --to arch --report workstation-arch.json
allp profile export workstation-arch workstation-arch.toml

# Move both TOML files and the report to the destination. Import the original
# there and repeat conversion with repository checks and a new output name.
allp profile import workstation.toml
allp profile convert workstation --to arch --name verified-arch --verify
allp profile install verified-arch --dry-run
allp profile install verified-arch
```

Targets are Debian, Ubuntu, Arch and Fedora. `--name` selects a new output
profile. Existing output profiles and the original are never overwritten by
conversion. The default JSON report is beside the output TOML with extension
`.conversion.json`; copy it with the profile when moving machines.

`profile verify` compares a fresh inventory, including observed versions,
installer, Flatpak scope/remote/branch, with the saved inventory. Changes are
reported and exit with code 8. A manager that fails detection or listing causes
capture to fail; a partial snapshot is not written. An absent manager is not an
error. Inventory can only cover managers supported and discoverable by Allp.
Manual binaries, AppImages, other users' tool installations and application
data are outside this inventory. Flatpak apps are captured; runtimes are
resolved by Flatpak when apps are installed.

## Mapping and reports

Each source entry appears in the report, which also embeds the complete source
profile. Statuses distinguish `mapped`, `preserved`, `verified`, `unmapped`,
`unavailable`, and `check_failed`. Offline mapping does not establish that a
package exists in a destination repository. Only a successful destination
lookup without an exact ID produces `unavailable`; parser, network, missing
manager and repository errors produce `check_failed`, never an absence claim.
When an optional portable manager is absent, its IDs are retained as
`preserved` so prerequisite setup can still run before app installation.
`availability_checked` records that checking was requested; only
`availability_verified: true` means every entry was verified successfully.

Offline conversion uses a deliberately small explicit mapping catalog for
common applications/tools, including Python, pip, fd and pkgconf names. It
does not guess replacements for libraries, drivers, kernels, distribution
metapackages or unknown names. `--verify` can check unchanged IDs against the
destination's configured sources, but an identical name alone is not proof of
software equivalence; review the result. Native command changes are recorded
per package. Review text is never executed.

Unknown or unavailable entries are excluded from the generated install list
and retained in the source and report. The output profile carries conversion
counts and warns about omitted entries when shown or installed. A report can
be incomplete even though the file conversion itself succeeded.

For names you have verified yourself, supply `--map mappings.toml`:

```toml
[[mappings]]
source_backend = "apt"
source_package = "my-source-package"
target_package = "my-destination-package"
```

Mappings apply to the `--to` target of that invocation. Duplicate source pairs,
unknown fields, leading options and malformed package IDs are rejected.

Reverse conversion is another explicit `--to` operation, for example
`allp profile convert workstation-arch --to debian`. Many-to-one package
relationships are not lossless; keep the original inventory for exact recovery.
Versions are observed metadata, not pins, and native versions are cleared when
switching managers. Snap channels are not captured; the current store metadata
and confinement are resolved during restore. The feature converts inventories,
not operating systems, repositories, configuration, services, drivers or data.

## Snap and Flatpak

Their app IDs and owning managers are preserved across distributions. Flatpak
scope, remote and branch remain attached to each app. Before installing apps,
Allp checks required managers, explains the optional manager setup, and prompts
before installing missing Snap/Flatpak prerequisites through a known system
provider. Noninteractive setup requires `--yes --allow-bootstrap`; `--yes`
alone does not authorize setup. Choose `--without snap`, `--without flatpak`
or `--without snap,flatpak` to skip those apps without changing the saved file.

A prerequisite dry run prints setup plans, executes nothing, and asks you to
rerun after setup so app availability can be checked. An unconfigured user
Flathub remote can be added after separate confirmation. Saved custom remotes
or system-scoped sources that need setup require manual configuration; Allp
does not replace their identity with user Flathub. Snap daemon activation may
need manual configuration after installing snapd. On Arch, snapd comes from
AUR; automatic profile bootstrap is not provided. With a helper installed,
review `allp install snapd --from aur --dry-run`, then follow
[Snap's Arch setup guide](https://snapcraft.io/docs/tutorials/install-the-daemon/arch-linux/).
