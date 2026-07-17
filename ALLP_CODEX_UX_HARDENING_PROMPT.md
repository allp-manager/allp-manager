# Allp — UX Hardening and Privilege Transparency Prompt for Codex

Read this file together with `ALLP_CODEX_MASTER_PROMPT.md`.

Work directly in the existing Allp repository. Do not create a replacement project. Preserve the current modular backend architecture and the working execution engine, but fix the remaining UX, search-ranking, paging, info-output, detection, update, and privilege-transparency problems described below.

The current engine is mostly functional. This task is primarily a UX hardening and behavior-correction pass required before public alpha release.

---

# 1. Current Observed Behavior

The following commands are already working:

```bash
allp detect
allp search git
allp search git --limit 5
allp search git --exact
allp install git --from apt --dry-run
allp update --from apt --dry-run
allp list --from apt
allp info git --from apt
```

Detected backends on the test system:

```text
APT
Snap
```

Not detected:

```text
Pacman
DNF
Flatpak
```

The current update command was executed as:

```bash
allp update
```

Allp itself was not started with sudo, but the program requested the user's sudo password during execution.

This is technically acceptable and is the intended security model:

- the Allp process runs as the normal user;
- Allp detects the user's normal PATH and user-scoped package managers;
- only native child commands requiring root are elevated;
- the user may be asked for a sudo password when the elevated child command starts.

However, the current UX does not explain this clearly before the password prompt.

---

# 2. Required Privilege Behavior

The official usage must remain:

```bash
allp update
```

Do not require:

```bash
sudo allp update
```

Allp must elevate only the child command that requires root.

Example internal plan:

```text
Allp process:
  /usr/local/bin/allp update

APT child process:
  sudo -- /usr/bin/apt-get update

Snap child process:
  sudo -- /usr/bin/snap refresh
```

Do not change this security architecture merely to avoid the password prompt.

## Required pre-execution privilege message

Before any sudo password prompt appears, Allp must clearly explain why elevation is required.

Desired example:

```text
Allp Update

Detected backends
✓ APT
✓ Snap

Selected operations
1. APT
   Action: Refresh package metadata
   Command: sudo -- /usr/bin/apt-get update
   Privilege: Administrator access required

2. Snap
   Action: Refresh installed snaps
   Command: sudo -- /usr/bin/snap refresh
   Privilege: Administrator access required

Allp is running as your normal user.
The native commands above require administrator privileges.
You may now be asked for your sudo password.

Continue? [Y/n]
```

Requirements:

1. Show the native command before triggering sudo.
2. Show which operations require root.
3. Explain that Allp itself is not running as root.
4. Explain that only child commands are elevated.
5. Do not request the sudo password during discovery or plan construction.
6. Trigger sudo only when execution begins.
7. Preserve native package-manager prompts.
8. Do not add `-y`, `--assumeyes`, or equivalent flags.
9. In `--no-interactive` mode, do not show an Allp confirmation prompt, but still print a concise privilege notice to stderr before execution.
10. In `--dry-run`, never trigger sudo.

Do not run `sudo -v` during discovery.

A centralized execution coordinator may optionally run one explicit sudo credential check immediately before the first root-required child operation, but only after all plans are visible, the user has confirmed execution, and privilege requirements have been explained.

If implemented, do it once per multi-backend operation, not once per backend.

The implementation must remain centralized in the execution/privilege layer, not inside APT, Snap, or other backends.

---

# 3. Detection Transparency

The current first message is:

```text
ℹ Detected: APT, Snap
```

This probably means fresh runtime detection happened, but the message is ambiguous.

The UI must distinguish three concepts:

1. Known backends compiled into Allp.
2. Backends detected and ready on this invocation.
3. Backends selected for the requested command.

For example, when running:

```bash
allp update --from apt
```

do not only print:

```text
Detected: APT, Snap
```

Instead print:

```text
Environment scan
Detected and ready: APT, Snap
Selected for update: APT
```

For:

```bash
allp update
```

print:

```text
Environment scan
Detected and ready: APT, Snap
Selected for update: APT, Snap
```

## Detection must genuinely run on every invocation

Do not reuse stale process-global or persisted detection results.

Required flow:

```text
parse command
→ fresh backend discovery
→ readiness filtering
→ capability filtering
→ --from filtering
→ selected backend set
→ plan construction
→ privilege explanation
→ execution
```

Add tests proving fresh detection:

1. Run with fake APT in temporary PATH.
2. Remove it.
3. Run again.
4. Second invocation must no longer detect APT.

Also test the inverse:

1. Start with no fake backend.
2. Add fake backend to PATH.
3. Next invocation must detect it.

## Normal and verbose detect output

Normal:

```text
Package managers

System
✓ APT       Ready
✗ Pacman    Not installed
✗ DNF       Not installed

Universal
✗ Flatpak   Not installed
✓ Snap      Ready
```

Verbose:

