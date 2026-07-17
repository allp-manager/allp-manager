# Allp

[English](README.md) | [فارسی](README.fa.md)

> One CLI for the package managers already on your machine.

Allp is a transparent package-manager orchestrator for Linux. It discovers native tools such as APT, Pacman, DNF, Flatpak, Snap, Homebrew/Linuxbrew, Python installers, and Node installers, then shows the exact native command before anything mutates the system.

Current version: **0.3.3**
Maturity: **public alpha**
Release title: **Allp v0.3.3 - Snap Validation and Repository Stabilization**

## Why Allp Exists

Linux software often lives across system repositories, universal app stores, Homebrew, Python, and Node. Allp gives those sources one consistent command surface without hiding the native package managers or pretending they are interchangeable.

Core principles:

- Native package managers remain visible and authoritative.
- Every mutating plan shows the exact native command.
- Commands are executed directly, not through hidden shell pipelines.
- Source selection is explicit when names collide.
- Backends advertise capabilities instead of generic code guessing behavior.
- Privilege handling is centralized and child-process only.

## Supported Systems

Allp targets Linux and Linux-like environments where the relevant native package managers are already installed. Homebrew support covers Linuxbrew behavior in this release; macOS validation remains experimental.

## Backends

| Source | Status | Search | Install | Remove | Update | Upgrade | List | Info |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| APT | Stable alpha | yes | yes | yes | yes | yes | yes | yes |
| Pacman | Stable alpha | yes | yes | yes | no | yes | yes | yes |
| DNF / DNF5 | Stable alpha | yes | yes | yes | yes | yes | yes | yes |
| Flatpak | Stable alpha | yes | yes | yes | yes | yes | yes | yes |
| Snap | Stable alpha | yes | yes | yes | yes | yes | yes | yes |
| Zypper, APK, XBPS, Portage, eopkg, swupd | Experimental | yes | mixed | mixed | mixed | mixed | mixed | mixed |
| Homebrew / Linuxbrew | Experimental | yes | yes | yes | yes | yes | yes | yes |
| Python: PyPI with pip, pipx, uv | Experimental | yes | yes | yes | yes | yes | yes | yes |
| Node: npm registry with npm, pnpm, Yarn | Experimental | yes | yes | yes | yes | yes | yes | yes |

JSON is available for read-only commands and dry-run maintenance/install planning where supported. See [docs/CAPABILITY_MATRIX.md](docs/CAPABILITY_MATRIX.md) for the detailed matrix.

## Installation

Build from source:

```bash
git clone https://github.com/Aliazadi-1776/allp.git
cd allp
cargo build --release
./target/release/allp --version
```

Install the release binary globally:

```bash
make install
allp --version
allp update && allp upgrade
```

`make install` builds the release binary and installs it as
`/usr/local/bin/allp`. It uses `sudo install` for that one file copy. For a
user-local install without sudo:

```bash
make install-user
```

Requirements:

- Rust 1.74 or newer
- Cargo
- The native package managers you want Allp to detect
- `sudo` only when a root-required child command actually needs elevation

Release binary usage is simply:

```bash
allp detect
allp search git
```

## Quick Start

```bash
allp detect
allp search git
allp install git
allp install git --dry-run
allp install pycharm
allp update
allp upgrade
allp update --scope dev
allp search git --json
```

Use `--from` for a precise backend:

```bash
allp install git --from apt --dry-run
allp install pycharm --from snap --dry-run
allp install black --from pipx --dry-run
allp install typescript --from pnpm --dry-run
```

## Search And Selection

Without `--from` or `--scope`, interactive `search` and `install` ask for one of three scopes:

- `apps`: system packages, universal applications, and Homebrew
- `dev`: Python and Node ecosystems
- `all`: every eligible source

Results are ranked as `Exact`, `Related`, or `Fuzzy`. All exact matches are shown, related matches are capped per backend, and weak fuzzy matches require `--all`.

Large interactive result sets keep stable global numbers. Space moves forward, `b` moves back, `/` filters, a number selects directly, Enter selects the highlighted or first visible result, and `q` or Escape cancels.

## Privilege Behavior

Preferred:

```bash
allp update
```

Allp itself should normally run as your user. Root-required child plans use `sudo --` only after the plan is shown and confirmed. Dry runs never invoke sudo.

If you intentionally run:

```bash
sudo allp update
```

Allp does not add nested sudo. Root-required system plans run directly, and user-scoped plans such as Homebrew, Python, Node, and Flatpak-user run as the original sudo user when that identity is available.

`--yes` bypasses only Allp's final confirmation after choices are resolved. It never adds native `-y`, `--assumeyes`, or equivalent flags.

## Snap Validation

Snap install planning no longer trusts raw `snap find` rows. After a Snap result is selected, Allp runs `snap info <candidate>` and resolves:

- canonical package name and display title;
- publisher and verification state;
- confinement;
- architecture availability;
- tracks and channels;
- stable-channel availability;
- installed state via `snap list <canonical-name>`.

Classic confinement is added only when metadata requires it:

```bash
allp install pycharm --from snap --dry-run
# planned native command includes:
snap install pycharm --classic
```

Strict snaps do not receive `--classic`. If no stable channel exists, or multiple stable tracks exist without a safe default, Allp stops instead of silently choosing candidate, beta, or edge. A dedicated interactive channel chooser is still a known limitation of this alpha.

## Python And Node

Python support treats PyPI as the source and pip, pipx, and uv as installer choices. Node support treats the npm registry as the source and npm, pnpm, and Yarn as installer choices. Registry matches are not treated as official merely because a name looks familiar, and fuzzy Python/Node matches are never installed automatically.

