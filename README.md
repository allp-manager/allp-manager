# Allp

[فارسی](README.fa.md)

> One CLI. Your native package managers. No hidden magic.

Allp is not another package manager.

Allp is a transparent command-line orchestrator for package managers that are already installed on the current machine. It discovers supported backends on every invocation, searches across system, universal, Homebrew, Python, and Node ecosystems, shows the exact native command before mutation, and executes the native package manager directly.

## Alpha Status

Allp is currently a v0.3.3 public-alpha candidate.

Release title: **Allp v0.3.3 — Official Software Resolution**.

| Category | Backend | Alpha status |
|---|---|---|
| System | APT | Stable alpha |
| System | Pacman | Stable alpha |
| System | DNF / DNF5 | Stable alpha |
| System | Zypper, APK, XBPS, Portage, eopkg, swupd | Experimental |
| Universal | Flatpak | Stable alpha |
| Universal | Snap | Stable alpha |
| Homebrew | macOS Homebrew / Linuxbrew | Experimental |
| Python | PyPI with pip, pipx, uv installers | Experimental |
| Node.js | npm registry with npm, pnpm, Yarn installers | Experimental |

Future ecosystems such as Cargo, Composer, Go, RubyGems, Maven/Gradle, and GUI/TUI modes remain out of scope for this phase.

## Demo

```bash
allp detect
allp search git
allp install git --dry-run
allp update --dry-run
```

## Command Shape

The canonical CLI shape is command first:

```text
allp <command> [arguments] [options]
```

Examples:

```bash
allp search git --limit 10
allp search git --scope apps
allp search openai --scope dev
allp search git --exact
allp search git --all
allp install git --from apt --dry-run
allp install git --scope all --dry-run
allp remove git --from apt --dry-run
allp update --from apt --dry-run
allp update --scope dev --target all --dry-run
allp update --from npm --target project --dry-run
allp update --from pipx --target tools --dry-run
allp list --from apt --json
allp list --from apt --filter git --limit 50 --no-pager
allp info git --from apt --json
allp info git --from apt --full
allp info git --from apt --raw
allp search openai --from python
allp install black --from pipx --dry-run
allp search typescript --from node
allp install typescript --from pnpm --dry-run
```

## Search UX

Search results are ranked as:

- `Exact`
- `Related`
- `Fuzzy`

By default, Allp shows all exact matches, up to five related matches per backend, and at most 25 visible results. Weak fuzzy matches are hidden unless `--all` is used. This prevents broad native searches, especially APT searches for short names like `git`, from dumping thousands of weak library/package matches.

Related matches are not treated as equivalent software. If multiple meaningful sources exist, Allp asks the user to choose.

Cross-ecosystem matches are always separate choices. `APT · git`, `Homebrew · git`, `PyPI · gitpython`, and `npm · simple-git` are not assumed to be the same software.

Use `--scope` for a broad search domain and `--from` for a precise backend, source, ecosystem, or installer:

- `--scope apps`: system packages, universal applications, and Homebrew formula/cask results.
- `--scope dev`: Python/PyPI and Node.js/npm-registry results.
- `--scope all`: every eligible source, grouped as System Packages, Universal Applications, then Developer Ecosystems.

In an interactive terminal, `allp search <query>` and `allp install <query>` ask for one of exactly three scopes when no `--from` or `--scope` is provided: Apps and tools, Developer ecosystems, or All sources.

Ambiguous install results use stable global numbers. Large interactive result sets are paged: Space moves forward, `b` moves backward, `/` filters, a number selects directly, Enter selects the highlighted/first visible result, and `q` or Escape cancels. Non-TTY and JSON output never start this selector.

Python and Node registry results are treated cautiously. Fuzzy Python or Node matches are never installed automatically, registry/source is shown separately from installer choices, and dry runs never execute package lifecycle scripts.

## Dry Run

`--dry-run` still performs discovery, search, selection, and real execution-plan construction. It only skips native command execution.

Dry run is not a native package-manager simulation.

## Confirmation And Automation

Every real mutating operation requires a final Allp confirmation after package/target selection and after the execution plan is shown. This includes a single exact install result, remove, update, upgrade, project dependency changes, global tool changes, lockfile changes, and environment changes.

Removal defaults to No. Riskier upgrade batches also default to No.

Use `--yes` or `-y` only when choices are fully resolved and you want to bypass Allp's own final confirmation:

```bash
allp install git --from apt --yes
allp update --from npm --target global --yes
```

