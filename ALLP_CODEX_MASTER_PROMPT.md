# Allp — Master Implementation Prompt for Codex

## Role

You are the principal Rust engineer responsible for turning the existing **Allp** repository into a clean, modular, testable, user-friendly Linux package-manager orchestrator.

Work directly in the existing repository. Inspect the current code before editing it. Preserve useful existing work, but do not preserve broken UX, incorrect abstractions, or accidental API decisions merely for compatibility with an unreleased alpha.

Do not create a second implementation beside the current one. Refactor the existing project in place.

The expected result is a production-quality **v0.1 alpha** suitable for public GitHub testing.


# 1. Product Definition

Allp is **not a new package manager**.

Allp is a transparent command-line orchestrator over package managers already installed on a Linux system.

Its core promise is:

> One CLI. Your native package managers. No hidden magic.

Allp must:

1. Detect supported package managers dynamically on every invocation.
2. Ask each detected backend what operations it supports.
3. Search across eligible backends.
4. Normalize results into a common model.
5. Let the user choose the source when more than one valid source exists.
6. Show the exact native command before execution.
7. Execute the native package manager directly.
8. Stream native stdin, stdout, and stderr.
9. Avoid reimplementing package-manager logic.
10. Never silently recommend or choose a source when multiple meaningful choices exist.

Allp must remain an orchestrator, not become a replacement for APT, Pacman, DNF, Flatpak, Snap, npm, Cargo, or any future backend.


# 2. Version 0.1 Scope

## Stable backends for v0.1

### System package managers

- APT
- Pacman
- DNF / DNF5

### Universal package managers

- Flatpak
- Snap

## Planned future backends

These must appear in documentation and roadmap, but must not be partially or falsely advertised as stable in v0.1:

- Zypper
- APK
- Cargo
- npm
- pnpm
- Yarn
- pip
- pipx
- Composer
- Go

The architecture must make adding one of these later require minimal changes.

Adding npm later should ideally require:

1. A new backend module.
2. Backend-local parsing and command planning.
3. One registration entry in the backend catalog.
4. Backend-specific tests and fixtures.

Adding npm must not require modifying generic operations such as search, install, remove, update, upgrade, list, or info.


# 3. Canonical CLI Contract

The official command shape is:

```text
allp <command> [arguments] [options]
```

Canonical examples:

```bash
allp detect
allp detect --json
allp detect --verbose

allp search git
allp search git --from apt
allp search git --exact
allp search git --limit 10
allp search git --all
allp search git --json

allp install git
allp install git --from apt
allp install git --from apt --dry-run
allp install git --no-interactive

allp remove git
allp remove git --from apt
allp remove git --from apt --dry-run

allp update
allp update --from apt
allp update --dry-run

allp upgrade
allp upgrade --from apt
allp upgrade --dry-run

allp list
allp list --from apt
allp list --json

allp info git
allp info git --from apt
allp info git --json
```

Do not document mutation flags before the command.

Avoid this as the primary syntax:

```bash
allp --dry-run update
allp --from apt install git
```

Implement options as command-specific option structs using Clap flattening where appropriate.

Only truly root-level behavior should remain globally positioned, for example:

```bash
allp --help
allp --version
```

`--verbose` and `--no-color` may be implemented as reusable common options, but the documented and tested canonical form should remain command-first.


# 4. Required Commands

## 4.1 `allp detect`

Purpose:

- Detect every built-in backend on every invocation.
- Show detected and unavailable backends.
- Show executable paths.
- Show capability status.
- Help users create useful bug reports.

Detection states must not be represented as one boolean. Use explicit states such as:

- Ready
- NotFound
- FoundButUnavailable
- FoundButUnconfigured
- UnsupportedVersion
- ProbeFailed

Normal commands should use lightweight discovery. `detect --verbose` may perform deeper probes.

Example:

```text
Detected package managers

System
✓ APT       Ready        /usr/bin/apt-get
✗ Pacman    Not found
✗ DNF       Not found

Universal
✓ Flatpak   Ready        /usr/bin/flatpak
⚠ Snap      Unavailable  snapd is not responding
```

## 4.2 `allp search <query>`

Search all detected backends advertising Search capability.

Search must not dump thousands of weak results. The current behavior that prints thousands of APT matches for `git` is unacceptable.

Use at least three match levels:

- Exact
- Related
- Fuzzy

For query `git`:

```text
git                                Exact
git-scm                            Related
git-cola                           Related
golang-github-git-lfs-dev          Fuzzy
libtest-requires-git-perl          Fuzzy
```

Default policy:

