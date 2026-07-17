# Allp — Makefile Installation and Local Release Preparation Workflow

Update the existing Allp repository Makefile and release workflow.

Do not modify unrelated application behavior. Preserve all existing working functionality.

The current project version must not change during normal builds, installs, tests, or ordinary commits. Version changes are allowed only through the explicit release-preparation workflow described below.

## Global installation

The Makefile must let the user run:

```bash
make install
```

and then invoke Allp globally from any directory:

```bash
allp update && allp upgrade
```

Use these variables:

```makefile
PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
BINARY := allp
RELEASE_BINARY := target/release/$(BINARY)
```

Required targets:

- `release`
- `install`
- `uninstall`
- `reinstall`
- `install-user`
- `install-check`

Required behavior:

### `make release`

- Run `cargo build --release`.
- Fail immediately if the build fails.
- Never run Cargo with sudo.

### `make install`

- Depend on `release`.
- Install only the final binary using:

```bash
sudo install -Dm755 target/release/allp /usr/local/bin/allp
```

- Use sudo only for writing to `/usr/local/bin`.
- Verify `/usr/local/bin/allp --version`.
- Print the installed path and version.

### `make uninstall`

- Remove only `/usr/local/bin/allp`.
- Do not remove source files, Cargo files, caches, configuration, or unrelated binaries.

### `make reinstall`

- Build successfully before replacing the installed binary.
- Do not remove a working installation before a successful build.
- Prefer safe or atomic replacement where practical.

### `make install-user`

- Install to `$(HOME)/.local/bin/allp` without sudo.
- Create the directory when missing.
- Warn if it is not in `PATH`.

### `make install-check`

- Print `command -v allp`.
- Run `allp --version`.
- Detect an older resolved installation.
- Print `rehash` for zsh and `hash -r` for Bash.

Update `make help`, `README.md`, and `README.fa.md` accordingly.

---

# Dedicated release workflow

Do not bump the version on every commit.

Do not attach release creation to ordinary VS Code commits.

Create an explicit release workflow that starts with:

```bash
make release-prepare
```

Also support:

```bash
make release-prepare VERSION=0.3.4
make release-prepare BUMP=patch
make release-prepare BUMP=minor
make release-prepare BUMP=major
```

Default:

```text
BUMP=patch
```

Example:

```text
0.3.3 → 0.3.4
```

Normal build, test, install, reinstall, and ordinary commits must never change the version.

## Required release targets

Add:

- `release-prepare`
- `release-status`
- `release-notes`
- `release-archive`
- `release-checksum`
- `release-finalize`
- `release-clean`
- `hooks-install`
- `hooks-status`

Suggested variables:

```makefile
DIST_DIR ?= dist
RELEASE_PREFIX ?= allp
BUMP ?= patch
VERSION ?=
```

Read the real version from `Cargo.toml`. Do not duplicate hardcoded versions across scripts.

## `make release-prepare`

This target prepares the next release but must not commit, tag, push, upload, or publish anything.

It must:

1. Confirm it is running in the Allp repository.
2. Confirm Git is available.
3. Reject unresolved merge conflicts.
4. Determine the next semantic version from `VERSION` or `BUMP`.
5. Validate semantic-version syntax.
6. Reject downgrades.
7. Reject an existing version or tag.
8. Update the package version in `Cargo.toml`.
9. Update `Cargo.lock` through Cargo, not manual text replacement.
10. Create or update the matching `CHANGELOG.md` section.
11. Create a GitHub-ready draft at:

```text
release/RELEASE_NOTES_vX.Y.Z.md
```

12. Run the full quality gate.
13. Store an ignored local release-ready marker containing the prepared version.
14. Print the exact VS Code commit message to use.

Expected final message:

```text
Release v0.3.4 is prepared.

Commit these files from VS Code using a message beginning with:

  release: Allp v0.3.4

After that commit, the repository-local hook will create:
  - annotated local tag v0.3.4
  - dist/allp-v0.3.4-source.tar.gz
  - dist/allp-v0.3.4-source.tar.gz.sha256
  - dist/RELEASE_NOTES_v0.3.4.md

Nothing will be pushed or published automatically.
```

