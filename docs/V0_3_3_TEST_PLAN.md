# v0.3.3 Test Plan

Run the full quality gate:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
make help
make quality
```

Release-blocking identity tests use fake executables and temporary projects only:

- `allp install Homebrew --dry-run` with a fake npm registry result named `homebrew` must select the official Homebrew bootstrap plan.
- The fake npm `homebrew` package must remain visible as `Conflicting name`.
- `allp install homebrew --from npm --dry-run` must show the npm plan and conflict warning without executing npm.
- `allp install homebrew --from npm --yes` in non-interactive mode must not execute because `--yes` does not bypass conflicting-identity confirmation.
- Homebrew bootstrap dry-run must work even with no detected package-manager backend.
- The Homebrew bootstrap plan must not contain `curl | bash`.
- `allp install pycharm --from snap --dry-run` with fake Snap metadata must plan `snap install pycharm --classic`.
- Strict Snap metadata must not add `--classic`.
- Snap search publisher decorations such as `JetBrains**` must be normalized into publisher name plus verification state.
- Snap info canonical names must replace stale or display-only search IDs before plan construction.
- Failed `snap info`, unsupported architecture metadata, edge-only channels, and ambiguous stable tracks must block install before execution.
- Already-installed Snap packages must not plan a normal install.
- Real fake Snap execution must use centralized sudo and must not create nested sudo.
- Dry-run Snap install tests must not invoke fake `snap install`.

Repository and documentation checks:

- `/target/`, logs, temporary files, local env files, secrets, caches, and generated packages are ignored.
- `Cargo.lock`, source, tests, fixtures, scripts, docs, `.github`, Makefile, and release metadata remain visible to Git.
- `cargo-check.log` is not tracked.
- `README.md`, `README.fa.md`, `TODO.md`, `ROADMAP.md`, and `CHANGELOG.md` describe v0.3.3 accurately.

Existing v0.3.2 confirmation, Python, Node, privilege, dry-run, and architecture tests remain part of the gate.