`--yes` never adds native package-manager auto-confirm flags, never bypasses ambiguity or installer/target selection, and never bypasses registry safety, PEP 668, Homebrew root protection, or ownership checks.

## Developer Updates

`allp update` and `allp upgrade` include detected Python and Node targets. Use `--target` to narrow development operations:

```bash
allp update --scope dev --target all --dry-run
allp update --from npm --target project --dry-run
allp update --from npm --target global --dry-run
allp update --from pnpm --target workspace --dry-run
allp update --from yarn --target project --dry-run
allp update --from pip --target environment --dry-run
allp update --from pipx --target tools --dry-run
allp update --from uv --target tools --dry-run
```

Node plans use native commands such as `npm update`, `npm update --global`, `pnpm update`, `pnpm update --latest`, and Yarn 1 or modern Yarn commands after version detection. Allp never generates `npx update`.

Python plans inspect pip outdated packages with JSON output, use `python -m pip install --upgrade ...` for selected active-environment packages, and support `pipx upgrade-all` and `uv tool upgrade --all`. Allp does not use sudo for original-user Python tools and never adds `--break-system-packages` automatically.

## JSON

Supported JSON commands:

```bash
allp detect --json
allp search git --json
allp list --json
allp info git --json
allp update --dry-run --json
allp upgrade --dry-run --json
```

JSON stdout uses a versioned envelope:

```json
{
  "schema_version": 1,
  "command": "search",
  "complete": true,
  "results": [],
  "issues": []
}
```

Human logs are not mixed into JSON stdout.

## Privilege Model

Preferred:

```bash
allp update
```

Avoid:

```bash
sudo allp update
```

Allp uses one centralized privilege policy. Plans can require current-user execution, administrator execution, or original-user execution when Allp was launched through sudo.

Allp elevates only the child process that requires root. Mutating native stdin, stdout, and stderr are inherited directly. If Allp is already running as root, root-required plans run directly without nested sudo and user-scoped plans such as Homebrew, Python, Node, and Flatpak-user are run as the original sudo user when that identity is available.

Before a root-required child command runs, Allp prints every planned native command, marks which plans need administrator access, explains that only child commands are elevated, and asks for final confirmation before real mutating execution. Dry runs never invoke sudo.

## List And Info UX

Large human-readable `list` output is paged automatically when stdout is an interactive terminal. Use `--no-pager` to force direct output, `--filter` to narrow results before limiting, and `--limit` to cap visible packages.

`info` defaults to curated package metadata. Use `--full` for normalized extended fields, `--raw` for native backend output, or `--json` for structured output.

## Exit Codes

```text
0   Success
2   Invalid CLI or input
3   Package not found
4   Ambiguous selection / source required
5   Requested backend not detected
6   Unsupported operation
7   Native command failed
8   Partial multi-backend failure
9   Timeout or cancellation
10  Internal error
11  Package manager busy
```

## Build And Test

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
```

## Documentation

- [Architecture](ARCHITECTURE.md)
- [Command contract](docs/CLI_CONTRACT.md)
- [Command semantics](docs/COMMANDS.md)
- [Backend contract](docs/BACKEND_CONTRACT.md)
- [Capability matrix](docs/CAPABILITY_MATRIX.md)
- [JSON schema](docs/JSON_SCHEMA.md)
- [Security model](docs/SECURITY_MODEL.md)
- [Privilege model](docs/PRIVILEGE_MODEL.md)
- [Confirmation model](docs/CONFIRMATION_MODEL.md)
- [Software identity](docs/SOFTWARE_IDENTITY.md)
- [Official bootstrap](docs/OFFICIAL_BOOTSTRAP.md)
- [Name collisions](docs/NAME_COLLISIONS.md)
- [Developer updates](docs/DEVELOPER_UPDATES.md)
- [v0.3.2 test plan](docs/V0_3_2_TEST_PLAN.md)
- [v0.3.3 test plan](docs/V0_3_3_TEST_PLAN.md)
- [Terminal UI](docs/TERMINAL_UI.md)
- [Linux coverage](docs/LINUX_COVERAGE.md)
- [Homebrew backend](docs/HOMEBREW_BACKEND.md)
- [Homebrew bootstrap](docs/HOMEBREW_BOOTSTRAP.md)
- [Python ecosystem](docs/PYTHON_ECOSYSTEM.md)
- [Node ecosystem](docs/NODE_ECOSYSTEM.md)
- [Roadmap](ROADMAP.md)
- [Adding a backend](docs/ADDING_BACKEND.md)
- [npm backend plan](docs/NPM_BACKEND_PLAN.md)

## License

MIT
