# Platforms and Installation

[← Wiki home](Home.md) · [فارسی](Platforms-and-Installation.fa.md)

Linux package orchestration is the primary product surface. macOS Homebrew is
experimental. Windows supports compilation, diagnostics, release target
selection and verified deferred self-replacement, but does not advertise Linux
Snap/Flatpak backends.

Allp recognizes Debian, Red Hat/Fedora, Arch, SUSE and Alpine families, plus
Bazzite as an image-based Fedora-family host. It records architecture, libc,
WSL/container state and platform data directories.

## Install options

| Method | Destination | Best for |
|---|---|---|
| Verified release installer | `~/.local/bin/allp` by default | Most users |
| `make install-user` | `~/.local/bin/allp` | Local source builds without sudo |
| `make install` | `/usr/local/bin/allp` | System-wide source build |
| `cargo build --release` | `target/release/allp` | Development/testing |

```bash
curl --fail --location --output install-allp.sh \
  https://github.com/allp-manager/allp-manager/releases/latest/download/install-allp.sh
less install-allp.sh
sh install-allp.sh
```

The installer checks platform/architecture, exact archive and adjacent SHA-256,
and rejects unexpected archive members. It never recommends `curl | sh`.

## Verify path and identity

```bash
command -v allp
allp --version --verbose
make install-check   # in a source checkout
```

If the wrong copy resolves, update `PATH` and run `hash -r` (bash) or `rehash`
(zsh). A native dpkg/rpm/Pacman-owned binary remains managed by that source.
