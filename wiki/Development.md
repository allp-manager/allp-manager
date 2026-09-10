# Development and Release Workflow

[← Wiki home](Home.md) · [فارسی](Development.fa.md)

## Local setup

```bash
git clone https://github.com/allp-manager/allp-manager.git
cd allp-manager
rustup show
cargo build
cargo run -- detect
```

The minimum supported Rust version is 1.74; `rust-toolchain.toml` tracks stable
with rustfmt and Clippy.

## Quality gate

```bash
make fmt-check
make check
make clippy
make test
make architecture
make release
make docs-check
make quality       # all of the above
```

Tests use fake executables and captured fixtures. Never run destructive native
package operations in a test. Parser changes require representative fixture
output. Behavior changes require an Unreleased changelog entry, a regression
test and an updated guardrail.

## Backend change checklist

1. Declare identity, category, requirements and precise capabilities.
2. Keep native argv and parsing inside the backend module.
3. Return immutable plans for mutations; never spawn from a backend.
4. Define scope and privilege behavior.
5. Add success, no-match, malformed-output and failure fixtures.
6. Register once in `src/backends/catalog.rs`.
7. Run `make quality`.

## CI

The repository runs the Linux quality gate, portable tests on Linux/macOS/
Windows, Linux cross-target checks, family bootstrap contracts, and a weekly
native backend canary. Release jobs build target-specific archives, checksums
and a manifest.

## Local release

```bash
make hooks-install
make release-prepare BUMP=patch
make release-status
# commit with: release: Allp vX.Y.Z
make release-push
```

Preparation updates versioned files and runs the quality gate. The guarded
post-commit hook creates only an annotated local tag and ignored `dist/`
artifacts. Nothing is pushed or published until `make release-push` is called.

Read [CONTRIBUTING.md](https://github.com/allp-manager/allp-manager/blob/main/CONTRIBUTING.md),
[REGRESSION_GUARDRAILS.md](https://github.com/allp-manager/allp-manager/blob/main/docs/REGRESSION_GUARDRAILS.md), and the
[release documentation](https://github.com/allp-manager/allp-manager/blob/main/release/README.md) before submitting changes.