```text
APT
Status: Ready
Capabilities: Search, Install, Remove, Update, Upgrade, List, Info
Commands:
  apt-get:    /usr/bin/apt-get
  apt-cache:  /usr/bin/apt-cache
  dpkg-query: /usr/bin/dpkg-query
Probe: Passed
```

Do not print command-path and missing-command details for every unavailable backend in normal mode.

Use headings:

```text
System package managers
Universal package managers
Development package managers
```

Do not label the backend groups as `System packages` or `Universal applications` on the detect screen.

---

# 4. Update and Upgrade UX

The current update output repeats the command in both the plan and summary.

Avoid duplicate information.

Desired dry-run output:

```text
Allp Update · Dry Run

Environment scan
Detected and ready: APT, Snap
Selected for update: APT

Planned operation

APT
Action: Refresh package metadata
Command: sudo -- /usr/bin/apt-get update
Privilege: Administrator access required

────────────────────────────────

Dry run completed
1 operation planned
0 commands executed
```

Desired real execution output:

```text
Allp Update

Environment scan
Detected and ready: APT, Snap
Selected for update: APT, Snap

Planned operations

1. APT
   Action: Refresh package metadata
   Command: sudo -- /usr/bin/apt-get update
   Privilege: Administrator access required

2. Snap
   Action: Refresh installed snaps
   Command: sudo -- /usr/bin/snap refresh
   Privilege: Administrator access required

Allp is running as your normal user.
Only the native child commands requiring root will be elevated.
You may be asked for your sudo password.

Continue? [Y/n]
```

After execution, the summary should not repeat full commands unless verbose mode is enabled:

```text
Update Summary

✓ APT   Completed
✓ Snap  Completed

2 operations completed
0 failed
```

On partial failure:

```text
Update Summary

✓ APT   Completed
✗ Snap  Failed · native command exited with status 1

1 completed
1 failed
```

Return the documented partial-failure exit code.

Use title case in headings, not raw operation identifiers such as `update summary`.

---

# 5. Search Ranking Corrections

The bounded search output is much better, but ranking is still weak.

For query `git`, APT currently ranks packages such as:

```text
elpa-eshell-git-prompt
elpa-git-annex
elpa-git-auto-commit-mode
```

too highly.

Snap may rank:

```text
git-bszakaly
git-burn
git-cola
```

before more obvious candidates such as `git-scm`.

Improve generic ranking.

## Ranking priority

Use signals similar to:

1. Package ID exactly equals query.
2. Display name exactly equals query.
3. Package ID normalized exactly equals query.
4. Package ID begins with `query-`.
5. Package ID begins with query and has a known product/source suffix.
6. Query is a complete token in package ID.
7. Display name begins with query.
8. Query appears only in description.

Apply penalties for:

- `-dev`;
- `-doc`;
- language-library prefixes such as `golang-`, `rust-`, `node-`, `python-`, `lib`;
- deeply namespaced IDs;
- very long IDs;
- package IDs where query appears far from the beginning;
- transitional packages where known.

Ranking must stay generic and must not hardcode a preferred package manager.

Do not claim package equivalence.

---

# 6. Backend Diversity for `--limit`

Current behavior:

```bash
allp search git --limit 5
```

may consume the entire limit with APT results and hide Snap completely.

This defeats the purpose of Allp.

Implement diversity-aware selection.

Recommended algorithm:

1. Include exact matches first.
2. Group remaining visible Related results by backend.
3. Select results round-robin across backends.
4. Stop at the total limit.
5. Keep deterministic order.

Example desired result:

```text
[1] APT   git       Exact
[2] APT   git-lfs   Related
[3] Snap  git-scm   Related
[4] APT   git-gui   Related
[5] Snap  git-cola  Related
```

Exact matches do not need round-robin suppression.

Add tests ensuring one verbose backend cannot hide all other detected backends under a small total limit.

---

# 7. Install Source Selection

Test and fix:

```bash
allp install git --dry-run
```

without `--from`.

The implementation must not silently auto-select APT merely because APT has an Exact match while Snap has Related matches.

Show:

```text
Install sources for "git"

Exact matches

[1] APT
    Package: git
    Version: 1:2.53.0-1ubuntu1
    Type: System package

Related matches

[2] Snap
    Package: git-scm
    Version: ...
    Type: Universal application

[3] Snap
    Package: git-cola
    Type: Universal application

Related matches may not represent the same software.

Choose a package [1-3, 0 to cancel]:
```

It is acceptable to visually separate Exact from Related, but do not hide meaningful Related alternatives.

In non-interactive mode:

```bash
allp install git --no-interactive --dry-run
```

must fail with a clear ambiguity error when multiple meaningful candidates are present.

Suggested recovery:

```text
Multiple install candidates were found.

Use one of:
  allp install git --from apt --dry-run
  allp install git-scm --from snap --dry-run
```

---

# 8. `list` UX

Current behavior prints roughly 2,500 installed APT packages directly to the terminal.

This is not acceptable for default interactive UX.

Implement automatic paging for long human-readable output.

Preferred pager resolution:

