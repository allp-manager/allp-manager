# Getting Started

[← Wiki home](Home.md) · [فارسی](Getting-Started.fa.md)

## Requirements

- Linux for mature package orchestration; macOS/Homebrew is experimental.
- A supported native package manager already installed.
- Rust 1.74+ only when building from source.
- `sudo` only when a selected native child genuinely requires root.

## Install a verified release

```bash
curl --fail --location --output install-allp.sh \
  https://github.com/allp-manager/allp-manager/releases/latest/download/install-allp.sh
less install-allp.sh
sh install-allp.sh
```

The installer verifies the adjacent SHA-256 file and archive contents, then
installs to `~/.local/bin`. To select a version or destination:

```bash
ALLP_INSTALL_DIR="$HOME/bin" sh install-allp.sh 0.6.2
```

Ensure the destination is on `PATH`, then verify the build identity:

```bash
allp --version
allp --version --verbose
```

## Build from source

```bash
git clone https://github.com/allp-manager/allp-manager.git
cd allp-manager
cargo build --release
./target/release/allp --version --verbose
```

Install for only your user with `make install-user`, or to `/usr/local/bin`
with `make install`.

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