## Release notes

The draft must be suitable for GitHub Releases and use actual Git history and `CHANGELOG.md`.

Do not invent changes.

Recommended structure:

```markdown
# Allp vX.Y.Z

## Highlights
## Added
## Changed
## Fixed
## Security
## Known Limitations
## Installation
## Upgrade
## Source Archive
## Checksums
```

Use editable placeholders only when history is insufficient.

---

# VS Code commit integration

The user must be able to prepare the release, then commit normally from the VS Code Source Control interface.

Create a tracked hooks directory:

```text
.githooks/
```

Create:

```text
.githooks/post-commit
```

The hook must run release finalization only when all these conditions are true:

1. A release-ready marker exists.
2. The commit message begins with `release:`.
3. The version in `Cargo.toml` matches the prepared version.
4. The commit contains the prepared version, changelog, and release-note changes.
5. No conflicting release changes remain.
6. The tag does not already exist.

An ordinary commit such as:

```text
fix: improve Snap parsing
```

must do nothing release-related.

A release commit such as:

```text
release: Allp v0.3.4
```

may finalize the local release.

Do not alter global Git hooks.

Enable repository-local hooks only through:

```bash
make hooks-install
```

which runs:

```bash
git config core.hooksPath .githooks
```

`make hooks-status` must report whether hooks are active.

Document that cloning the repository does not activate hooks until `make hooks-install` is run.

## Post-commit finalization script

Put complex logic in a script such as:

```text
scripts/release-finalize.sh
```

The hook should only validate context and invoke the script.

The script must:

1. Read the committed version from `Cargo.toml`.
2. Verify the release-ready marker.
3. Verify the commit message begins with `release:`.
4. Verify the tag does not exist.
5. Create an annotated local tag `vX.Y.Z` pointing to the exact release commit.
6. Create:

```text
dist/allp-vX.Y.Z-source.tar.gz
```

7. Create:

```text
dist/allp-vX.Y.Z-source.tar.gz.sha256
```

8. Finalize/copy release notes to:

```text
dist/RELEASE_NOTES_vX.Y.Z.md
```

9. Print the exact manual push and upload steps.

Expected output:

```text
Local release v0.3.4 is ready.

Tag:
  v0.3.4

Artifacts:
  dist/allp-v0.3.4-source.tar.gz
  dist/allp-v0.3.4-source.tar.gz.sha256
  dist/RELEASE_NOTES_v0.3.4.md

Nothing was pushed to GitHub.

Review the files, then run:
  git push origin <branch>
  git push origin v0.3.4

Upload the files from dist/ to the GitHub Release.
```

---

# Source archive

Create the archive from the exact committed tag, never from an arbitrary dirty working tree.

Use:

```bash
git archive \
  --format=tar.gz \
  --prefix=allp-vX.Y.Z/ \
  --output=dist/allp-vX.Y.Z-source.tar.gz \
  vX.Y.Z
```

The archive must not contain:

- `.git/`
- `target/`
- `dist/`
- local release markers
- editor state
- local caches
- `.env` files
- logs
- temporary files
- old archives
- generated binaries
- secrets

Keep tracked source, tests, fixtures, scripts, docs, Cargo files, Makefile, README files, license, changelog, roadmap, TODO, contributing, security, and GitHub workflow files.

## Checksum

Create:

```bash
sha256sum dist/allp-vX.Y.Z-source.tar.gz \
  > dist/allp-vX.Y.Z-source.tar.gz.sha256
```

The checksum file must contain the archive base filename, not an absolute path.

Validate it with:

```bash
cd dist
sha256sum -c allp-vX.Y.Z-source.tar.gz.sha256
```

Update finalized release notes with the real SHA-256 value.

---

# `.gitignore`

Ensure these are ignored:

```gitignore
/target/
/dist/
/.release-state/
*.log
*.tmp
*.temp
*.bak
*.swp
*.swo
*~
.env
.env.*
!.env.example
```

Do not ignore:

- `Cargo.lock`
- `Makefile`
- `.githooks/`
- `scripts/`
- `release/`
- `README.md`
- `README.fa.md`
- source files
- tests
- fixtures
- documentation

