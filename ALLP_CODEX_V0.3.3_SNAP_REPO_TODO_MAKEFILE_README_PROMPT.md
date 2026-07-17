# Allp v0.3.3 — Focused Snap Repair, Repository Hygiene, TODO Completion, Makefile, and Professional Documentation

Read this specification completely before changing any code.

Work directly in the existing Allp repository.

This is a focused stabilization task for version:

```text
0.3.3
```

Do not create a replacement project.

Do not redesign the architecture.

Do not change unrelated behavior.

The allowed scope is limited to:

1. Fixing Snap candidate validation and classic-confinement installation.
2. Cleaning generated, local, temporary, and machine-specific files through a correct `.gitignore`.
3. Reviewing and completing release-relevant TODO/FIXME items.
4. Creating a useful project `Makefile`.
5. Creating complete, polished English and Persian README files.
6. Updating supporting documentation only where required.
7. Running the complete quality gate and reporting actual results.

Everything else must remain unchanged.

---

# 1. Strict Preservation Rules

Preserve all currently working behavior:

- interactive scope selection;
- paged and numbered results;
- search ranking;
- software-identity resolution;
- Homebrew bootstrap;
- final confirmation flow;
- `--yes`;
- dry-run semantics;
- sudo/root/original-user behavior;
- no nested sudo;
- update and upgrade behavior;
- Node and Python behavior;
- live execution progress;
- native output streaming;
- JSON behavior;
- exit-code behavior;
- install and remove behavior outside this Snap-specific fix;
- current backend registration;
- current command syntax;
- existing architecture boundaries;
- current color and terminal UI;
- all working tests.

Do not perform a broad refactor.

Do not rename public commands, flags, modules, JSON fields, or documented behavior unless absolutely required by this focused work.

Before editing:

1. Run `git status --short`.
2. Inspect `git diff`.
3. Inspect tracked generated files.
4. Inspect existing `.gitignore`.
5. Inspect existing README and documentation files.
6. Search for TODO/FIXME/stub markers.
7. Run the current tests.
8. Record existing failures.
9. Add focused regression tests.
10. Make the smallest possible changes.

---

# 2. Version Identity

The final version must remain:

```text
0.3.3
```

Do not create `0.3.4`.

Release title:

```text
Allp v0.3.3 — Snap Validation and Repository Stabilization
```

Update version references only when they are currently inconsistent with `0.3.3`.

---

# 3. Reproduced Snap Bug

Command:

```bash
sudo allp install pycharm
```

Current search result:

```text
Snap pycharm
Exact package name
Version: 2026.1.4
Publisher/source: jetbrains**
```

Current invalid plan:

```bash
/usr/bin/snap install pycharm
```

Native failure:

```text
error: snap "pycharm" not found
```

The official PyCharm Snap requires classic confinement.

Expected plan:

```bash
/usr/bin/snap install pycharm --classic
```

Do not hardcode PyCharm.

The fix must apply generically to every Snap package whose metadata requires classic confinement.

---

# 4. Search Candidates Are Not Install Plans

Never convert a raw `snap find` result directly into an executable installation plan.

Required flow:

```text
Snap search
→ normalize candidate
→ user selects candidate
→ revalidate with snap info
→ resolve canonical package name
→ resolve publisher and verification
→ resolve confinement
→ resolve architecture availability
→ resolve tracks and channels
→ inspect installed state
→ build immutable plan
→ final confirmation
→ execute
```

Search output can be incomplete, decorated, stale, or unavailable for the current architecture.

A selected candidate must be revalidated before plan construction.

---

# 5. Canonical Snap Metadata

After selection, query native Snap metadata using a stable command such as:

```bash
snap info <candidate-name>
```

Parse at least:

- canonical package name;
- display title;
- publisher/account;
- publisher verification state;
- summary;
- description;
- confinement;
- available tracks;
- available channels;
- stable channel availability;
- version and revision where available;
- architecture availability where exposed;
- installed state.

Do not use display title as the package ID.

Do not trust only the search-result table.

If metadata validation fails, do not run `snap install`.

---

# 6. Classic Confinement

When metadata reports:

```text
confinement: classic
```

the immutable install plan must include:

```bash
snap install <canonical-name> --classic
```

