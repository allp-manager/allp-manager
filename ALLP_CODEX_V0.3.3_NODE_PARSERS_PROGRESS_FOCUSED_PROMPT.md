# Allp v0.3.3 — Focused Node Management, Accurate Backend Results, and Live Progress

## Critical instruction

Read this file completely before changing code.

Work directly in the current Allp repository. Do not create a replacement project. This is a narrowly scoped patch for version `0.3.3`.

Do not redesign or rewrite unrelated parts of the application. Preserve all currently working behavior, including:

- interactive search-scope selection;
- numbered and paged result selection;
- confirmation flow;
- sudo/root/original-user handling;
- dry-run behavior;
- direct native process execution;
- colored terminal UI;
- JSON output;
- software-identity resolution;
- Homebrew bootstrap;
- Python behavior;
- install/remove behavior;
- current CLI syntax and architecture boundaries.

Only implement these three areas:

1. Complete Node.js runtime and package-manager CLI version management.
2. Correct APT, Flatpak, and Snap result parsing and summaries.
3. Add visible live execution progress so long-running operations never look frozen.

The final version must remain:

```text
0.3.3
```

Do not create `0.3.4`.

---

# 1. Protect working code

Before editing:

1. Inspect `git status`.
2. Inspect the current diff.
3. Run the current test suite.
4. Record current failures.
5. Add focused regression tests.
6. Make the smallest possible changes.

Do not perform broad refactors. Do not rename unrelated public APIs. Do not change existing UX outside this specification.

---

# 2. Complete Node.js component model

The current status:

```text
npm global Up to date
```

only describes globally installed npm registry packages. It does not represent:

- Node.js runtime;
- all installed Node versions;
- active Node version;
- default Node version;
- npm CLI;
- pnpm CLI;
- Yarn CLI;
- Corepack;
- nvm;
- fnm;
- Volta;
- asdf;
- project dependencies;
- workspace dependencies.

Model these separately.

Suggested concepts:

```rust
enum NodeComponentKind {
    Runtime,
    RuntimeVersion,
    NpmCli,
    PnpmCli,
    YarnCli,
    Corepack,
    GlobalPackages,
    ProjectDependencies,
    WorkspaceDependencies,
}

enum NodeRuntimeOwner {
    OsPackageManager,
    Homebrew,
    Nvm,
    Fnm,
    Volta,
    Asdf,
    Manual,
    Unknown,
}
```

Exact names may differ, but preserve the distinctions.

Do not label the Node ecosystem fully Ready merely because `node` exists.

Expected component-level output:

```text
Node.js Ecosystem · Partial

Runtime
✔ Node.js v24.14.0
  Path: /home/user/.nvm/versions/node/v24.14.0/bin/node
  Owner: nvm
  Active: Yes
  Default: Yes

Package Manager CLIs
✔ npm v11.9.0
○ pnpm Not installed
○ Yarn Not installed
✔ Corepack v0.x

Package Targets
✔ npm global packages Up to date
○ npm project Not applicable · no package.json found
```

---

# 3. Detect Node runtime ownership

Before planning a Node runtime update or upgrade, determine how Node was installed.

Supported ownership detection:

- OS package manager;
- Homebrew;
- nvm;
- fnm;
- Volta;
- asdf;
- manual installation;
- unknown.

Use reliable signals such as:

- resolved executable path;
- package-manager ownership query;
- symlink target;
- version-manager metadata;
- original-user HOME;
- known version-manager directory layouts;
- relevant environment variables.

Do not infer ownership from the executable name alone.

## OS package manager

Confirm ownership using the relevant native package-manager query where possible. Do not assume every `/usr/bin/node` belongs to APT on every distribution.

## Homebrew

Recognize Brew-owned paths and metadata. Never run Brew as root.

## nvm

Do not rely only on `command -v nvm`; nvm is commonly a shell function. Inspect the original user's `NVM_DIR`, installed versions, aliases, active version, and default alias safely.

## fnm, Volta, and asdf

Use their native stable or structured inspection commands where possible.

## Manual or unknown

When ownership cannot be verified:

```text
⚠ Node.js ownership could not be determined.
Allp will not modify the runtime automatically.
```