Draft release notes under `release/` must be tracked.

Generated final assets under `dist/` must remain ignored and ready for manual GitHub upload.

---

# Release status and fallback

`make release-status` must report:

- current version
- prepared version
- latest local tag
- working-tree state
- release marker state
- hook activation state
- archive paths
- whether the release tag exists
- next action

It must not modify anything.

Provide a manual fallback:

```bash
make release-finalize
```

It must perform the same local finalization as the post-commit hook and require:

- committed release state
- matching version
- no conflicting tag
- successful consistency checks

Never tag uncommitted changes.

`make release-clean` may remove only ignored generated artifacts and local release markers. It must never delete commits, tags, source, release-note drafts, Cargo files, or docs.

---

# VS Code tasks

Create tracked VS Code tasks only if repository policy allows it.

Preferred tasks:

- `Allp: Prepare Patch Release`
- `Allp: Prepare Minor Release`
- `Allp: Prepare Major Release`
- `Allp: Release Status`
- `Allp: Finalize Local Release`
- `Allp: Build and Install`
- `Allp: Reinstall`

Tasks must call Makefile targets and must not duplicate release logic.

If `.vscode/` remains intentionally ignored, provide an example under:

```text
contrib/vscode/tasks.json
```

---

# README updates

Update `README.md` and `README.fa.md` with this workflow:

```bash
make hooks-install
make release-prepare BUMP=patch
```

Then commit from VS Code using:

```text
release: Allp v0.3.4
```

Expected local files after commit:

```text
dist/allp-v0.3.4-source.tar.gz
dist/allp-v0.3.4-source.tar.gz.sha256
dist/RELEASE_NOTES_v0.3.4.md
```

Then the user manually runs:

```bash
git push origin <branch>
git push origin v0.3.4
```

State clearly:

> Allp prepares the local release tag and assets, but never pushes or publishes them automatically.

Document `make release-finalize` as a fallback.

---

# Safety requirements

Never:

- bump version on every commit
- trigger release from ordinary commits
- automatically commit
- automatically push commits
- automatically push tags
- create or publish a GitHub Release automatically
- upload assets automatically
- rewrite history
- move/delete existing tags automatically
- overwrite existing archives silently
- run release creation with sudo
- run Cargo as root
- archive dirty uncommitted files
- include `target/` or secrets in source archives
- create tags before the release commit exists

Generated release assets must remain local until manually reviewed and uploaded.

---

# Tests

Use temporary Git repositories and add non-destructive tests for:

- ordinary commit does not trigger release
- release commit without marker does nothing or fails safely
- mismatched version fails safely
- valid release commit creates the expected local tag
- tag points to the exact release commit
- source archive is created from the tag
- archive prefix is correct
- archive excludes `.git`, `target`, `dist`, `.env`, and markers
- checksum validates
- release notes contain version and checksum
- existing tag is never overwritten
- existing archive is not silently overwritten
- patch/minor/major bump works
- explicit VERSION works
- invalid semantic version fails
- downgrade fails
- install/reinstall behavior remains unchanged
- release creation uses no sudo

Do not create tags in the developer's real repository during automated tests.

---

# Quality gate

Run:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
make help
make release-status
```

Test release scripts only in temporary repositories.

Do not tag or finalize the real repository during validation.

---

# Final report

Report:

1. Makefile targets added.
2. Installation workflow.
3. Version-bump logic.
4. Release-ready marker behavior.
5. VS Code commit workflow.
6. Hook activation.
7. Tag creation behavior.
8. Source archive behavior.
9. SHA-256 behavior.
10. Release-note generation.
11. `.gitignore` changes.
12. Exact files changed.
13. Commands actually run.
14. Actual test results.
15. Remaining limitations.
16. Confirmation that normal commits do not create releases.
17. Confirmation that nothing is pushed or published automatically.

Do not claim completion if ordinary commits can bump versions or create tags, if the archive is built from uncommitted files, if the tag does not point to the release commit, if generated assets are automatically committed, or if anything is automatically pushed or published.
