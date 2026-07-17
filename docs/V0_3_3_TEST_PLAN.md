# v0.3.3 Test Plan

Run the full quality gate:

```bash
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
```

Release-blocking identity tests use fake executables and temporary projects only:

- `allp install Homebrew --dry-run` with a fake npm registry result named `homebrew` must select the official Homebrew bootstrap plan.
- The fake npm `homebrew` package must remain visible as `Conflicting name`.
- `allp install homebrew --from npm --dry-run` must show the npm plan and conflict warning without executing npm.
- `allp install homebrew --from npm --yes` in non-interactive mode must not execute because `--yes` does not bypass conflicting-identity confirmation.
- Homebrew bootstrap dry-run must work even with no detected package-manager backend.
- The Homebrew bootstrap plan must not contain `curl | bash`.

Existing v0.3.2 confirmation, Python, Node, privilege, dry-run, and architecture tests remain part of the gate.
