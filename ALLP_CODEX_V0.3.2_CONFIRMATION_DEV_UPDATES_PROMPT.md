# Allp v0.3.2 — Mandatory Confirmation and Complete Python/Node Updates

Read this file together with all previous Allp specifications:

- `ALLP_CODEX_MASTER_PROMPT.md`
- `ALLP_CODEX_UX_HARDENING_PROMPT.md`
- `ALLP_CODEX_EXPANSION_PRIVILEGE_UI_PROMPT.md`
- `ALLP_CODEX_INTERACTIVE_SCOPE_SELECTOR_PROMPT.md`

Work directly in the existing Allp repository. Do not create a replacement project.

This specification defines **Allp v0.3.2**.

The current implementation has two release-blocking problems:

1. Mutating operations may continue without asking for final confirmation.
2. Python and Node backends are detected but skipped during `update` and `upgrade`.

Fix both problems across the whole application, not inside only one backend.

---

# 1. Version

Set the project version to:

```text
0.3.2
```

Update:

- `Cargo.toml`
- lockfile where applicable
- `CHANGELOG.md`
- README version references
- `allp --version`
- generated release metadata

Release title:

```text
Allp v0.3.2 — Confirmed Operations and Developer Ecosystem Updates
```

---

# 2. Mandatory Final Confirmation

Every real mutating operation must require an Allp-level final confirmation before execution.

This includes:

- install
- remove
- update
- upgrade
- project dependency changes
- global tool changes
- lockfile changes
- environment changes

The rule still applies when:

- exactly one package was found;
- the match is Exact;
- `--from` was supplied;
- only one backend is selected;
- sudo is not required;
- Allp is already running as root;
- the native package manager has its own prompt.

One exact result is not permission to install.

---

# 3. Single Exact Match UX

Example:

```bash
allp install git --from apt
```

Required behavior:

```text
✔ One exact match found

APT · git
Version: 1:2.53.0-1ubuntu1
Type: System package
Source: APT repositories

Execution Plan
Action: Install system package
Command: sudo -- /usr/bin/apt-get install -- git
Privilege: Administrator access required

Install this package? [Y/n]
```

A shorter friendly variant is acceptable:

```text
✔ Found one exact package:
  APT · git · 1:2.53.0-1ubuntu1

Install it? [Y/n]
```

Do not execute immediately after showing the result.

Accepted confirmation:

```text
y
yes
Enter
```

Cancellation:

```text
n
no
q
Esc
Ctrl+C
```

Cancellation output:

```text
ℹ Installation cancelled
0 commands executed
```

---

# 4. Multiple Results

When several results exist:

1. Show paged numbered results.
2. Require explicit package selection.
3. Resolve installer choice when needed.
4. Resolve target/scope when needed.
5. Build the immutable Execution Plan.
6. Show the native command.
7. Ask for final confirmation.
8. Execute only after confirmation.

Selecting a numbered result is not final execution permission.

Example:

```text
Selected Package
Python · PyPI · black

Installer: pipx
Scope: Original user
Command: /usr/bin/pipx install black

Install this package? [Y/n]
```

---

# 5. Remove Confirmation

Removal must default to No.

```text
Package to Remove

APT · git
Installed version: 1:2.53.0-1ubuntu1

Execution Plan
Command: sudo -- /usr/bin/apt-get remove -- git
Privilege: Administrator access required

⚠ This operation will remove an installed package.

Remove it? [y/N]
```

---

# 6. Update and Upgrade Batch Confirmation

`update` and `upgrade` must show all selected plans before execution.

Example:

```bash
allp update
```

```text
Allp Update

Environment Scan
Detected and ready: APT, Snap, npm, pipx
Selected for update: APT, Snap, npm, pipx

Planned Operations

[1] APT
    Action: Refresh package metadata
    Command: sudo -- /usr/bin/apt-get update
    Privilege: Administrator access required

[2] Snap
    Action: Refresh installed snaps
    Command: sudo -- /usr/bin/snap refresh
    Privilege: Administrator access required

[3] npm
    Target: Global user packages
    Action: Update packages within allowed version ranges
    Command: npm update --global
    Privilege: Original user

[4] pipx
    Target: Isolated Python tools
    Action: Upgrade all installed pipx tools
    Command: pipx upgrade-all
    Privilege: Original user

⚠ 4 operations will be executed.
Continue? [Y/n]
```

