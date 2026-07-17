# v0.3.2 Test Plan

Quality gate:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
```

Non-destructive manual matrix:

```bash
allp install git --from apt --dry-run
allp update --dry-run
allp update --scope dev --dry-run
allp update --from npm --target project --dry-run
allp update --from npm --target global --dry-run
allp update --from pnpm --target project --dry-run
allp update --from yarn --target project --dry-run
allp update --from pip --target environment --dry-run
allp update --from pipx --target tools --dry-run
allp update --from uv --target tools --dry-run
allp upgrade --scope dev --dry-run
allp upgrade --from npm --target project --dry-run
allp upgrade --from pnpm --target project --dry-run
allp upgrade --from pipx --target tools --dry-run
allp upgrade --from uv --target tools --dry-run
```

Mutation tests must use temporary projects and fake executables. Do not update or modify the developer's primary projects.
