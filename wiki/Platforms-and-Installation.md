# Platforms and Installation

[← Wiki home](Home.md) · [فارسی](Platforms-and-Installation.fa.md)

Linux package orchestration is the primary product surface. macOS Homebrew is
experimental. Windows supports compilation, diagnostics, release target
selection and verified deferred self-replacement, but does not advertise Linux
Snap/Flatpak backends.

Allp recognizes Debian, Red Hat/Fedora, Arch, SUSE and Alpine families, plus
Bazzite as an image-based Fedora-family host. It records architecture, libc,
WSL/container state and platform data directories.

**⚠️ Install Rust and Cargo before running make.** Git, Make and a C compiler/linker are also required. Follow the [official Rust installation guide](https://www.rust-lang.org/tools/install), reopen your terminal, then check:

```bash
rustc --version
cargo --version
```

## Install from source

```bash
git clone https://github.com/allp-manager/allp-manager.git
cd allp-manager
make install
```

`make install` builds ALLP Manager and uses `sudo` to copy it to `/usr/local/bin/allp`.

## User installation without sudo

Choose this instead of `make install` to install only for your user, from the same repository directory:

```bash
make install-user
export PATH="$HOME/.local/bin:$PATH"
allp --version
```

The destination is `~/.local/bin/allp`. The PATH command above applies to the current terminal; add that line to your shell configuration for future sessions.

## Termux on Android

Termux is not currently a supported ALLP Manager environment. `make install` uses `sudo` and installs to `/usr/local/bin`; this does not fit a standard Termux installation. `make install-user` avoids sudo during installation, but does not make package operations compatible with Termux: ALLP Manager’s APT mutations still require root. Native Termux support needs separate runtime, path and privilege handling. Until that support is implemented and tested, manage Termux packages with its native `pkg` command.

## Verify path and identity

```bash
command -v allp
allp --version --verbose
make install-check   # in a source checkout
```

If the wrong copy resolves, update `PATH` and run `hash -r` (bash) or `rehash`
(zsh). A native dpkg/rpm/Pacman-owned binary remains managed by that source.
