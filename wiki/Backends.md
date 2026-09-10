# Backend Guide

[← Wiki home](Home.md) · [فارسی](Backends.fa.md)

Allp discovers backends on every invocation and advertises only capabilities
that are actually available. “Experimental” means implemented and fake-PATH
tested, but still needing broader real-host validation.

| Family | Backends | Status | Notes |
|---|---|---|---|
| System | APT, Pacman, DNF/DNF5 | Stable alpha | Native distro packages |
| Image-based | rpm-ostree on Bazzite/Fedora Atomic | Experimental | Transactional deployment and layering |
| Other Linux | Zypper, APK, XBPS, Portage, eopkg, swupd | Experimental | Capabilities vary by tool |
| Universal apps | Flatpak, Snap | Stable alpha | Remote/socket readiness is validated |
| Cross-platform | Homebrew/Linuxbrew | Experimental | Owner and prefix are validated |
| Development | PyPI + pip/pipx/uv | Experimental | Environment/user/tool scopes |
| Development | npm + npm/pnpm/Yarn | Experimental | Project/workspace/global scopes |
| Development | crates.io + Cargo | Experimental | Binary crates; no project dependency edits |

## System backends

APT refreshes metadata with `apt-get update` and upgrades with its native
upgrade plan. Pacman intentionally has no standalone update capability because
partial upgrades are unsafe; upgrade uses a full sync-and-upgrade plan. DNF
supports DNF4 and DNF5 output shapes.

On Bazzite, DNF host mutation is disabled even if a helper exists. `rpm-ostree`
handles metadata refresh, image upgrades and explicit package layering. A
layered change normally requires reboot and is presented as a last resort after
Flatpak, Homebrew or containers.

## Flatpak

Allp distinguishes “missing executable”, “installed with no remotes”, “ready”,
and “probe failed”. Flathub addition is a separate, user-scoped bootstrap plan.

```bash
allp doctor flatpak
allp search firefox --from flatpak
allp install org.mozilla.firefox --from flatpak --dry-run
```

## Snap

Snap prefers the local snapd REST socket for wide discovery, exact resolution,
installation and terminal change monitoring. Before planning it validates the
canonical name, publisher, confinement, architecture, channel and installed
state. An authoritative `snap-not-found` response never falls back to stale CLI
metadata. CLI fallback is restricted to transport or compatibility failures.

```bash
allp doctor snap
allp install pycharm --from snap --dry-run
```

Classic confinement adds `--classic` only when metadata requires it.

## Homebrew

One validated locator is shared by detect, doctor and operations. It checks
configured paths, revalidated state, original-user paths and official prefixes.
Under `sudo allp`, Homebrew still runs as its validated owner.

```bash
allp doctor homebrew --verbose --no-color
sudo allp update --from homebrew --dry-run --skip-self-update
```

## Python, Node and Rust

Registry names are not assumed to be official. Fuzzy registry matches cannot be
installed automatically. User/project scopes are preserved and are never
silently elevated.

```bash
allp install black --from pipx --dry-run
allp install typescript --from pnpm --dry-run
allp install ripgrep --from cargo --dry-run
```

Cargo host maintenance never calls `cargo add` or project `cargo update`.
Global binary upgrades require the optional community `cargo-update` command.

The exact operation matrix is maintained in
[CAPABILITY_MATRIX.md](https://github.com/allp-manager/allp-manager/blob/main/docs/CAPABILITY_MATRIX.md).