For PyCharm:

```bash
/usr/bin/snap install pycharm --classic
```

When a channel is selected:

```bash
/usr/bin/snap install pycharm --classic --channel=<track/channel>
```

Strictly confined packages must not receive `--classic`.

If classic confinement is unsupported on the current system, fail before execution with a clear diagnostic.

---

# 7. Channel and Track Validation

Before installation:

1. Determine whether a stable channel exists.
2. Prefer stable only when metadata proves it is available.
3. Ask the user to choose when multiple meaningful tracks exist.
4. Never silently choose candidate, beta, or edge.
5. Label channel risk explicitly.

Example:

```text
Available Channels

[1] latest/stable
[2] 2026.1/stable
[3] latest/candidate
[4] latest/beta
[5] latest/edge

Choose a channel [1-5, 0 to cancel]:
```

Non-stable selections require explicit confirmation.

---

# 8. Stale or Unavailable Snap Results

If `snap info <name>` fails after search:

```text
⚠ The selected Snap result is not currently installable.

Search result:
  pycharm

Validation:
  Snap Store metadata could not be resolved

Possible reasons:
  - stale search result;
  - package unavailable for this architecture;
  - selected channel unavailable;
  - temporary Snap Store failure.
```

Offer:

```text
[1] Search again
[2] Show Snap diagnostics
[0] Cancel
```

Do not start the install child process.

Do not represent this only as a generic native exit-code failure.

---

# 9. Publisher Normalization

Do not store decorations as part of the publisher name.

Current invalid value:

```text
jetbrains**
```

Normalize into separate fields:

```rust
publisher_name: "jetbrains"
publisher_verification: Verified
```

Human output:

```text
Publisher: JetBrains · Verified
```

Support plain and decorated native-output fixtures.

Do not infer official status solely from package name.

---

# 10. Installed-State Check

Before creating a normal install plan, inspect:

```bash
snap list <canonical-name>
```

Possible outcomes:

- not installed;
- installed and current;
- installed with refresh available;
- installed on a different channel;
- unknown.

When already installed:

```text
✔ PyCharm is already installed

Installed version: 2026.1.4
Installed channel: latest/stable
```

Do not execute a normal installation automatically.

Offer explicit actions where applicable:

```text
[1] Refresh
[2] Switch channel
[3] Reinstall
[0] Cancel
```

---

# 11. Required Snap Plan

A valid plan must include:

- software display name;
- canonical Snap package name;
- publisher;
- verification state;
- selected track/channel;
- confinement;
- operation type;
- privilege requirement;
- exact native command.

Expected example:

```text
Planned Operation

Snap
  Software: PyCharm
  Package: pycharm
  Publisher: JetBrains · Verified
  Channel: latest/stable
  Confinement: Classic
  Action: Install Snap application
  Command: /usr/bin/snap install pycharm --classic
  Privilege: Already running as administrator
```

Final confirmation remains required.

---

# 12. Snap Error Classification

Normalize Snap-specific errors where possible:

```text
PackageNotFound
MetadataUnavailable
ChannelUnavailable
ArchitectureUnsupported
ClassicConfinementRequired
ClassicConfinementUnsupported
StoreUnavailable
DaemonUnavailable
PermissionDenied
AlreadyInstalled
NativeFailure
```

Map:

```text
error: snap "<name>" not found
```

to `PackageNotFound`.

Preserve raw stderr in verbose diagnostics.

---

# 13. Repository Hygiene and `.gitignore`

Review the real repository before changing `.gitignore`.

The goal is that normal development commands such as:

```bash
git status
git add .
```

do not include generated build output, editor state, caches, logs, temporary files, local environment files, or secrets.

Do not blindly paste a generic `.gitignore`.

Inspect the repository and add only relevant patterns.

The `.gitignore` should normally cover applicable files such as:

```gitignore
# Rust build output
/target/

# Temporary and backup files
*.tmp
*.temp
*.log
*.bak
*.swp
*.swo
*~
.DS_Store
Thumbs.db

# IDE and editor-local files
.idea/
.vscode/
*.code-workspace

# Test and coverage artifacts
coverage/
coverage-*/
*.profraw
*.profdata
lcov.info

# Local environment and secrets
.env
.env.*
!.env.example

# Runtime/cache directories
.cache/
.tmp/
tmp/

# Locally generated packages and archives
*.deb
*.rpm
*.AppImage
*.dmg
*.msi
*.exe
*.tar.gz
*.zip
```