- show all exact matches;
- show at most 5 strong related matches per backend;
- show at most 25 visible results in total;
- hide weak fuzzy results unless `--all` is used.

Support:

```bash
allp search git --exact
allp search git --limit 10
allp search git --all
allp search git --from snap
```

Use deterministic sorting:

1. Match class: Exact, Related, Fuzzy
2. Backend category: System, Universal, Development
3. Backend name
4. Package ID

Run backend searches concurrently with bounded concurrency. Apply timeout per backend. Report partial failures explicitly.

## 4.3 `allp install <query>`

Flow:

1. Fresh discovery.
2. Select eligible Search + Install backends.
3. Search.
4. Normalize candidates.
5. Rank candidates.
6. Show meaningful source choices.
7. User selection.
8. Build immutable ExecutionPlan.
9. Show exact native command.
10. Execute unless `--dry-run`.

Do not discard meaningful related results merely because one exact result exists.

Example:

```text
APT      git       Exact
Snap     git-scm   Related
Snap     git-cola  Related
```

The user should see these choices with a warning:

```text
Related matches may not represent the same software.
```

Never assume similar names represent the same project.

Auto-selection is allowed only when exactly one strong candidate exists and every eligible backend completed successfully.

If one backend fails or times out, do not claim uniqueness.

In non-interactive mode, ambiguity must fail with a stable exit code and explain how to use `--from` or an exact package ID.

## 4.4 `allp remove <query>`

Search installed inventories only. Do not search remote repositories first.

If one installed package matches, build its removal plan.

If the same software is installed through multiple backends, prompt:

```text
Installed copies found

[1] APT      code
[2] Flatpak  com.visualstudio.code
```

Do not infer ownership solely from PATH.

## 4.5 `allp update`

Every invocation must begin with fresh discovery.

Definition:

> Run the backend-defined native update action for every detected backend advertising Update capability.

Do not force identical semantics.

Examples:

- APT update refreshes package metadata.
- Snap refresh may update installed snaps.
- Flatpak update may update applications.
- Pacman does not safely support an APT-style metadata-only refresh.

The UI must describe the action:

```text
Allp Update · Dry Run

Detected package managers
✓ APT
✓ Snap

Planned operations

1. APT
   Action: Refresh package metadata
   Command: sudo /usr/bin/apt-get update

2. Snap
   Action: Refresh installed snaps
   Command: sudo /usr/bin/snap refresh

Dry run completed
2 operations planned
0 commands executed
```

Run mutating operations sequentially. Continue after failures by default. Return non-zero if one or more backends fail. Print a final summary.

## 4.6 `allp upgrade`

Run the backend-defined native bulk-upgrade action for supported detected backends.

Backends without a safe bulk-upgrade operation must return Unsupported. Allp must not invent an upgrade-all implementation.

Update and Upgrade may map to the same native command for some backends, but the action metadata must explain it.

## 4.7 `allp list`

List installed packages grouped by backend.

Support:

```bash
allp list
allp list --from apt
allp list --json
```

Use paging for large human-readable output.

## 4.8 `allp info <query>`

First inspect installed inventories. If multiple installed matches exist, prompt. If no installed match exists, query remote information.

Show:

- backend;
- package ID;
- display name;
- version;
- source/repository;
- scope;
- description;
- artifact type;
- installed state when known.


# 5. CLI and UX Principles

The CLI must feel deliberate, not like debug output.

Required:

- clear headings;
- small result sets by default;
- stable ordering;
- consistent symbols;
- no spinner over native package-manager output;
- spinner only during discovery/query work;
- exact native command before execution;
- clear distinction between package ID, display name, backend, source, scope, artifact type, and match type;
- clear dry-run summary;
- clear partial-failure summary;
- recovery instructions in errors.

Avoid:

- thousands of results;
- giant unusable numbered lists;
- treating every substring as useful;
- silently preferring APT;
- hiding meaningful alternative backends;
- repainting native transaction progress;
- exposing raw Rust enum names.

Use human labels:

```text
Exact
Related
Fuzzy
Success
Failed
Skipped
Dry run
```

Do not show raw forms like `DryRun` or `MatchKind::Fuzzy`.


# 6. Architecture Requirements

Recommended structure:

```text
src/
├── main.rs
├── lib.rs
├── app/
├── cli/
├── domain/
├── discovery/
├── operations/
├── execution/
└── backends/
    ├── catalog.rs
    ├── contract.rs
    ├── system/
    ├── universal/
    └── development/
```

## `main.rs`

Keep minimal:

- parse CLI;
- call bootstrap;
- map errors to exit codes.

No backend-specific logic.

## `app`

Composition root:

