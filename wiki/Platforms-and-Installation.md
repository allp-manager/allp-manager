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

Run Allp as the Termux app user on Android. Use an Android release archive for ARM64 or x86-64, or build in Termux with `pkg install rust clang make git` followed by `make install-termux` in the source checkout. The release installer runs with `sh install-allp.sh` and installs to `$PREFIX/bin` by default.

The `termux` backend (`--from pkg` is also accepted) searches and lists packages, shows package details, and plans installation, removal, and upgrades in the Termux prefix. It uses the APT or Pacman tools configured for that Termux installation. Termux rejects package management from an effective UID of 0, including root shells on rooted devices. Leave the root shell before using Allp there. Preview an upgrade with `allp upgrade --from termux --dry-run`.

## Verify path and identity

```bash
command -v allp
allp --version --verbose
make install-check   # in a source checkout
```

If the wrong copy resolves, update `PATH` and run `hash -r` (bash) or `rehash`
(zsh). A native dpkg/rpm/Pacman-owned binary remains managed by that source.