No child command may start before the batch confirmation.

For upgrade:

```text
⚠ Upgrade may install newer major versions, modify manifests,
  update lockfiles, or change application behavior.

Continue with upgrade? [y/N]
```

Upgrade defaults to No whenever it can cross constraints or change project files.

---

# 7. Confirmation and sudo Are Separate

There are two separate decisions:

1. Did the user authorize the operation?
2. Does the selected child command require elevation?

Correct normal-user flow:

```text
Install this package? [Y/n]
```

After Yes:

```text
⚠ Administrator access is required.
Only the selected native child command will be elevated.
You may now be asked for your sudo password.
```

Then invoke sudo.

Do not trigger sudo before:

- discovery;
- search;
- package selection;
- installer selection;
- target selection;
- plan rendering;
- final confirmation.

---

# 8. Already Running as Root

Example:

```bash
sudo allp install git
```

Allp must:

- detect effective UID 0;
- never ask permission to use sudo;
- never prepend nested sudo;
- never say it is running as a normal user;
- still ask for final operation confirmation;
- display root context once.

```text
⚠ Allp is running with administrator privileges.
System operations will run directly as root.
User-scoped operations will use the original user when available.
```

Then:

```text
Execution Plan
Command: /usr/bin/apt-get install -- git
Privilege: Already running as administrator

Install this package? [Y/n]
```

Final confirmation remains mandatory under root.

---

# 9. Explicit Automation Flag

Add:

```bash
--yes
-y
```

Rules:

- bypass only Allp's own final confirmation;
- never add native `-y`, `--assumeyes`, or equivalent flags;
- never bypass package ambiguity;
- never bypass installer/target selection;
- never auto-install Fuzzy Python or Node results;
- never bypass Homebrew root protection;
- never bypass PEP 668 protection;
- never bypass unsafe project ownership checks.

Examples:

```bash
allp install git --from apt --yes
allp update --from npm --target global --yes
```

In non-TTY mode, a mutating command requires fully resolved choices plus `--yes`.

Without `--yes`:

```text
Confirmation is required, but no interactive terminal is available.

Review with:
  allp update --dry-run

Execute explicitly with:
  allp update --yes
```

---

# 10. Dry Run

`--dry-run` must:

- discover;
- inspect updates;
- select/resolve targets when supplied;
- build real plans;
- show exact commands;
- show privilege behavior;
- show affected files;
- execute nothing;
- ask for no password;
- invoke no sudo;
- ask no final execution confirmation.

End with:

```text
✔ Dry run completed
4 operations planned
0 commands executed
0 privilege prompts triggered
```

---

# 11. Python and Node Are First-Class Update Backends

Python and Node must participate in:

```bash
allp update
allp upgrade
```

If detected and eligible, each target must either:

1. produce an actionable plan; or
2. report a precise reason why it cannot.

Never output only:

```text
Python  Skipped
Node    Skipped
```

Valid detailed output:

```text
○ pip
  Skipped: no active virtual environment and system Python is externally managed

○ npm project
  Skipped: no package.json found in the current directory

○ npm global
  Skipped: no globally installed packages were found

○ pipx
  Skipped: pipx is not installed

○ uv tools
  Skipped: no uv-managed tools were found
```

Do not use Skip to hide an unimplemented capability claimed by v0.3.2.

---

# 12. Developer Update Targets

Add a generic target model.

Suggested domain concept:

```rust
enum DeveloperTarget {
    Project,
    Workspace,
    GlobalTools,
    ActiveEnvironment,
    IsolatedTools,
}
```

Add:

```bash
--target <target>
```

Supported values:

```text
project
workspace
global
environment
tools
all
```

Examples:

```bash
allp update --from npm --target project
allp update --from npm --target global
allp update --from pnpm --target workspace
allp update --from pip --target environment
allp update --from pipx --target tools
allp update --from python --target all
allp update --scope dev --target all
```

When multiple targets are detected and no target was supplied:

```text
Select Update Targets

[1] Node project · npm · /home/user/project
[2] Global Node tools · npm
[3] Python virtual environment · /home/user/project/.venv
[4] Python isolated tools · pipx
[5] Python isolated tools · uv
[6] All detected targets

Choose [1-6, 0 to cancel]:
```

---

# 13. Update vs Upgrade

## Update