Never guess an update command.

---

# 4. Original-user discovery under sudo

When Allp is invoked through sudo, Node discovery must use the original user's context where appropriate:

- `SUDO_USER`;
- `SUDO_UID`;
- `SUDO_GID`;
- original HOME;
- original PATH;
- original version-manager directories;
- project ownership.

Do not inspect only root's HOME and PATH.

User-owned Node operations must run as the original user. Do not create root-owned files in:

- nvm/fnm/Volta/asdf directories;
- npm cache;
- pnpm store;
- Yarn cache;
- project directories;
- workspace directories;
- manifests or lockfiles.

Preserve the existing centralized privilege system. Do not add backend-local sudo logic.

---

# 5. Node update semantics

Command:

```bash
allp update --from node
```

Meaning:

> Update the active Node runtime within its current major or selected channel, and update compatible Node ecosystem targets without silently crossing major versions.

Never update the Node runtime through `npm update`.

Required behavior by owner:

- OS package manager: delegate to the owning system backend.
- Homebrew: use the Brew backend in original-user context.
- nvm: preserve current major/LTS channel by default.
- fnm/Volta/asdf: use native supported behavior.
- manual/unknown: do not modify automatically.

Show the exact version transition before execution.

---

# 6. Node upgrade semantics

Command:

```bash
allp upgrade --from node
```

Meaning:

> Display newer LTS and Current channels and require explicit selection before changing major versions.

Example:

```text
Installed Node.js

Active
[1] v22.14.0 · LTS · managed by nvm

Available Targets
[2] Latest patch in v22
[3] Latest LTS
[4] Latest Current

Choose a target [2-4, 0 to cancel]:
```

Do not automatically choose the newest major. Major-version changes require explicit selection and final confirmation.

For supported version managers, inspect:

- every installed Node version;
- active version;
- default version;
- same-major update;
- newer LTS;
- Current release.

Do not claim online availability when the lookup failed.

---

# 7. Separate package-manager CLI targets

Model these independently:

- npm CLI;
- pnpm CLI;
- Yarn CLI;
- Corepack.

Do not confuse them with packages managed by those CLIs.

## npm

Separate:

- npm CLI version;
- npm global packages;
- npm project dependencies;
- npm workspace dependencies.

Do not treat `npm update --global` as npm CLI self-update. Detect whether npm is bundled with Node, OS-managed, Brew-managed, independently installed, or unknown.

## pnpm

Separate:

- pnpm CLI;
- global packages;
- project dependencies;
- workspace dependencies.

Detect Corepack-managed pnpm separately.

## Yarn

Detect:

- Yarn Classic;
- modern Yarn;
- Corepack-managed Yarn;
- npm-global Yarn;
- Brew/OS-managed Yarn;
- unknown ownership.

## Corepack

Display installed version and enabled state where reliably detectable. Do not alter Corepack state without explicit confirmation.

---

# 8. Project and workspace safety

Keep these as separate targets:

- npm project/workspace;
- pnpm project/workspace;
- Yarn project/workspace.

Do not silently modify:

- `package.json`;
- `package-lock.json`;
- `pnpm-lock.yaml`;
- `yarn.lock`;
- workspace manifests.

Show affected files before execution and preserve all existing confirmation rules.

---

# 9. Accurate APT result parsing

Fix only the parser and normalized outcome. Do not rewrite the APT execution engine.

Parse output such as:

```text
1 upgraded, 0 newly installed, 0 to remove and 3 not upgraded.
```

Also parse:

```text
The following upgrades have been deferred due to phasing:
  python3-software-properties
  software-properties-common
  software-properties-gtk
```

Produce normalized fields for:

- updated count;
- newly installed count;
- removed count;
- not-upgraded count;
- deferred count;
- deferred package names;
- deferred reason.

Add a deferred reason model containing at least:

```rust
enum DeferredReason {
    PhasedUpdate,
    HeldPackage,
    DependencyConstraint,
    Unknown,
}
```

Expected output:

```text
✔ APT Updated · 1 package
⚠ APT Deferred · 3 phased updates
```

Phased updates are not failures. Do not force phased updates. Show package names in verbose output.

---

# 10. Accurate Snap result parsing

