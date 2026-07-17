# Allp — Regression Repair Prompt

## Purpose

Read this file completely before changing any code.

Work directly in the current Allp repository. Do not create a replacement project.

This is a focused regression-repair task. Recent changes added useful confirmation, privilege, developer-ecosystem, and interactive-selection behavior, but they also introduced or exposed regressions in install, update, and upgrade flows.

Do not redesign the whole project again.

Preserve working behavior and repair only the broken contracts described here.

Do not change the public version number unless the repository already requires a release bump or the user explicitly asks for one.

---

# 1. Critical Instruction: Protect Working Features

Before editing:

1. Inspect the current Git status.
2. Inspect the current diff.
3. Run the existing test suite.
4. Record the currently failing tests and reproducible CLI failures.
5. Identify the smallest set of modules responsible.
6. Do not rewrite unrelated backend, renderer, selector, discovery, or execution code.

The following behavior already works and must not regress:

- interactive search-scope selection;
- grouped search results;
- stable numbered result selection;
- Exact/Related result classification;
- final installation confirmation;
- root-context detection;
- no nested sudo when Allp already runs as root;
- command-first CLI syntax;
- direct native command execution without `sh -c`;
- dry-run execution planning;
- APT, Flatpak, and Snap discovery;
- colored status output;
- JSON remaining free of ANSI and prompts.

Add regression tests before or alongside fixes.

---

# 2. Current Reproduced Problems

## Problem A — Incorrect Update Summary States

Current output includes lines like:

```text
⚠ pip environment Skipped · Skipped: no active Python environment; refusing to modify system Python
⚠ pipx tools Skipped · Skipped: backend not installed
⚠ npm project Skipped · Skipped: no project manifest found
⚠ npm global Skipped · Skipped: no globally installed outdated npm packages found
```

Problems:

- duplicated `Skipped · Skipped`;
- unrelated conditions collapsed into one state;
- missing tools shown as warnings;
- an up-to-date target shown as skipped;
- summary counts only completed and failed;
- unavailable tools make normal output noisy.

## Problem B — Incorrect Upgrade Target Selection

Current output may say:

```text
Selected for upgrade:
APT, Flatpak, Snap, Yarn, npm global, npm project,
pip environment, pipx tools, pnpm, uv tools
```

even when:

- Yarn is not installed;
- pnpm is not installed;
- pipx is not installed;
- uv is not installed;
- no npm project manifest exists;
- no active Python environment exists;
- npm global packages are already up to date.

Unavailable, protected, and not-applicable targets must not be described as selected execution plans.

## Problem C — Cancellation UX Is Misleading

When the user presses Enter at:

```text
Continue with upgrade? [y/N]:
```

the default answer is No, so cancellation is expected.

However, the current program prints one skipped line for every target:

```text
APT      Skipped · cancelled by user before execution
Flatpak Skipped · cancelled by user before execution
Snap     Skipped · cancelled by user before execution
```

This makes a clean batch cancellation look like backend failures or skipped operations.

## Problem D — APT Installation Does Not Handle Busy Locks

Reproduction:

```bash
sudo allp install git
```

After selecting APT and confirming, APT fails:

```text
E: Could not get lock /var/lib/dpkg/lock-frontend.
It is held by process 7515 (packagekitd)
E: Unable to acquire the dpkg frontend lock
```

Allp currently reports only:

```text
APT command failed with exit code 100
```

The operating system lock is real, but Allp should classify and handle it as a busy package-management state, not as a generic installation failure.

## Problem E — Installing an Already Installed Package

The selected APT package may already be installed.

Allp should inspect installed state before planning a normal install.

It should not blindly run:

```bash
apt-get install -- git
```

when the package is already installed and no reinstall or upgrade intent was requested.

---

# 3. Required Normalized Result-State Model

Introduce or repair a shared result-state model.

Use distinct normalized states:

```rust
enum OperationStatus {
    Updated,
    UpToDate,
    Completed,
    Available,
    Selected,
    AlreadyInstalled,
    NotApplicable,
    NotSelected,
    Unavailable,
    Protected,
    Busy,
    Cancelled,
    Skipped,
    Failed,
}
```

Exact naming may vary, but these distinctions must exist.

## Required mappings

```text
No outdated npm global packages
→ UpToDate

No package.json found
→ NotApplicable

Backend executable not installed
→ Unavailable

No active Python environment and system Python is protected
→ Protected

Package already installed
→ AlreadyInstalled

Package-manager lock held by another process
→ Busy

User rejected the whole batch
→ Cancelled

User did not choose a valid available target
→ NotSelected

Native operation changed packages
→ Updated

Native operation succeeded and change state is unknown
→ Completed

Native command failed for a real error
→ Failed
```