Apply compatible updates while respecting declared constraints whenever the native ecosystem supports that distinction.

Expected behavior:

- safer;
- generally respects existing ranges;
- may update lockfiles;
- must explain native semantics.

## Upgrade

Move selected packages or tools toward latest available versions, potentially crossing constraints or modifying manifests/lockfiles.

Expected behavior:

- stronger warning;
- may cross major versions;
- may modify project files;
- defaults to No when risky.

If a backend maps Update and Upgrade to the same native command:

- support both;
- explain the mapping;
- do not Skip merely because they are identical.

---

# 14. Node Ecosystem

Node participants:

- npm registry as source;
- npm installer;
- pnpm installer;
- Yarn installer.

Do not generate:

```bash
npx update
```

`npx` is a package-command executor, not the standard npm package update operation.

Use the real native command of the selected installer.

---

# 15. npm

## Project inspection

Requirements:

- valid `package.json`;
- npm detected;
- valid project root.

Inspect:

```bash
npm outdated --json
```

## Project Update

```bash
npm update
```

Show possible changes:

```text
node_modules
package-lock.json
```

## Global inspection

```bash
npm outdated --global --depth=0 --json
```

## Global Update

```bash
npm update --global
```

Run as original user, not root, unless the installation truly belongs to root.

## Project Upgrade

For latest versions beyond current ranges:

1. inspect outdated packages;
2. show current/wanted/latest;
3. let the user select packages or all;
4. build explicit native npm arguments;
5. preserve dependency type;
6. show `package.json` and `package-lock.json` effects;
7. require strong confirmation.

Do not automatically install or invoke an external updater.

Do not use `npx npm-check-updates` unless explicitly requested and already installed.

## npm Self Update

Updating the npm CLI itself is a separate operation and is not the meaning of:

```bash
allp update --from npm
```

---

# 16. pnpm

## Project Update

```bash
pnpm update
```

## Project Upgrade

```bash
pnpm update --latest
```

## Global Update

```bash
pnpm update --global
```

## Global Upgrade

```bash
pnpm update --global --latest
```

## Workspace

When a pnpm workspace is detected, ask whether to update:

- current package;
- selected workspace package;
- all workspace packages.

Do not silently run recursive workspace updates.

Show affected manifests and `pnpm-lock.yaml`.

Run as original user.

---

# 17. Yarn

Detect Yarn major version before planning.

## Yarn 1 / Classic

Compatible update:

```bash
yarn upgrade
```

Latest upgrade:

```bash
yarn upgrade --latest
```

Show that `yarn.lock` may change.

## Modern Yarn / Berry

Do not reuse Yarn 1 commands blindly.

Use version-appropriate commands and official CLI behavior.

Requirements:

- detect project and Yarn version;
- inspect selected packages;
- use version-correct `yarn up` behavior;
- distinguish re-resolution from latest upgrades where supported;
- show affected workspaces/manifests/lockfiles;
- require explicit project-wide confirmation.

If one sub-capability cannot be implemented safely, report that exact sub-capability as unsupported rather than skipping all Node support.

---

# 18. Python Ecosystem

Python participants:

- pip
- pipx
- uv tools
- optional safe uv project support

Rules:

- run in correct user/environment context;
- never default to sudo;
- never modify externally managed system Python automatically;
- never create root-owned virtualenv/tool files for the original user.

---

# 19. pip

pip has no single native bulk `update-all` command.

Use a safe inspect/select/plan flow.

## Environment detection

Detect:

- active virtual environment;
- selected interpreter;
- user environment;
- externally managed system Python.

## Inspect outdated

```bash
python -m pip list --outdated --format=json
```

## Select packages

```text
Outdated Python Packages

[1] requests    2.31.0 → 2.32.4
[2] rich        13.7.0 → 14.1.0
[3] httpx       0.27.0 → 0.28.1

[a] Select all
[0] Cancel
```

Do not automatically upgrade all pip packages.

## Execute selected packages

```bash
python -m pip install --upgrade requests rich httpx
```

Validate package IDs before constructing arguments.

For arbitrary installed pip environments, Update and Upgrade may map to the same native `pip install --upgrade` operation.

Explain:

```text
ℹ pip uses the same native upgrade operation for Update and Upgrade
  in the selected environment.
```

## PEP 668

When externally managed:

