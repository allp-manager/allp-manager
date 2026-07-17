# Contributing To Allp

Thank you for helping build Allp.

## Required Checks

Run the full local suite before opening a pull request:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
```

Do not claim a check passed unless you ran it.

## Architecture Rules

- Keep backend-specific command flags and parsers inside the backend.
- Generic operations must depend on capabilities, not backend IDs.
- Backends must return execution plans instead of spawning mutating processes.
- Never execute through `sh -c`.
- Never add automatic source recommendations.
- Preserve native terminal output for install, remove, update, and upgrade.
- Add fixtures for every parser change.

## Backend Pull Requests

Include:

- supported command versions;
- supported capabilities;
- real output fixtures;
- installation scope behavior;
- privilege behavior;
- known limitations;
- native commands used for search, list, info, install, remove, update, and upgrade.

## Bug Reports

Please include:

```bash
allp detect --json
allp detect --verbose
```

Also include Linux distribution, package-manager version, command executed, expected result, and actual native output.