Parse:

```text
All snaps up to date.
```

as:

```text
UpToDate
```

When Snap actually refreshes packages, classify as `Updated` and include a count when reliable.

Use `Completed` only when the command succeeded but the outcome cannot be interpreted safely.

---

# 11. Accurate Flatpak result parsing

Parse:

```text
Looking for updates…
Nothing to do.
```

as:

```text
UpToDate
```

Handle user and system scopes independently.

Examples:

```text
✔ Flatpak · User Up to date
✔ Flatpak · System Up to date
```

If updates occurred, classify as `Updated` with a reliable count where possible. Use `Completed` only as a fallback.

Do not merge user and system outcomes when they differ.

---

# 12. Required normalized summary

Use at least:

- Updated;
- UpToDate;
- Deferred;
- Completed;
- Failed.

For the supplied real output, the expected summary is:

```text
Upgrade Summary

✔ APT           Updated · 1 package
⚠ APT           Deferred · 3 phased updates
✔ Flatpak       Up to date
✔ Snap          Up to date
✔ npm global    Up to date
○ npm project   Not applicable
⚠ pip environment Protected

1 package updated
3 updates deferred
0 failed
```

Do not classify every successful backend as generic `Completed`.

---

# 13. Live execution progress

The current upgrade flow can remain visually silent for long periods, making users think Allp has frozen.

Fix this without hiding native output and without moving package-manager operations into detached background jobs.

Mutating child processes must remain:

- foreground;
- sequential;
- cancellable;
- connected to native input/output as required.

Do not run hidden background package-manager jobs.

---

# 14. Execution boundaries

Before every selected child operation, print a clear marker:

```text
● [1/3] APT upgrade started
  Action: Upgrade installed APT packages
  Started: 10:07:12
```

Then stream native output.

After completion:

```text
✔ [1/3] APT finished in 2m 31s
  Result: Updated · 1 package
```

Then start the next backend:

```text
● [2/3] Flatpak · User update started
```

Required boundaries apply to:

- APT;
- Flatpak user;
- Flatpak system;
- Snap;
- Node runtime;
- npm CLI;
- npm global/project;
- pnpm;
- Yarn;
- Corepack;
- every other selected execution plan.

Never print native lines such as `Nothing to do` or `All snaps up to date` without first identifying the backend that produced them.

---

# 15. Quiet-period heartbeat

When a child command produces no output for a noticeable period in an interactive TTY, show a low-noise heartbeat:

```text
ℹ APT is still running · 15s elapsed
ℹ APT is still running · 30s elapsed
```

Required behavior:

- first heartbeat after roughly 10–15 seconds of silence;
- repeat every 15–30 seconds;
- reset the quiet timer whenever native stdout/stderr appears;
- display elapsed time;
- print progress to stderr;
- stop immediately when the child exits;
- no animated spinner over native package-manager output;
- no excessive terminal repainting;
- no heartbeat in JSON output;
- no cursor control in non-TTY output;
- no heartbeat in dry run;
- respect existing quiet/verbose flags.

Do not prefix every native output line.

---

# 16. Stage-level progress

Show meaningful stages before native execution where work may take time.

Node example:

```text
● Inspecting installed Node versions
✔ Found 3 installed versions

● Checking latest LTS metadata
✔ Latest LTS: v24.14.0

● Building Node upgrade plan
✔ Plan ready
```

APT example:

```text
● Waiting for package-manager lock
✔ Lock available

● Starting APT upgrade
```

Flatpak example:

```text
● Checking Flatpak user updates
✔ No pending user updates
```

Snap example:

```text
● Checking pending Snap refreshes
✔ All snaps are up to date
```

Do not claim completion before the native operation actually exits.

---

# 17. Execution lifecycle architecture

Keep responsibilities separated.

The execution layer may publish lifecycle events similar to:

```rust
enum ExecutionEvent {
    Preparing,
    Started,
    NativeOutputObserved,
    Heartbeat,
    Finished,
    Failed,
    Cancelled,
}
```

Exact naming may differ.

Rules:

- execution coordinator manages timing and process lifecycle;
- renderer displays progress;
- backend parsers normalize results;
- backends do not implement independent spinner systems;
- native output streaming remains intact.