Do not generate:

```text
Skipped · Skipped: ...
```

Renderer output should add the label exactly once.

---

# 4. Status Icons and Colors

Use consistent semantics:

```text
✔ Updated / UpToDate / Completed / AlreadyInstalled
  Green

✖ Failed
  Red

⚠ Protected / Busy / security warning
  Amber or yellow

○ NotApplicable / NotSelected / Unavailable / Cancelled
  Dim neutral

ℹ Explanatory information
  Cyan or blue
```

Do not use an amber warning icon merely because an optional backend is not installed.

Normal output should hide unavailable optional tools unless:

```bash
--verbose
```

is active.

JSON must still contain every normalized status.

---

# 5. Correct Target-Discovery Pipeline

For `update` and `upgrade`, maintain separate sets:

```text
1. Detected runtimes and ecosystems
2. Detected backend components
3. Available operation targets
4. Unavailable or not-applicable targets
5. Targets selected by the user
6. Immutable execution plans
```

Do not merge these concepts.

Required flow:

```text
fresh discovery
→ component readiness
→ target discovery
→ applicability checks
→ outdated-state inspection where supported
→ selectable target list
→ user selection
→ plan construction
→ final confirmation
→ execution
→ normalized summary
```

Only actionable plans may be shown under:

```text
Selected for execution
```

---

# 6. Component-Based Ecosystem Readiness

Do not mark an ecosystem fully Ready because its runtime exists.

Example:

```text
Python Ecosystem · Partial

✔ Python runtime   Ready
○ pip              Unavailable
○ pipx             Unavailable
○ uv               Unavailable
```

Example:

```text
Node.js Ecosystem · Partial

✔ Node.js runtime  Ready
✔ npm              Ready
○ pnpm             Unavailable
○ Yarn             Unavailable
```

A runtime is not an installer.

An installer is not automatically an applicable update target.

A backend is not selected merely because it was detected.

---

# 7. Update and Upgrade Summary UX

Group normal output:

```text
System and Applications
Developer Ecosystems
```

Example:

```text
Update Summary

System and Applications

✔ APT            Up to date
✔ Flatpak        Up to date
✔ Snap           Up to date

Developer Ecosystems

✔ npm global     Up to date
○ npm project    Not applicable · no package.json found
⚠ pip environment
                 Protected · no active virtual environment;
                 system Python was not modified

Optional unavailable tools are hidden.
Use --verbose to show pipx, uv, pnpm, and Yarn.

4 targets checked
0 updated
4 up to date
1 protected
0 failed
```

Verbose output may add:

```text
Optional Targets

○ pipx    Unavailable · pipx is not installed
○ uv      Unavailable · uv is not installed
○ pnpm    Unavailable · pnpm is not installed
○ Yarn    Unavailable · Yarn is not installed
```

Summary counters must account for every normalized state.

Do not report only:

```text
3 completed
0 failed
```

when several other target states exist.

---

# 8. Correct Cancellation Behavior

Upgrade confirmation may remain default No:

```text
Upgrade 3 selected targets? [y/N]
Press y to continue; Enter cancels.
```

When the user presses Enter, types `n`, presses Escape, or interrupts before execution:

```text
ℹ Upgrade cancelled by user
0 commands executed
```

Do not render one Cancelled/Skipped line for every plan in normal output.

Verbose mode may show per-target cancellation details.

Cancellation must not be represented as:

- failure;
- backend skip;
- partial failure.

No child command may execute after cancellation.

---

# 9. Correct sudo and Root Behavior

When invoked as a normal user:

```bash
allp install git
```

- show the execution plan;
- ask for final operation confirmation;
- explain root requirement;
- elevate only the root-required child;
- do not elevate user-scoped operations.

When invoked as root through sudo:

```bash
sudo allp install git
```

- never ask whether sudo may be used;
- never add nested sudo;
- never claim Allp is running as a normal user;
- still require final operation confirmation;
- run root-required system operations directly;
- preserve original-user context for user-scoped operations.

This behavior currently appears mostly correct and must not regress.

---

# 10. Flatpak Under sudo

The current label:

```text
Privilege: Current user
```

is ambiguous when Allp itself is running as root through sudo.

For a user-scoped Flatpak target:

```text
Scope: User
Run as: <SUDO_USER>
HOME: original user's home
Command: /usr/bin/flatpak update --user
```

For a system-scoped Flatpak target:

```text
Scope: System
Run as: Root
Command: /usr/bin/flatpak update --system
```