Only keep patterns that are appropriate for the actual repository.

Important rules:

- Keep `Cargo.lock` tracked because Allp is an application/CLI binary.
- Keep all source code tracked.
- Keep tests and fixtures tracked.
- Keep scripts tracked.
- Keep documentation tracked.
- Keep `.github/` tracked.
- Keep configuration examples tracked.
- Keep release metadata tracked.
- Do not ignore an entire directory merely because one generated file exists inside it.
- Do not hide real source files or required assets.
- Do not ignore files simply because they are currently untracked.

Inspect files already tracked that should be ignored.

For confirmed generated/local files already tracked, use:

```bash
git rm --cached <path>
```

only when safe.

Do not delete the user's local file unless necessary.

Do not run:

```bash
git clean -fdx
```

Validate rules with:

```bash
git status --short
git status --ignored
git check-ignore -v <path>
```

The final working tree should be predictable and easy to stage.

---

# 14. Secret and Credential Safety

Search tracked and untracked repository content for obvious accidental secrets:

- API keys;
- tokens;
- passwords;
- private keys;
- `.env` credentials;
- generated authentication files.

Do not print secret values.

If a likely secret is found:

1. Do not stage or commit it.
2. Add an appropriate ignore rule.
3. Create a safe example file where useful.
4. Report only the file path and secret type.
5. Do not rotate external credentials automatically.
6. Do not rewrite Git history unless explicitly requested.

Clearly fake test values may remain in test fixtures.

---

# 15. TODO, FIXME, Stub, and Placeholder Audit

Search the complete repository for:

```text
TODO
FIXME
XXX
HACK
unimplemented!
todo!
panic!("not implemented")
placeholder
stub
```

Also inspect:

- ignored tests;
- disabled tests;
- commented-out implementation blocks;
- placeholder errors;
- incomplete backend capabilities;
- README TODO sections;
- release blockers in ROADMAP;
- empty handlers or fake-success branches.

Classify each finding:

```text
A. Release-blocking and in scope
B. Release-blocking but outside this focused task
C. Small safe completion
D. Valid future roadmap item
E. Stale comment already completed
F. Intentional unsupported behavior
```

Required behavior:

- Complete all release-blocking TODOs directly related to Snap validation, repository hygiene, documentation, and tests.
- Complete small, safe TODOs that clearly belong to `0.3.3` and do not change unrelated behavior.
- Remove stale TODO comments where the implementation already exists.
- Move legitimate future work into `ROADMAP.md`.
- Keep intentional unsupported behavior explicit and tested.
- Do not silently delete difficult TODOs.
- Do not implement large unrelated features just to reduce the TODO count.
- Do not weaken or delete tests.
- Do not replace TODOs with vague comments.

Create or update:

```text
TODO.md
ROADMAP.md
```

Each remaining TODO should include:

- concise description;
- priority;
- reason it remains;
- related module;
- target milestone where known.

Final report must include:

- TODO/FIXME findings count;
- completed count;
- stale comments removed;
- items moved to roadmap;
- remaining count;
- remaining release blockers.

---

# 16. Required Makefile

Create a root-level file named:

```text
Makefile
```

The Makefile must be useful, simple, documented, and safe.

It must not duplicate complex business logic.

It should only orchestrate existing commands.

Required targets:

```make
help
fmt
fmt-check
check
clippy
test
architecture
build
release
quality
clean
run
version
git-status
docs-check
```

Recommended behavior:

```make
help:
	@show documented targets

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --all-targets

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all-targets

architecture:
	bash scripts/check-architecture.sh

build:
	cargo build

release:
	cargo build --release

quality:
	run fmt-check, check, clippy, test, architecture, and release

clean:
	cargo clean

run:
	cargo run -- $(ARGS)

version:
	cargo run -- --version

git-status:
	git status --short

docs-check:
	validate documentation using existing lightweight repository tooling
```

Requirements:

- `make help` must be the default target.
- Mark targets as `.PHONY`.
- Support passing CLI arguments through:

```bash
make run ARGS="search git"
```

- Do not require sudo.
- Do not run package installations.
- Do not mutate system state.
- Do not automatically commit, push, tag, or release.
- Do not hide command failures.
- Do not use destructive Git commands.
- Keep shell usage portable where practical.
- If `scripts/check-architecture.sh` is absent, do not invent a fake success; adapt the target with a clear message or use the existing equivalent.

Optional safe targets may include:

```make
install-local
uninstall-local
audit
todo
```

Only add them when implementation is accurate and safe.

Document the Makefile in both README files.

---

# 17. Professional README Documentation

Create or significantly improve:

```text
README.md
README.fa.md
```

The README must look polished, modern, and trustworthy.

It should be visually strong without becoming mostly badges or marketing.

Use:

- a strong title and one-line value proposition;
- concise feature highlights;
- clean tables;
- realistic terminal examples;
- architecture overview;
- clear warnings and limitations;
- links to deeper documentation;
- consistent section hierarchy.

At the top include language navigation:

```text
English | فارسی
```

`README.md` must be English.

`README.fa.md` must be natural Persian, not a low-quality literal translation.

The Persian document should be RTL-friendly in content organization.

Both READMEs must describe actual implemented behavior only.

Do not claim unfinished TODOs as complete.

---

# 18. Required README Sections

Both README files should include:

1. Project title.
2. Concise value proposition.
3. Current version `0.3.3`.
4. Current maturity: alpha/beta/stable, based on reality.
5. Why Allp exists.
6. Core principles:
   - native package managers remain visible;
   - transparent native commands;
   - no hidden shell pipelines;
   - source selection;
   - capability-based backends;
   - centralized privilege handling.
7. Supported operating systems.
8. Supported package managers and ecosystems.
9. Capability matrix:
   - search;
   - install;
   - remove;
   - update;
   - upgrade;
   - list;
   - info;
   - dry run;
   - JSON.
10. Installation from source.
11. Build requirements.
12. Release binary usage.
13. Quick start.
14. Search-scope selector explanation.
15. Installation examples.
16. Update and upgrade examples.
17. Normal-user versus sudo behavior.
18. Original-user handling under sudo.
19. Snap validation and classic confinement.
20. Node and Python ecosystem overview.
21. Dry-run usage.
22. JSON usage.
23. Makefile usage.
24. Troubleshooting:
   - APT lock;
   - DNF/RPM database issue;
   - missing pip/pipx/uv;
   - npm global permissions;
   - Flatpak user/system scope;
   - Snap metadata/channel/classic validation.
25. Security model.
26. Architecture overview.
27. Development workflow.
28. Test and quality commands.
29. Contributing.
30. Roadmap.
31. Changelog.
32. License.
33. Known limitations.

Do not include fake screenshots.

Do not include broken badges.

Only use badges that are accurate and stable.

---

# 19. README Command Examples

Examples must match the current CLI exactly.

Include examples such as:

```bash
allp detect
allp search git
allp install git
allp install pycharm
allp update
allp upgrade
allp update --scope dev
allp install git --dry-run
allp search git --json
```

Make sudo guidance explicit:

Preferred:

```bash
allp update
```

Allp should elevate only root-required child operations.

Also document safe behavior when intentionally invoked as:

```bash
sudo allp update
```

Do not recommend running all user-scoped package managers as root.

---

# 20. Additional Documentation

Create or update only where useful:

```text
CHANGELOG.md
ROADMAP.md
TODO.md
CONTRIBUTING.md
SECURITY.md
docs/ARCHITECTURE.md
docs/CLI_REFERENCE.md
docs/PRIVILEGE_MODEL.md
docs/BACKENDS.md
docs/TROUBLESHOOTING.md
```

Do not create duplicate documents when accurate equivalents already exist.

Update existing canonical documents instead.

Ensure relative links work.

Check documentation for stale version numbers and stale commands.

---

# 21. Changelog Entry

Add a `0.3.3` entry that includes only implemented changes:

- Snap candidate metadata revalidation;
- canonical Snap package resolution;
- classic-confinement plans;
- channel/track validation;
- publisher verification normalization;
- installed-state preflight;
- Snap diagnostics;
- repository hygiene;
- TODO audit;
- Makefile;
- README and documentation improvements.

