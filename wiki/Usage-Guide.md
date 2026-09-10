# Practical Usage Guide

[← Wiki home](Home.md) · [فارسی](Usage-Guide.fa.md)

This guide follows the way a person actually uses Allp: inspect the machine,
search, choose a trustworthy source, review a plan, mutate, and verify.

## 1. Know your environment

```bash
allp --version --verbose
allp doctor
allp detect --verbose
```

`doctor` explains the OS, distro family, architecture/libc, user and sudo
context, Allp installation ownership, data directories, release target and
backend readiness. `detect` focuses on every built-in backend and capability.
Fix `FoundButUnavailable` or `FoundButUnconfigured` before use.

## 2. Search in the right scope

```bash
allp search firefox --scope apps
allp search black --scope dev
allp search git --scope all
allp search pycharm --from snap
```

- `apps`: system packages, universal apps and Homebrew.
- `dev`: Python, Node and Rust/Cargo ecosystems.
- `all`: every eligible source.
- `--from`: one exact backend or installer such as `apt`, `flatpak`, `snap`,
  `homebrew`, `pipx`, `pnpm`, or `cargo`.

Matches are Exact, Related or Fuzzy. Use `--exact` for strict matching or
`--all` for weak fuzzy results. Same names do not prove package equivalence.

## 3. Inspect before installation

```bash
allp info firefox
allp info firefox --from flatpak --full
allp info git --from apt --raw
allp install git --from apt --dry-run
```

Default info is curated, `--full` expands normalized metadata, and `--raw`
shows native output. Dry run performs discovery, resolution and plan creation
while executing no mutation and no sudo.

## 4. Install or remove

```bash
allp install git --from apt
allp install org.mozilla.firefox --from flatpak
allp install black --from pipx
allp install typescript --from pnpm
allp install ripgrep --from cargo

allp remove git --from apt --dry-run
allp remove git --from apt
```

Read the package ID, source, scope, exact native argv and privilege label. If a
source is ambiguous, choose it explicitly; `--yes` does not choose for you.
Prerequisite installation or remote addition is a separate plan.

## 5. Inspect installed software

```bash
allp list
allp list --from apt --filter git
allp list --from flatpak --limit 50 --no-pager
allp list --json
```

Filtering happens before limiting. Long interactive output uses a directly
spawned pager, not a hidden shell pipeline.

## 6. Maintain the machine

```bash
allp update --dry-run
allp upgrade --dry-run
allp update
allp upgrade
```

Update refreshes metadata; upgrade changes installed software. Plans are built
for the whole batch before confirmation, then run sequentially and continue
after individual failures. Exit code 8 means partial failure.

```bash
allp update --from apt --skip-self-update
allp upgrade --from flatpak
allp update --scope dev --target tools --dry-run
allp upgrade --scope dev --target global --dry-run
allp update --no-tui
```

## 7. Reproduce a toolset

```bash
allp profile save workstation
allp profile export workstation workstation.toml
# review the TOML on the destination
allp profile import workstation.toml --name new-machine
allp profile install new-machine --dry-run
allp profile install new-machine
```

Profiles preserve backend identity. Versions are inventory observations, not
pins, and system inventories may include dependencies.

## 8. Use in CI or scripts

```bash
allp search git --from apt --json
allp update --dry-run --json
allp install git --from apt --no-interactive --yes
```

Require the expected JSON `schema_version`, inspect `complete` and `issues`, and
handle documented exit codes. Real mutation should follow a reviewed dry run.
Unattended prerequisite bootstrap additionally needs `--allow-bootstrap`.

## Recommended habits

1. Run Allp as your normal user, not with blanket `sudo allp`.
2. Use `--from` for reproducibility and security-sensitive installs.
3. Dry-run unfamiliar backends and maintenance batches.
4. Keep native package-manager locks intact.
5. Verify registry ownership; do not trust a familiar package name alone.