Do not silently run user-scoped Flatpak operations with root HOME.

Do not use the phrase `Current user` when the effective user and original user differ.

Add tests verifying de-escalation.

---

# 11. APT Installed-State Preflight

Before building an APT install plan, inspect whether the exact package is already installed.

Use a stable native query such as `dpkg-query` through the APT backend.

Possible outcomes:

```text
Not installed
Installed and current version known
Installed but candidate version newer
Installed state unknown
```

## Already installed and no upgrade requested

Desired output:

```text
✔ git is already installed

Backend: APT
Installed version: 1:2.53.0-1ubuntu1
Candidate version: 1:2.53.0-1ubuntu1

Nothing to install.
```

Do not launch APT.

Offer explicit actions only when useful:

```text
[1] Show package information
[2] Reinstall
[3] Upgrade if a newer version is available
[0] Cancel
```

Do not automatically reinstall.

## Installed but newer candidate exists

Show:

```text
⚠ git is already installed, but a newer version is available.

Installed: 1:2.52...
Available: 1:2.53...

[1] Upgrade
[2] Reinstall current candidate
[0] Cancel
```

The requested `install` command must not silently turn into an upgrade unless the user selects it.

---

# 12. APT Lock Preflight and Error Classification

APT and dpkg may be busy because another package-management process is active.

Allp must never:

- delete a lock file;
- recommend deleting a lock file;
- kill packagekitd automatically;
- kill apt, dpkg, unattended-upgrades, or a software-center process automatically;
- run concurrent package mutations.

Classify known lock failures as:

```text
Busy
```

not generic Failed.

Recognize messages including:

```text
Could not get lock
Unable to acquire the dpkg frontend lock
It is held by process
Could not open lock file
Waiting for cache lock
```

Extract when available:

- lock path;
- holder PID;
- holder process name;
- native exit code.

Example normalized issue:

```text
APT is busy

Lock: /var/lib/dpkg/lock-frontend
Holder: packagekitd
PID: 7515

Another package-management operation is currently running.
```

---

# 13. APT Busy-Lock UX

When a lock is detected before or during execution:

```text
⚠ APT is currently busy.

The package database lock is held by:
  Process: packagekitd
  PID: 7515
  Lock: /var/lib/dpkg/lock-frontend

Do not remove the lock file.

[1] Wait and retry
[2] Retry now
[3] Cancel
```

If `packagekitd` is the holder, a helpful note may say:

```text
The graphical software center or a background update service may be checking for updates.
Closing the software center or waiting briefly may release the lock.
```

Do not claim the process is safe to terminate.

## Wait and retry

Implement bounded waiting:

- configurable timeout;
- visible progress;
- cancellation support;
- no busy loop;
- no repeated password prompts;
- preserve the already confirmed execution plan;
- revalidate the plan before retry if package state may have changed.

Suggested option:

```bash
--lock-timeout <seconds>
```

A reasonable default may be chosen and documented.

If the native APT version supports a safe lock-timeout option, the APT backend may use it. Otherwise implement centralized bounded retry logic around the same immutable plan.

Do not use shell loops.

## Retry now

Retry once after rechecking the lock.

## Cancel

Output:

```text
ℹ Installation cancelled because APT is busy
0 commands executed by Allp
```

---

# 14. APT Lock Failure in Non-Interactive Mode

Do not wait indefinitely.

Return a stable Busy/TemporaryUnavailable exit code.

Example:

```text
APT is busy: /var/lib/dpkg/lock-frontend is held by packagekitd (PID 7515).

Retry later or use:
  allp install git --lock-timeout 60 --yes
```

Do not classify this as a normal package-command failure.

Add a centralized error variant such as:

```rust
AllpError::BackendBusy
```

or equivalent.

---

# 15. Confirmation Must Remain Separate from Busy Retry

The user already confirmed:

```text
Install this package? [Y/n]: y
```

If the exact same immutable plan is delayed only because of a lock, waiting and retrying does not require a second installation confirmation.

However, rebuild and reconfirm when:

- selected package changes;
- candidate version changes materially;
- command arguments change;
- scope changes;
- privilege mode changes.

Lock waiting itself is not authorization to alter the plan.

---

# 16. Better APT Failure Rendering

Generic fallback:

```text
✖ APT command failed
Exit code: 100
Command: /usr/bin/apt-get install -- git
```

Busy-specific rendering:

```text
⚠ APT did not start because another package operation owns the lock.

Process: packagekitd
PID: 7515
Lock: /var/lib/dpkg/lock-frontend
```