Examples:

```bash
allp search openai --from python
allp install black --from pipx --dry-run
allp search typescript --from node
allp install typescript --from pnpm --dry-run
allp update --scope dev --target all --dry-run
```

## Dry Run And JSON

Dry run performs discovery, search, selection, metadata validation, and execution-plan construction. It skips only mutating native command execution.

```bash
allp install git --dry-run
allp install pycharm --from snap --dry-run
allp update --dry-run
```

JSON examples:

```bash
allp detect --json
allp search git --json
allp list --json
allp info git --json
allp update --dry-run --json
```

Human logs are not mixed into JSON stdout.

## Makefile Workflow

The root Makefile keeps development, installation, and local release work in
plain commands:

```bash
make help
make fmt
make fmt-check
make check
make clippy
make test
make architecture
make build
make release
make quality
make run ARGS="search git"
make version
make git-status
make docs-check
make install
make reinstall
make uninstall
make install-user
make install-check
```

`make install`, `make reinstall`, and `make uninstall` use sudo only to manage
`/usr/local/bin/allp`. They do not install native packages, run Allp package
operations, commit, push, tag, publish, or hide failures.

## Local Release Workflow

The release workflow is local by design. It does not push, publish a GitHub
Release, or upload assets.

Run once per clone:

```bash
make hooks-install
```

Prepare the next version explicitly:

```bash
make release-prepare BUMP=patch
# or:
make release-prepare VERSION=0.3.4
```

`release-prepare` updates the package version, Cargo.lock through Cargo,
CHANGELOG, README version references, and a tracked draft such as
`release/RELEASE_NOTES_v0.3.4.md`, then runs `make quality`. It writes an
ignored readiness marker only after that quality gate passes.

Commit the prepared files normally, for example from VS Code Source Control:

```text
release: Allp v0.3.4
```

Only a commit whose subject begins with `release:` and matches the prepared
marker is finalized. The post-commit hook creates:

- annotated local tag `v0.3.4`
- `dist/allp-v0.3.4-source.tar.gz`
- `dist/allp-v0.3.4-source.tar.gz.sha256`
- `dist/RELEASE_NOTES_v0.3.4.md`

The source archive is generated from the exact committed tag with `git archive`.
Ordinary commits such as `fix: improve Snap parsing` do not change versions,
create tags, or generate `dist/` output. Use `make release-status` to inspect
state and `make release-workflow-test` to run the temp-repository automation
tests. VS Code task examples live in `contrib/vscode/tasks.json` because
`.vscode/` is editor-local and ignored.

## Troubleshooting

| Symptom | What to do |
|---|---|
| APT lock error | Wait for the owning package process. Do not delete dpkg lock files. |
| DNF/RPM database error | Check rpmdb permissions or repair the RPM database before retrying. |
| Missing pip, pipx, or uv | Run `allp detect --verbose` and install/configure the missing Python tool intentionally. |
| npm global permission issue | Fix npm prefix ownership or use a user-owned Node manager; Allp will not sudo npm globals. |
| Flatpak user/system mismatch | Use `allp list --from flatpak` and choose the installed scope. |
| Snap metadata failure | Run `snap info <name>` and `allp search <name> --from snap --all`; stale search rows are blocked before install. |
| Snap classic failure | Use the validated Allp plan; classic snaps include `--classic` only after `snap info` proves it. |

## Security Model

Allp stores commands as program plus argument vector and never executes package-manager work through `sh -c`. Native package output is data, not trusted code. Dry runs do not execute installers. Allp does not store sudo passwords, collect telemetry, or add native confirmation flags.

Read [SECURITY.md](SECURITY.md) for reporting and alpha limitations.

## Architecture

```text
CLI -> discovery -> operation -> backend parser/planner -> renderer -> process runner
```

Backends own native command syntax and parsers. Generic operations coordinate capabilities, selection, confirmation, and execution plans. The runner owns direct process execution, output streaming, sudo handling, and original-user de-escalation.

See [ARCHITECTURE.md](ARCHITECTURE.md), [docs/BACKEND_CONTRACT.md](docs/BACKEND_CONTRACT.md), and [docs/PRIVILEGE_MODEL.md](docs/PRIVILEGE_MODEL.md).

## Development

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
make quality
```

Use fake executable fixtures for package-manager behavior. Do not run destructive package-manager operations in tests.

## Contributing

Keep backend-specific parsing and flags inside backend modules. Add fixtures for parser changes. Preserve command syntax, JSON contracts, privilege behavior, dry-run semantics, and terminal UI expectations. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Roadmap

Near-term work is broader real-distro validation, richer parser fixtures, diagnostics/doctor support, and a safer Snap channel-selection UX. Future ecosystems such as Cargo, Composer, Go, RubyGems, Maven/Gradle, and GUI/TUI modes are not implemented in 0.3.3.

See [ROADMAP.md](ROADMAP.md) and [TODO.md](TODO.md).

## Changelog

0.3.3 stabilizes Snap install planning, repository hygiene, release documentation, and the Makefile workflow. See [CHANGELOG.md](CHANGELOG.md).

## Known Limitations

- Allp is public alpha software and not security-audited.
- Snap multiple-track selection is conservative and may require native `snap` commands for explicit channel choice.
- Experimental backends need broader validation on real distributions and host setups.
- Python and Node project-scope policies remain intentionally cautious.
- Signal forwarding and deeper trusted-path validation remain future hardening work.

## License

MIT. See [LICENSE](LICENSE).