- never add `--break-system-packages` automatically;
- never use sudo;
- offer safe alternatives:
  - virtual environment;
  - pipx;
  - uv tool;
  - explicit isolated environment.

---

# 20. pipx

Detect installed pipx tools.

Upgrade all:

```bash
pipx upgrade-all
```

Upgrade one:

```bash
pipx upgrade <package>
```

Respect native pinning.

Example skip:

```text
○ black
  Skipped: pinned in pipx
```

Run as original user.

---

# 21. uv Tools

Detect uv-managed tools.

Upgrade all:

```bash
uv tool upgrade --all
```

Upgrade selected:

```bash
uv tool upgrade <name>
```

Respect recorded constraints.

If Update and Upgrade map to the same command for uv tools, state that clearly rather than skipping.

Run as original user.

Do not mix uv tool updates with uv project lockfile changes.

---

# 22. Python Project Updates

Project dependencies are distinct from environment packages.

If uv project support is implemented:

- detect `pyproject.toml`;
- detect `uv.lock`;
- show project root;
- show affected files;
- require `--target project` or interactive target selection;
- require strong confirmation;
- preserve original-user ownership.

Never silently modify:

```text
pyproject.toml
uv.lock
requirements.txt
poetry.lock
Pipfile.lock
```

Unsupported project managers must be listed explicitly, not silently skipped.

---

# 23. Default `allp update`

When the user runs:

```bash
allp update
```

Inspect all detected update-capable targets.

Example:

```text
Update Targets Found

System and Applications
[1] APT metadata
[2] Snap applications

Node.js
[3] npm project · /home/user/project
[4] npm global tools

Python
[5] active virtual environment · /home/user/project/.venv
[6] pipx tools
[7] uv tools

[8] All targets
[0] Cancel
```

Do not invent project targets when no project exists.

After target selection, show every plan and ask once for final confirmation.

---

# 24. Default `allp upgrade`

Use the same target discovery, but show stronger risk information.

```text
⚠ Upgrade can cross version ranges and modify project files.

Detected Upgrade Targets
[1] npm project
[2] pnpm global tools
[3] pip virtual environment
[4] pipx tools
[5] uv tools
[6] All targets
```

Show affected files and commands before confirmation.

---

# 25. Skip Policy

Skip is valid only when the operation truly cannot or should not run.

Valid reasons:

```text
backend not installed
no installed packages
no project manifest
no active Python environment
externally managed Python environment
package pinned
capability unsupported by detected version
original user cannot be recovered safely
```

Invalid output:

```text
Skipped: Node
Skipped: Python
Skipped: development backend
```

Every Skip must include:

- backend or target;
- exact reason;
- recovery suggestion when useful.

---

# 26. Summary UX

```text
Update Summary

✔ APT metadata            Completed
✔ Snap applications       Completed
✔ npm global tools        Completed
✔ pipx tools              Completed
○ npm project             Not selected
○ pip environment         No active virtual environment

4 completed
0 failed
2 not run
```

Partial failure:

```text
Update Summary

✔ APT metadata            Completed
✖ npm project             Failed · native command exited with status 1
✔ pipx tools              Completed
○ pip environment         Skipped · externally managed

2 completed
1 failed
1 skipped
```

Use the established color/icon policy.

---

# 27. TTY, Non-TTY, and JSON

## Interactive TTY

- select targets;
- select packages;
- show plans;
- ask final confirmation;
- then elevate/execute.

## Non-TTY

No prompts.

Require:

- exact target;
- resolved package choices;
- `--yes`.

## JSON

Include structured fields:

```json
{
  "requires_confirmation": true,
  "confirmation_bypassed": false,
  "targets": [],
  "plans": [],
  "results": [],
  "skips": []
}
```

No prompts or ANSI in JSON.

---

# 28. Architecture

Centralize confirmation.

Suggested concepts:

```rust
enum ConfirmationPolicy {
    RequiredDefaultYes,
    RequiredDefaultNo,
    NotRequiredDryRun,
    BypassedByExplicitYes,
}

struct ConfirmationRequest {
    operation: OperationKind,
    risk: RiskLevel,
    plans: Vec<ExecutionPlanSummary>,
    default_answer: bool,
}
```

Add capabilities:

```rust
Capability::InspectUpdates
Capability::Update
Capability::Upgrade
```

Suggested target models:

```rust
struct UpdateTarget {
    backend_id: BackendId,
    ecosystem: PackageDomain,
    target_kind: DeveloperTarget,
    location: Option<PathBuf>,
    installed_count: Option<usize>,
}

struct OutdatedPackage {
    package_id: String,
    current_version: String,
    wanted_version: Option<String>,
    latest_version: String,
    target: UpdateTargetId,
}
```

Generic operations must not contain npm, pnpm, Yarn, pip, pipx, or uv command strings.

Those belong to backend implementations.

---

# 29. Required Tests

## Confirmation

- one exact install asks;
- `--from` does not bypass confirmation;
- remove defaults to No;
- update asks once for the batch;
- upgrade defaults to No when risky;
- root invocation still asks operation confirmation;
- root invocation never asks for sudo;
- normal user confirms before sudo;
- dry run asks no execution confirmation;
- `--yes` bypasses only Allp confirmation;
- `--yes` does not add native auto-confirm flags;
- non-TTY without `--yes` fails;
- selection and confirmation remain separate.

## Node

- npm project detection;
- npm global detection;
- npm outdated parsing;
- npm project update plan;
- npm global update plan;
- no `npx update` plan;
- pnpm project/global plans;
- pnpm latest upgrade plans;
- Yarn major version detection;
- Yarn 1 mapping;
- modern Yarn mapping;
- lockfile effect reporting;
- original-user execution under sudo;
- project ownership protection.

## Python

- active venv detection;
- externally managed handling;
- pip outdated JSON parsing;
- selected pip upgrade plan;
- no automatic bulk pip upgrade;
- pipx upgrade-all;
- pinned pipx reporting;
- uv tool upgrade all;
- uv selected-tool upgrade;
- original-user execution under sudo;
- no sudo for user Python tools.

## Aggregation

- Python targets appear in `allp update`;
- Node targets appear in `allp update`;
- Python targets appear in `allp upgrade`;
- Node targets appear in `allp upgrade`;
- implemented targets are not silently skipped;
- Skip reasons are explicit;
- mixed root/user plans execute correctly;
- summaries are accurate;
- partial failures return correct exit codes.

---

# 30. Documentation

Update:

- `README.md`
- `README.fa.md`
- `CHANGELOG.md`
- `ROADMAP.md`
- `docs/COMMANDS.md`
- `docs/CLI_CONTRACT.md`
- `docs/PRIVILEGE_MODEL.md`
- `docs/PYTHON_ECOSYSTEM.md`
- `docs/NODE_ECOSYSTEM.md`
- `docs/TERMINAL_UI.md`
- `docs/CAPABILITY_MATRIX.md`

Add:

- `docs/CONFIRMATION_MODEL.md`
- `docs/DEVELOPER_UPDATES.md`
- `docs/V0_3_2_TEST_PLAN.md`

Document that Allp must use native commands such as:

```text
npm update
pnpm update
Yarn version-appropriate update commands
python -m pip list --outdated
python -m pip install --upgrade ...
pipx upgrade-all
uv tool upgrade --all
```

Do not document or generate `npx update` as npm update behavior.

---

# 31. Quality Gate

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

Use temporary projects and fake executables for mutation tests.

Do not perform destructive installs, removals, project updates, or upgrades on the developer's primary machine/projects without explicit permission.

---

# 32. Manual Non-Destructive Matrix

```bash
allp install git --from apt --dry-run
allp install git --from apt
allp install git --from apt --yes

sudo allp install git --from apt --dry-run

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

Verify:

- no real mutation in dry run;
- every real mutation asks;
- no nested sudo;
- Python/Node are not silently skipped;
- no `npx update` command is generated.

---

# 33. Final Report

Report:

1. Confirmation architecture.
2. Single-result confirmation behavior.
3. Batch confirmation behavior.
4. Root/sudo behavior.
5. `--yes` behavior.
6. npm implementation.
7. pnpm implementation.
8. Yarn version-specific implementation.
9. pip inspect/select/upgrade behavior.
10. pipx behavior.
11. uv tool behavior.
12. Python/Node target discovery.
13. Every remaining Skip reason.
14. Files changed.
15. Commands run.
16. Actual test results.
17. Remaining limitations.
18. Public-alpha readiness for v0.3.2.

Do not claim a test passed unless it actually ran.
Do not call v0.3.2 ready while Python or Node update/upgrade still appear as generic silent skips.