- create backend catalog;
- create discovery;
- create renderer;
- create runner;
- dispatch use cases.

Do not build a god object.

## `domain`

Pure models:

- BackendId
- BackendCategory
- Capability
- DetectionState
- BackendStatus
- PackageCandidate
- InstalledPackage
- PackageInfo
- MatchKind
- ArtifactKind
- Scope
- Source
- OperationKind
- BackendAction
- NativeCommand
- ExecutionPlan
- OperationResult
- MultiOperationReport
- BackendIssue
- AllpError
- ExitCode

No backend parsers in domain.

## `discovery`

Responsibilities:

- search PATH directly in Rust;
- resolve executable paths;
- create a per-invocation detected backend set;
- perform optional probes;
- return explicit states.

Do not shell out to `which` or `command -v`.

Use distinct names:

- BackendCatalog: known built-in backends.
- BackendDiscovery: performs detection.
- DetectedBackendSet: usable backends for one invocation.

## `backends`

Each backend owns:

- identity;
- category;
- requirements;
- capabilities;
- probes;
- query commands;
- parsers;
- plan construction;
- human action descriptions.

Generic operations must not contain backend names or native command strings.

## `operations`

Separate use-case modules:

- detect
- search
- install
- remove
- update
- upgrade
- list
- info

Operations coordinate contracts only. They do not know command syntax and do not parse backend output.

## `execution`

Responsibilities:

- safe command rendering;
- direct process invocation;
- child privilege escalation;
- inherited stdin/stdout/stderr;
- captured query output;
- timeout;
- cancellation;
- signal forwarding;
- exit-code preservation;
- sensitive argument redaction.

Never use `sh -c` or `bash -c`.


# 7. Backend Contract

Do not use a giant mandatory interface that forces all capabilities.

Use a capability-based contract. An object-safe Backend trait with default Unsupported methods is acceptable if every operation filters by declared capability first.

Core principle:

> Backend plans; central runner executes.

A practical shape may resemble:

```rust
trait Backend: Send + Sync {
    fn id(&self) -> BackendId;
    fn display_name(&self) -> &'static str;
    fn category(&self) -> BackendCategory;
    fn capabilities(&self) -> CapabilitySet;
    fn command_requirements(&self) -> &[CommandRequirement];

    fn probe(...);
    fn search(...);
    fn list_installed(...);
    fn info(...);
    fn plan_install(...);
    fn plan_remove(...);
    fn plan_update(...);
    fn plan_upgrade(...);
}
```

The exact API may improve, but generic operations must remain backend-agnostic.


# 8. Execution Plan

ExecutionPlan is a central immutable model containing:

- backend ID;
- backend display name;
- operation;
- human action description;
- optional package ID;
- absolute executable path;
- argument vector;
- working directory;
- environment changes;
- root requirement;
- interactivity;
- stdio mode;
- timeout policy;
- redacted display arguments;
- optional scope;
- optional source.

Example:

```text
backend: apt
operation: install
action: Install system package
program: /usr/bin/apt-get
args:
  - install
  - git
requires_root: true
interactive: true
```

`--dry-run` must still perform discovery, search, selection, and real plan construction. It must only skip execution.

Dry run is not the same as a native package-manager simulation.


# 9. Privilege and Security

Official usage:

```bash
allp update
```

Not:

```bash
sudo allp update
```

Allp elevates only the child process requiring root.

Centralize privilege policy. Do not hardcode sudo in backends.

Before elevating, validate the resolved executable enough to avoid elevating an attacker-controlled PATH binary. Consider:

- absolute path;
- executable type;
- ownership;
- user/group writability;
- trusted system directories.

Represent commands as executable + args. Never concatenate shell strings.

Reject package identifiers beginning with `-` when they could become options. Use native `--` terminators where supported.

Never automatically add `-y`, `--assumeyes`, or similar flags in v0.1.


# 10. Backend Semantics

## APT

Expected:

- Search
- Install
- Remove
- Update
- Upgrade
- List
- Info

Prefer stable machine-oriented commands such as apt-cache, apt-get, and dpkg-query where appropriate.

## Pacman

Expected:

- Search
- Install
- Remove
- Upgrade
- List
- Info

Do not run `pacman -Sy` alone.

Choose and document one safe policy:

- Update unsupported; Upgrade uses `pacman -Syu`; or
- Update and Upgrade both map to full `pacman -Syu`, with clear action labels.

## DNF / DNF5

Prefer DNF5 when both exist. Keep implementation differences inside the backend.

## Flatpak

Support application IDs, remotes, origin, branch, and user/system scope. Do not treat display names as IDs.

## Snap

