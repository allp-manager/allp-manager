# Linux Coverage

Allp supports package-manager families rather than distribution names.

Stable alpha coverage:

- APT for Debian, Ubuntu, Linux Mint, Pop!_OS, elementary OS, and compatible derivatives.
- Pacman for Arch-family systems. `update` synchronizes package databases with `pacman -Sy`; `upgrade` uses full sync-and-upgrade semantics.
- DNF/DNF5 for Fedora/RHEL-family systems.
- Flatpak.
- Snap, including a snapd usability probe.

Experimental coverage:

- Zypper for openSUSE/SUSE-family systems.
- APK for Alpine.
- XBPS for Void Linux.
- Portage/emerge for Gentoo-family systems. Remove/list are not advertised.
- eopkg for Solus.
- swupd for Clear Linux bundles.
- rpm-ostree for Bazzite and Fedora Atomic-style systems. Bazzite disables DNF
  host mutations, stages package layering transactionally, and uses
  `rpm-ostree upgrade` for the host image.

Unsupported in this phase:

- Nix/NixOS.
- transactional-update systems.
- Guix.
- Slackware tools.

Unsupported rows are documented honestly; they are not silently treated as stable package-manager support.
