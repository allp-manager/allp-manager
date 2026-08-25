# Bazzite / rpm-ostree Backend

Bazzite is an image-based Fedora-family host. Allp uses `rpm-ostree` for host
package layering and transactional system-image maintenance instead of treating
DNF like a mutable workstation package manager.

## Detection And Routing

The backend is ready only after `rpm-ostree status --json` succeeds and returns
valid deployment metadata. It can be selected with `rpm-ostree`, `rpmostree`,
`bazzite`, `fedora-atomic`, or `atomic`.

When `/etc/os-release` identifies Bazzite, the ordinary DNF backend is reported
as unavailable for host mutations with an explanation directing the user to
rpm-ostree. This prevents an executable `dnf` helper on Bazzite from being
mistaken for the host installation path.

## Native Commands

| Allp operation | Native rpm-ostree command | Meaning |
|---|---|---|
| Search | `rpm-ostree search <query>` | Search enabled rpm-md repositories |
| Install | `rpm-ostree install -- <package>` | Stage a layered host package |
| Remove | `rpm-ostree uninstall -- <package>` | Stage removal of a requested layer |
| List | `rpm-ostree status --json` | List requested packages on the booted deployment |
| Info | `rpm-ostree search <package>` | Curate repository search metadata |
| Update | `rpm-ostree refresh-md` | Refresh rpm-md metadata |
| Upgrade | `rpm-ostree upgrade` | Stage the next system image/deployment |

Mutating plans are root-required and transactional. Layering changes and system
upgrades normally become active after reboot; Allp states this in the plan and
does not reboot automatically.

## Layering Policy

Bazzite recommends package layering only as a last resort because layered
packages can block future upgrades or rebases. Every Allp layering plan therefore
recommends, in order of fit, Homebrew for CLI tools, Flatpak for applications, or
a development container before changing the host image.

`ujust update` intentionally is not used by this backend: it updates the system,
Flatpaks, and containers together, which would cross backend boundaries and can
duplicate work already orchestrated by Allp.

References: [Bazzite package-layering documentation](https://docs.bazzite.gg/Installing_and_Managing_Software/rpm-ostree/)
and the [rpm-ostree administrator handbook](https://coreos.github.io/rpm-ostree/administrator-handbook/).