Probe snapd usability, not only binary presence. Apply shared ranking because Snap search is broad. Do not assume `git-scm` equals APT `git`.


# 11. Generic Search Ranking

Implement ranking outside backends.

Backends normalize raw results. The generic ranking layer controls visibility and ordering.

Signals may include:

- exact package-ID match;
- exact display-name match;
- ID starts with query;
- token-boundary match;
- normalized hyphen/underscore match;
- description-only match;
- package-ID length penalty;
- development-library suffix penalty;
- vendor namespace penalty.

Ranking improves presentation; it is not a recommendation engine and must not claim equivalence.

Use:

```rust
enum MatchKind {
    Exact,
    Related,
    Fuzzy,
}
```

Default visibility:

```text
Exact: visible
Related: bounded per backend
Fuzzy: hidden unless --all
```

Create unit tests for `git` classification and for ambiguous names such as npm `code` versus Visual Studio Code.


# 12. Concurrency and Reliability

Use bounded parallelism for read-only operations:

- search;
- list;
- optional info lookup.

Do not spawn unbounded threads.

Mutating operations remain sequential in v0.1.

Apply per-backend timeouts and cancellation.

Ctrl+C must stop cleanly.

Multi-backend operations:

- continue after failure by default;
- show success/failure/skipped for every backend;
- return non-zero when any backend fails.


# 13. JSON Contract

Required:

```bash
allp detect --json
allp search git --json
allp list --json
allp info git --json
allp update --dry-run --json
allp upgrade --dry-run --json
```

Do not mix human logs with JSON stdout.

Prefer a versioned envelope:

```json
{
  "schema_version": 1,
  "command": "search",
  "complete": true,
  "results": [],
  "issues": []
}
```


# 14. Exit Codes

Centralize stable alpha exit codes:

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
```

Do not scatter numeric literals.


# 15. Error UX

Errors must explain recovery.

Example ambiguity:

```text
Multiple install sources were found for "git".

Use one of:
  allp install git --from apt
  allp install git-scm --from snap
```

Example unavailable backend:

```text
The requested backend "flatpak" is not available.

Run:
  allp detect --verbose
```

Example incomplete search:

```text
Search completed with incomplete coverage.

APT      Success
Snap     Timed out
Flatpak  Not detected

Allp will not auto-select a unique result because one eligible backend failed.
```


# 16. Testing Requirements

Before completion, run:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
```

All must pass.

Required unit tests:

- capability filtering;
- command rendering;
- package-ID validation;
- match classification;
- ranking;
- per-backend limits;
- total limits;
- deterministic sorting;
- incomplete-search uniqueness prevention;
- non-interactive ambiguity;
- exit-code mapping;
- plan construction;
- dry-run skip.

Required integration tests using fake executables in temporary PATH:

- fresh discovery each invocation;
- backend appears after PATH change;
- missing/unusable backend;
- exact search;
- broad APT search remains bounded;
- Snap related matches;
- install selection;
- remove ownership selection;
- update dry run;
- partial failure;
- JSON stdout purity;
- no shell execution.

Use backend-owned fixtures:

```text
tests/fixtures/apt/
tests/fixtures/pacman/
tests/fixtures/dnf/
tests/fixtures/flatpak/
tests/fixtures/snap/
```


# 17. Architecture Guardrails

Maintain or improve an architecture-boundary check that prevents backend names and executable strings inside generic operations.

Add a test proving a dummy backend can be registered without editing generic operations.

Avoid:

- giant Engine;
- `utils.rs` dumping grounds;
- service locators;
- global mutable registry;
- backend-specific `if` chains;
- detached parser folders;
- shell strings;
- duplicated prompt/rendering logic.


# 18. Documentation

Update or create:

- README.md
- README.fa.md
- ARCHITECTURE.md
- ROADMAP.md
- TODO.md
- CHANGELOG.md
- CONTRIBUTING.md
- SECURITY.md
- docs/ADDING_BACKEND.md
- docs/COMMANDS.md
- docs/BACKEND_CONTRACT.md
- docs/CAPABILITY_MATRIX.md
- docs/CLI_CONTRACT.md
- docs/JSON_SCHEMA.md
- docs/SECURITY_MODEL.md
- docs/NPM_BACKEND_PLAN.md

README must say:

> Allp is not another package manager.

Demo:

```bash
allp detect
allp search git
allp install git --dry-run
allp update --dry-run
```

The capability matrix must distinguish Stable, Experimental, Detection only, and Unsupported.


# 19. Roadmap

## v0.1