---

# 18. Output-mode rules

## Interactive TTY

Show operation start, heartbeat during quiet periods, finish, elapsed time, and normalized result.

## Non-TTY

Use stable line-oriented output. No cursor movement or animation.

## JSON

Do not print human progress to stdout. Do not corrupt final JSON. Any progress events must use a documented structured stream or stderr.

## Dry run

Show planned order only:

```text
Dry-run execution order
[1] APT
[2] Flatpak · User
[3] Flatpak · System
[4] Snap
```

Execute nothing and show no heartbeat.

---

# 19. Failure and cancellation progress

On failure:

```text
✖ [2/4] Flatpak · System failed after 24s
  Exit code: 1
```

On cancellation:

```text
○ [2/4] Flatpak · System cancelled after 8s
```

Preserve the current batch continuation/fail-fast policy. Do not change policy defaults in this task.

---

# 20. Focused tests

## Node tests

Use fake environments for:

- OS-package-manager-owned Node;
- Homebrew-owned Node;
- nvm;
- fnm;
- Volta;
- asdf;
- manual installation;
- unknown ownership;
- multiple installed versions;
- active/default version;
- same-major update;
- newer LTS and Current;
- original-user discovery under sudo;
- root HOME not used for user-owned Node;
- npm CLI separate from npm global packages;
- pnpm CLI separate from package targets;
- Yarn version/ownership detection;
- Corepack detection;
- project/workspace detection;
- no runtime update through npm;
- no automatic major upgrade;
- unknown ownership blocks runtime mutation.

Do not modify the developer's real Node installation.

## Parser fixtures

APT fixture must assert:

- `Updated`;
- updated count 1;
- deferred count 3;
- package names;
- `PhasedUpdate`;
- zero failure.

Snap fixture:

```text
All snaps up to date.
```

must assert `UpToDate`.

Flatpak fixture:

```text
Looking for updates…
Nothing to do.
```

must assert `UpToDate`.

## Progress tests

Add tests for:

- start marker before native output;
- finish marker after child exit;
- elapsed-time formatting;
- heartbeat after quiet interval;
- heartbeat reset after output;
- heartbeat stops after completion;
- no spinner over native output;
- no progress ANSI in JSON;
- no cursor control in non-TTY;
- no heartbeat in dry run;
- sequential operation numbering;
- backend name visible before native output;
- failure and cancellation boundaries;
- native output remains available.

Use a fake clock where possible. Do not add slow real-time sleeps to tests.

---

# 21. Validation commands

Run:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo check --all-targets
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
bash scripts/check-architecture.sh
cargo build --release
```

Use only non-destructive CLI validation:

```bash
allp update --dry-run
allp upgrade --dry-run
allp update --from node --dry-run
allp upgrade --from node --dry-run
allp update --verbose --dry-run
allp upgrade --verbose --dry-run
```

Use temporary fake environments for real lifecycle tests.

Do not:

- upgrade the developer's real Node runtime;
- modify real npm/pnpm/Yarn projects;
- change real lockfiles;
- perform real system package mutations;
- alter unrelated files or behavior.

---

# 22. Final report

Report only changes related to this focused patch:

1. Node runtime ownership model.
2. Installed/active/default Node version discovery.
3. Node update versus upgrade behavior.
4. npm, pnpm, Yarn, and Corepack CLI separation.
5. Project/workspace behavior.
6. Original-user discovery under sudo.
7. APT parser changes.
8. Flatpak parser changes.
9. Snap parser changes.
10. Deferred-update representation.
11. Execution start/finish boundaries.
12. Quiet-period heartbeat.
13. Exact files changed.
14. Commands actually run.
15. Actual test results.
16. Remaining limitations.
17. Confirmation that unrelated features were not changed.

Do not claim completion if:

- Node runtime is still represented only by npm global status;
- unknown Node ownership is modified automatically;
- Node major versions can change silently;
- APT phased updates still appear as generic Completed;
- Snap `All snaps up to date` still appears as Completed;
- Flatpak `Nothing to do` still appears as Completed;
- long-running operations still show no execution boundary or heartbeat;
- unrelated working features were changed or regressed.