Do not show a red failure icon when nothing was attempted due to a temporary lock unless the timeout expires or the user requested fail-fast behavior.

On timeout:

```text
✖ APT remained busy for 60 seconds.
No package changes were made.
```

---

# 17. Preserve Native Output Without Duplicating Noise

Native output such as:

```text
Nothing to do.
All snaps up to date.
```

may remain visible during execution.

The normalized summary should translate it to:

```text
✔ Snap  Up to date
```

Do not repeat the entire native output in the summary.

Use backend-specific parsers only where reliable.

Fallback to `Completed` when change state cannot be reliably determined.

---

# 18. Required Regression Tests

## Status normalization

- npm global with no outdated packages → UpToDate;
- npm project without manifest → NotApplicable;
- missing pipx → Unavailable;
- protected system Python → Protected;
- no duplicated status labels;
- normal output hides optional unavailable tools;
- verbose output shows unavailable details;
- JSON contains all statuses;
- counters include every state.

## Target selection

- unavailable target never appears as selected;
- not-applicable target never produces an execution plan;
- protected target never produces an unsafe execution plan;
- only actionable plans are selected;
- component readiness is separate from ecosystem readiness.

## Cancellation

- Enter at `[y/N]` cancels;
- explicit `n` cancels;
- explicit `y` proceeds;
- cancellation runs zero child commands;
- normal cancellation summary is compact;
- verbose cancellation may show target details.

## Root and sudo

- root mode has no nested sudo;
- root mode still asks operation confirmation;
- root mode never prints normal-user notice;
- original user is recovered under sudo;
- Flatpak user scope de-escalates;
- Flatpak system scope remains root;
- original HOME is used for user scope.

## APT installed state

- already-installed exact package performs no install by default;
- installed package with same version shows AlreadyInstalled;
- newer candidate offers upgrade but does not auto-upgrade;
- reinstall requires explicit selection;
- installed-state query failure has a safe fallback.

## APT lock handling

- lock stderr maps to Busy;
- process name and PID are parsed when present;
- lock path is parsed;
- lock file is never deleted;
- holder process is never killed;
- wait/retry is bounded;
- cancellation works during waiting;
- non-interactive lock returns stable exit code;
- retry preserves exact plan;
- changed plan requires reconfirmation;
- dry run never probes by taking a mutation lock;
- no repeated sudo request during retry.

---

# 19. Required CLI Regression Matrix

Run non-destructive tests:

```bash
allp install git --from apt --dry-run
allp install git --from apt
sudo allp install git --from apt --dry-run

allp update --dry-run
allp update --verbose --dry-run
allp upgrade --dry-run
allp upgrade --verbose --dry-run

allp update --scope dev --dry-run
allp upgrade --scope dev --dry-run
```

Use fake APT/dpkg executables and fixtures to test:

```text
already installed
newer candidate
busy lock held by packagekitd
busy lock timeout
retry succeeds
generic exit code 100
```

Do not manipulate the real dpkg lock in automated tests.

Do not kill the real packagekitd process.

---

# 20. Architecture Guardrails

Keep backend-specific behavior local:

```text
APT installed-state parsing
APT lock-error parsing
APT native plan construction
```

Keep generic behavior centralized:

```text
normalized operation statuses
target selection
confirmation
batch cancellation
bounded retry policy
renderer
summary counting
runtime privilege context
```

Generic operations must not contain raw APT stderr strings.

The APT backend should convert native output into normalized backend issues such as:

```rust
BackendIssue::Busy {
    lock_path,
    holder_pid,
    holder_process,
}
```

The generic execution coordinator decides whether to:

- wait;
- retry;
- cancel;
- return an exit code.

---

# 21. Quality Gate

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

Do not claim success for commands that did not run.

Do not perform destructive real package operations while validating the repair.

---

# 22. Final Report

At completion, report:

1. Root causes of each regression.
2. Exact modules changed.
3. Status-model changes.
4. Target-discovery changes.
5. Cancellation changes.
6. Flatpak sudo/original-user changes.
7. APT installed-state preflight.
8. APT lock classification and retry behavior.
9. New exit codes or error variants.
10. Commands actually run.
11. Actual test results.
12. Remaining limitations.
13. Whether existing working features were preserved.
14. Public-alpha readiness after this repair.

Do not report the task complete if:

- unavailable targets still appear as selected;
- cancellation still prints many fake skipped failures;
- `Skipped · Skipped` still appears;
- APT busy-lock errors remain generic exit-code failures;
- an already-installed package still triggers an ordinary install automatically;
- Flatpak user scope can run with root HOME;
- previously working interactive search or confirmation behavior regresses.
