# Allp v0.6.6

Allp can now verify saved package inventories, convert them between Linux
package-manager families with a per-package report, and coordinate AUR upgrades
through an installed yay or paru. Maintenance output gives clearer update-check
failures and native diagnostics. Stable installation is also available through
the project's signed APT feed after one-time repository setup.

## Package profiles

- Compare saved inventories with the host using `allp profile verify NAME`.
- Convert profiles with `allp profile convert NAME --to debian|ubuntu|arch|fedora`.
  Reports retain every source entry, explain command/name mappings, and distinguish
  unavailable packages from failed destination checks. The original is preserved.
- Retain Snap/Flatpak ownership and Flatpak installation, branch and remote.
  Missing Snap/Flatpak prerequisites are offered before application installation;
  unattended setup requires `--yes --allow-bootstrap`.

## AUR on Arch

- Search, inspect, install and upgrade AUR packages through yay or paru.
- Default upgrades run Pacman first, then one helper with AUR-only `-Sua`.
  A failed selected Pacman upgrade defers AUR, including with stale-metadata override.
- Preserve helper review/editor prompts and original-user execution. Foreign
  package inventories are retained even when no AUR helper is installed.

## Maintenance diagnostics

- Show failed checks before execution confirmation and compact human summaries.
- Keep independent global npm results if a project check fails and use a bounded
  45-second deadline for registry queries.
- Report Snap Store connectivity failures with native evidence and diagnostic
  commands. Show read-only Snap task/download details in live maintenance progress.

## Distribution

- Signed Stable APT feed: https://allp-manager.github.io/apt/ (amd64 and arm64).
  Follow its one-time setup, then use `sudo apt install allp` and normal APT upgrades.
- Native DEB/RPM builders, an artifact-only package workflow, and a checksum-based
  Homebrew formula generator are included. Source is published under the MIT license.
- Ubuntu archive packaging request: https://bugs.launchpad.net/ubuntu/+bug/2174024
  This request is pending review and does not mean official archive acceptance.

## Validation and limitations

The local `make release-prepare VERSION=0.6.6` quality gate passed: formatting,
compilation, Clippy with warnings denied, 391 Rust tests, architecture checks,
release build, documentation checks and installer regression tests.

AUR support remains experimental; helper build/review behavior needs validation
on each intended Arch environment. Conversion does not migrate an operating
system, application data, services, drivers or configuration, and unknown packages
remain in the report for review. Saved versions are observations, not pins;
Snap channels are not captured. The APT feed requires explicit setup and is
separate from the Debian/Ubuntu archives. The initial v0.6.5 GNU/Linux packages
require glibc 2.39; APT checks each published package's actual native dependencies.

## Source archive

The source archive is generated from the exact annotated tag `v0.6.6`.
Every published binary/source archive includes a SHA-256 sidecar and is included
in the verified release manifest.

SHA256: _generated during finalization_