- APT
- Pacman
- DNF/DNF5
- Flatpak
- Snap
- detect/search/install/remove/update/upgrade/list/info
- command-first CLI
- bounded search UX
- Exact/Related/Fuzzy ranking
- dry run
- JSON
- stable alpha exit codes
- tests and documentation

## v0.2

- Zypper
- APK
- doctor
- diagnostics
- safe cleanup where supported

## v0.3

- Cargo
- npm
- pnpm
- Yarn
- pip
- pipx
- Composer
- Go

Development package managers need explicit scopes. Do not silently modify project lockfiles.

## Later

- export/import
- history/replay
- configuration
- external backend protocol
- TUI
- GUI
- API/SDK

Never promise universal undo.


# 20. Explicit Non-Goals for v0.1

Do not add:

- GUI
- TUI
- plugin marketplace
- telemetry
- recommendation engine
- automatic source preference
- background daemon
- universal alias database
- repository editor
- mirror manager
- Allp-owned package cache
- project dependency modification
- automatic confirmation flags
- universal undo
- shell-based execution


# 21. Known Current-Alpha Problems

Inspect and fix:

1. Search can print thousands of weak APT results.
2. Exact/Fuzzy classification is too coarse.
3. Install may discard meaningful related results when one exact result exists.
4. Source choice can be bypassed.
5. CLI options were modeled too globally.
6. Command order is inconsistent.
7. Output exposes raw enum names.
8. Update output lacks action descriptions.
9. Search output is not navigable.
10. Pager and limits are missing.
11. Some parsers may use fragile human-oriented output.
12. Previous compile issues included:
    - duplicate renderer method names;
    - unsized JSON serialization;
    - Pacman iterator borrowing;
    - partial moves in install/remove errors.
13. Verify those compile fixes remain present.

The architecture may be reusable, but current UX must not be treated as correct.


# 22. Implementation Order

## Phase 1 — Audit

- inspect modules;
- run build/tests;
- list failures;
- find boundary violations;
- find CLI inconsistencies.

## Phase 2 — Domain and Contracts

- match model;
- backend action metadata;
- execution plan;
- errors/exit codes;
- detection states.

## Phase 3 — CLI Refactor

- command-first syntax;
- command-specific options;
- help and examples;
- human labels.

## Phase 4 — Search UX

- ranking;
- limits;
- Exact/Related/Fuzzy;
- deterministic sorting;
- `--exact`;
- `--limit`;
- `--all`;
- paging;
- incomplete-search behavior.

## Phase 5 — Install and Remove

- source selection;
- non-interactive behavior;
- installed ownership;
- dry-run output.

## Phase 6 — Update and Upgrade

- action descriptions;
- sequential execution;
- summaries;
- partial failures.

## Phase 7 — Backend Hardening

- APT;
- Pacman;
- DNF/DNF5;
- Flatpak;
- Snap;
- probes;
- parsers;
- fixtures.

## Phase 8 — Tests and Documentation


# 23. Definition of Done

Build quality:

```text
cargo fmt: pass
cargo check: pass
cargo clippy -D warnings: pass
cargo test: pass
architecture check: pass
release build: pass
```

These commands must work:

```bash
allp detect
allp search git
allp search git --limit 5
allp search git --exact
allp install git --dry-run
allp install git --from apt --dry-run
allp remove git --dry-run
allp update --dry-run
allp upgrade --dry-run
allp list --from apt
allp info git --from apt
```

UX acceptance:

- `allp search git` does not print thousands of results;
- meaningful Snap alternatives remain visible;
- weak library matches are hidden by default;
- user can select source;
- dry run states zero commands executed;
- update explains backend actions;
- errors include recovery;
- JSON is valid and clean.

Modularity acceptance:

- a sample development backend can be added without changing generic operation modules.

Safety acceptance:

- no shell execution;
- child-only elevation;
- no automatic confirmation flags;
- no unsafe Pacman partial refresh;
- no silent recommendation;
- no false uniqueness after partial search failure.


# 24. Final Deliverables

When finished, provide:

1. Architecture changes.
2. UX changes.
3. Files changed.
4. Commands run.
5. Actual test results.
6. Remaining limitations.
7. Human test commands for Ubuntu.
8. Release-readiness judgment: not ready, alpha ready, or beta ready.
9. Never claim a test passed unless it was actually executed.


# 25. Product Philosophy

Every decision must preserve:

- Native package managers remain the source of truth.
- Native commands remain visible.
- The user retains final control.
- No hidden operations.
- No automatic recommendation.
- No fake capability parity.
- Unsupported operations are explicit.
- Small reliable support beats broad shallow support.
- Adding a backend must not require rewriting the app.
- CLI usability is part of correctness, not decoration.