1. `$PAGER` when set and safe.
2. `less`.
3. `more`.
4. Direct stdout fallback.

Recommended less arguments:

```text
-FRSX
```

Do not use a shell pipeline.

Spawn the pager process directly and write output to its stdin.

Show a count:

```text
Installed Packages · APT
2579 packages
```

Support at least:

```bash
allp list --from apt
allp list --from apt --filter git
allp list --from apt --limit 50
allp list --from apt --json
allp list --from apt --no-pager
```

Behavior:

- human output + terminal + large result set: pager;
- redirected stdout: no pager;
- JSON: no pager;
- `--no-pager`: no pager;
- small result set: direct output.

Filtering should occur before limiting and paging.

---

# 9. `info` UX

Current default info output prints too much raw APT metadata:

- checksums;
- filename;
- task list;
- dependency internals;
- raw long descriptions;
- package control fields.

Default output must be curated.

Desired default:

```text
Git

Backend:       APT
Package ID:    git
Version:       1:2.53.0-1ubuntu1
Installed:     Yes
Architecture:  amd64
Source:        Ubuntu repositories
Homepage:      https://git-scm.com/
Type:          System package
Scope:         System

Description
Fast, scalable, distributed revision control system.
```

Support:

```bash
allp info git --from apt
allp info git --from apt --full
allp info git --from apt --raw
allp info git --from apt --json
```

Definitions:

- default: curated important fields;
- `--full`: normalized extended metadata;
- `--raw`: native backend output when supported;
- `--json`: structured normalized data.

Do not print every unknown key-value field by default.

Wrap long descriptions for terminal width.

---

# 10. Consistent Headings and Vocabulary

Preferred headings:

```text
Package Managers
Search Results
Installed Packages
Package Information
Planned Operations
Update Summary
Upgrade Summary
```

Backend category labels:

```text
System Package Managers
Universal Package Managers
Development Package Managers
```

Package result category labels:

```text
System Packages
Universal Applications
Development Packages and Tools
```

These contexts must not be conflated.

---

# 11. Required CLI Tests

Run and validate:

```bash
allp detect
allp detect --verbose
allp detect --json

allp search git
allp search git --limit 5
allp search git --exact
allp search git --from snap
allp search git --json

allp install git --dry-run
allp install git --from apt --dry-run
allp install git --no-interactive --dry-run

allp remove git --dry-run
allp remove git --from apt --dry-run

allp update --dry-run
allp update --from apt --dry-run
allp upgrade --dry-run

allp list --from apt
allp list --from apt --filter git
allp list --from apt --limit 20
allp list --from apt --no-pager
allp list --from apt --json

allp info git --from apt
allp info git --from apt --full
allp info git --from apt --json
```

Do not perform destructive real install/remove tests on the developer's primary machine without explicit permission.

For real update testing, explain before executing that sudo may be requested.

---

# 12. Automated Tests Required

Add or update tests for:

## Privilege

- dry run never invokes sudo;
- Allp remains a normal-user process;
- root-required child is wrapped centrally;
- command is rendered before privilege invocation;
- no `sudo -v` during discovery;
- no privilege prompt for user-scoped operations.

## Detection

- fresh discovery each invocation;
- detected-ready and selected sets differ correctly with `--from`;
- normal output compact;
- verbose output includes paths and capabilities;
- unusable Snap is not Ready merely because the binary exists.

## Search

- Exact/Related/Fuzzy ranking;
- `git-scm` ranks above weak library results;
- development libraries receive penalties;
- total limit preserves backend diversity;
- deterministic output;
- weak fuzzy results hidden by default.

## Install

- meaningful Related results remain selectable;
- partial search prevents false uniqueness;
- non-interactive ambiguity fails clearly.

## List

- automatic pager for large TTY output;
- no pager for JSON;
- no pager for redirected stdout;
- `--no-pager`;
- filter-before-limit.

## Info

- curated default fields;
- extended output only with `--full`;
- raw metadata hidden by default;
- JSON remains complete and valid.

---

# 13. Build and Quality Gate

Before finishing, run:

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

Then run the non-destructive CLI test matrix.

Do not claim a test passed unless it actually ran.

---

# 14. Final Report Required

At completion report:

1. How privilege escalation now works.
2. How the UI explains sudo before prompting.
3. Proof that detection is fresh per invocation.
4. Difference between detected and selected backends.
5. Search-ranking changes.
6. Backend-diverse limiting changes.
7. Install-selection changes.
8. Pager implementation.
9. Info-output changes.
10. Files changed.
11. Commands executed.
12. Actual test results.
13. Remaining limitations.
14. Public-alpha readiness judgment.

---

# 15. Non-Negotiable Principles

- Allp itself should normally run without sudo.
- Only root-required native child commands are elevated.
- Detection must happen before elevation.
- Native commands must be visible before execution.
- Dry run must never request a password.
- Generic operations must remain backend-agnostic.
- No shell command strings.
- No hidden source selection.
- No automatic confirmation flags.
- No unbounded terminal output.
- UX is part of correctness.
