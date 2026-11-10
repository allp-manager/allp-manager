# Getting Started

[← Wiki home](Home.md) · [فارسی](Getting-Started.fa.md)

## Requirements

- Linux for mature package orchestration; macOS/Homebrew is experimental.
- A supported native package manager already installed.
- Rust 1.74+ only when building from source.
- `sudo` only when a selected native child genuinely requires root.

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

## First safe session

```bash
# 1. See platform, paths and usable backends
allp doctor
allp detect --verbose

# 2. Search without changing the host
allp search git

# 3. Inspect the exact plan
allp install git --from apt --dry-run

# 4. Run after reviewing the source, argv and privilege label
allp install git --from apt
```

Use `--scope apps`, `--scope dev`, or `--scope all` to avoid an interactive
scope question. Use `--from <backend>` whenever source identity matters.

## Update versus upgrade

`update` refreshes backend metadata. `upgrade` upgrades installed software.
The exact native meaning is backend-defined and always shown first.

```bash
allp update --dry-run
allp upgrade --dry-run
allp update
allp upgrade
```

Never delete native lock files after a busy error; find or wait for the owning
package-manager process instead.