Do not claim unrelated or incomplete work.

---

# 22. Required Snap Tests

Add fake Snap fixtures and regression tests for:

- plain publisher;
- decorated verified publisher;
- canonical package name from `snap info`;
- display title differing from canonical ID;
- classic confinement adds `--classic`;
- strict confinement does not add `--classic`;
- stable channel selection;
- multiple tracks require selection;
- edge-only package requires explicit confirmation;
- failed `snap info` blocks install;
- stale search result;
- unsupported architecture;
- already-installed package;
- installed package on another channel;
- publisher verification stored separately;
- PyCharm plan equals `snap install pycharm --classic`;
- dry run performs no installation;
- no nested sudo;
- centralized privilege handling;
- unrelated install flows remain unchanged.

Do not install real PyCharm in tests.

---

# 23. Repository Validation

Run:

```bash
git status --short
git status --ignored
git check-ignore -v target/
```

Ensure:

- `/target/` is ignored;
- local editor files are ignored;
- logs and temporary files are ignored;
- secrets are ignored;
- source files remain visible;
- tests and fixtures remain visible;
- docs remain visible;
- scripts remain visible;
- `.github/` remains visible;
- `Cargo.toml` remains visible;
- `Cargo.lock` remains tracked;
- the Makefile remains tracked;
- README files remain tracked.

Do not run `git clean -fdx`.

Do not delete unknown user files.

---

# 24. Documentation Validation

Verify:

- version is consistently `0.3.3`;
- README command examples exist;
- Makefile targets work;
- Markdown links resolve;
- English README is accurate;
- Persian README is complete;
- capability tables match actual support;
- no secret appears;
- no TODO is falsely presented as completed;
- no planned feature is marketed as implemented.

---

# 25. Manual Non-Destructive Snap Validation

Run:

```bash
allp install pycharm --dry-run
allp install pycharm --from snap --dry-run
allp search pycharm --from snap
snap info pycharm
```

Expected dry-run plan:

```bash
snap install pycharm --classic
```

Do not perform the real installation during automated validation.

---

# 26. Quality Gate

Run all of these:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
make help
make quality
```

If `make quality` invokes the same checks, still report the actual commands and avoid falsely claiming duplicate independent validation.

Do not claim a command passed unless it actually completed successfully.

---

# 27. Git Safety

Do not:

- commit automatically;
- push automatically;
- create a tag automatically;
- publish a release automatically;
- rewrite history;
- reset user changes;
- delete branches;
- run destructive checkout/reset commands;
- run `git clean -fdx`;
- delete files merely to make Git status clean.

Prepare the repository so the user can safely run:

```bash
git add .
git commit
git push
```

At completion, show the exact suggested commands but do not execute them.

Suggested commit title:

```text
fix: stabilize Snap installs and repository workflow
```

---

# 28. Final Report

Report:

1. Root cause of the Snap bug.
2. Snap metadata-validation flow.
3. Classic-confinement behavior.
4. Channel/track behavior.
5. Publisher normalization.
6. Installed-state behavior.
7. Snap error classifications.
8. `.gitignore` changes and reasons.
9. Files removed from Git tracking, if any.
10. Secret scan summary without exposing values.
11. TODO/FIXME audit counts.
12. TODOs completed.
13. Stale TODOs removed.
14. TODOs moved to roadmap.
15. Remaining release blockers.
16. Makefile targets.
17. README and documentation changes.
18. Exact files changed.
19. Commands actually run.
20. Actual test and quality-gate results.
21. Remaining limitations.
22. Confirmation that unrelated behavior was preserved.
23. Exact suggested `git add`, `git commit`, and `git push` commands.

Do not call the task complete if:

- PyCharm still plans `snap install pycharm` without `--classic`;
- raw Snap search results can become install plans without metadata validation;
- generated build output appears in normal `git add .`;
- `Cargo.lock` is ignored;
- source/tests/docs/scripts are accidentally ignored;
- release-relevant TODOs remain unexplained;
- README describes planned behavior as implemented;
- the Makefile contains destructive or privileged targets;
- unrelated working behavior was modified or regressed.
