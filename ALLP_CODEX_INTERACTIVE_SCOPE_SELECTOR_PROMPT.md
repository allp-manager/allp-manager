# Allp — Interactive Search Scope, Paged Results, and Numbered Selection Addendum

Read this file together with:

- `ALLP_CODEX_MASTER_PROMPT.md`
- `ALLP_CODEX_UX_HARDENING_PROMPT.md`
- `ALLP_CODEX_EXPANSION_PRIVILEGE_UI_PROMPT.md`

Work directly in the existing Allp repository. Do not create a replacement project.

This addendum defines the interactive scope selector and the final result-selection UX for ambiguous install queries such as:

```bash
sudo allp install chatgbt
allp install git
allp install formatter
```

The purpose is to prevent Allp from immediately searching every backend and dumping mixed results without first understanding what domain the user wants.

---

# 1. Interactive Scope Selection

When the user runs an install or search command without a precise `--from` filter, Allp should ask which package domain should be searched.

Example:

```bash
allp install chatgbt
```

Desired first prompt:

```text
Where should Allp search?

[1] Apps and tools
    System packages: APT, Pacman, DNF, Zypper, APK, XBPS, Portage, eopkg, swupd
    Universal applications: Snap, Flatpak / Flathub
    Homebrew Formula and Cask

[2] Developer ecosystems
    Python / PyPI / pip / pipx / uv
    Node.js / npm / pnpm / Yarn

[3] All sources

Choose a search scope [1-3, 0 to cancel]:
```

The user originally described the concepts as:

```text
tool / apps
workflow
all
```

Use polished user-facing labels instead:

```text
Apps and tools
Developer ecosystems
All sources
```

These names must be used consistently across help, prompts, documentation, and tests.

Optional short aliases may be supported:

```text
apps
dev
all
```

Examples:

```bash
allp install git --scope apps
allp install code --scope apps
allp install openai --scope dev
allp install chatgpt --scope all
```

`--from` remains the precise backend/ecosystem selector:

```bash
--from apt
--from snap
--from flatpak
--from brew
--from python
--from node
--from pipx
--from npm
```

`--scope` is the broad domain selector.

---

# 2. Interaction Under `sudo`

Example:

```bash
sudo allp install chatgbt
```

Allp is already running as root, so it must not ask:

```text
Do you want me to continue with sudo?
```

It must not claim:

```text
Allp is running as your normal user.
```

Instead, show one concise context notice:

```text
⚠ Allp is running with administrator privileges.
  System operations may run directly as root.
  User-scoped operations will be returned to the original user when possible.
```

Then show the scope selector.

The scope-selection prompt itself does not require any additional sudo confirmation.

Privilege behavior is decided only after the user selects a concrete result and Allp builds the final execution plan.

Examples:

- APT result selected while already root:
  - run `/usr/bin/apt-get ...` directly;
  - do not prepend another `sudo`.

- Snap system result selected while already root:
  - run directly as root when the native operation requires root.

- Python, pipx, npm, pnpm, Yarn, Homebrew, or project-scoped result selected:
  - run as the original user identified through `SUDO_USER`, `SUDO_UID`, and `SUDO_GID`;
  - use the original user's HOME;
  - do not create root-owned project files, caches, environments, or lockfiles.

If the original user cannot be recovered safely, refuse user-scoped execution with a clear error.

---

# 3. Scope Behavior

## Option 1 — Apps and Tools

Search both system-package and universal-application sources.

### System package sources

Search detected sources from:

```text
APT
Pacman
DNF / DNF5
Zypper
APK
XBPS
Portage / emerge
eopkg
swupd
Homebrew Formula
```

Only detected and ready backends should actually be queried.

### Universal application sources

Search detected sources from:

```text
Snap
Flatpak / Flathub
Homebrew Cask
```

Treat Flatpak as the installer/backend and Flathub as a source/remote where applicable.

Do not present `Flatpak` and `Flathub` as identical concepts internally:

```text
Installer/backend: Flatpak
Source/remote: Flathub
```

Do not show unavailable backend sections unless verbose diagnostics are requested.

Group Apps and Tools results in this order:

```text
1. System Packages
2. Universal Applications
```

Example:

```text
Apps and Tools Results for "git"

System Packages

APT
[1] git                 Exact     1:2.53.0
[2] git-lfs             Related   3.7.1

Homebrew Formula
[3] git                 Exact     2.53.0
[4] git-lfs             Related   3.7.1

Universal Applications

Snap
[5] git-scm             Related

Flatpak · Flathub
[6] org.example.GitApp  Related
```

## Option 2 — Developer Ecosystems

APT
[1] git                 Exact     1:2.53.0
[2] git-lfs             Related   3.7.1

Homebrew Formula
[3] git                 Exact     2.53.0
[4] git-lfs             Related   3.7.1
```

## Option 2 — Developer Ecosystems

For this phase, search only:

```text
Python
Node.js
```

Python sources/installers:

```text
PyPI
pip
pipx
uv
```

Node sources/installers:

```text
npm registry
npm
pnpm
Yarn
```

Remember:

- registry package and installer are separate concepts;
- one npm-registry package may have npm, pnpm, and Yarn as installer choices;
- one PyPI package may have pip, pipx, or uv as installer choices depending on artifact type and scope.

Example:

```text
Developer Ecosystem Results for "chatgpt"

Python · PyPI
[1] openai            Related
[2] chatgpt-api       Related
[3] chatgpt-cli       Related

Node.js · npm registry
[4] openai            Related
[5] chatgpt           Related
[6] chatgpt-api       Related
```

Display the security warning whenever there is no exact match:

```text
⚠ No exact package was found.
  Related or fuzzy registry packages may be unofficial, unrelated,
  abandoned, typosquatted, or malicious.
```

Never auto-install fuzzy-only Python or Node results.

## Option 3 — All Sources

Search every detected and eligible source, but display results in this exact high-level order:

```text
1. System Packages
2. Applications
3. Developer Ecosystems
```

Within those groups:

### System Packages

```text
APT
Pacman
DNF / DNF5
Zypper
APK
XBPS
Portage
eopkg
swupd
Homebrew Formula
```

### Applications

```text
Snap
Flatpak / Flathub
Homebrew Cask
```

### Developer Ecosystems

```text
Python / PyPI
Node.js / npm registry
```

Do not merge all sources into one flat list without section headings.

Add this warning:

```text
⚠ Similar names across package sources and programming ecosystems
  do not imply the same software.
```

---

# 4. Numbered Result Selection

Every selectable result must have a visible number.

The user selects the final result by entering its number.

Example:

```text
Search Results for "git"

System Packages

APT
[1] git                     Exact     1:2.53.0
    Fast, scalable, distributed revision control system

Homebrew Formula
[2] git                     Exact     2.53.0
    Distributed revision control system

Applications

Snap
[3] git-scm                 Related   2.55
    Git packaged as a Snap

Developer Ecosystems

Python · PyPI
[4] gitpython               Related   3.1.45
    Python library for interacting with Git repositories

Node.js · npm registry
[5] simple-git              Related   3.27.0
    Git interface for Node.js

Choose a result [1-5, 0 to cancel]:
```

After a result is selected:

1. Resolve installer choices if necessary.
2. Resolve scope if necessary.
3. Build the immutable execution plan.
4. Show exact native command.
5. Show privilege behavior.
6. Ask for final confirmation.
7. Execute.

Do not install immediately after the first number if another required choice remains.

Example for a Node registry package:

```text
Selected package
npm registry · typescript

Choose an installer:

[1] npm
[2] pnpm
[3] Yarn

Choose [1-3, 0 to cancel]:
```

Example for Python CLI:

```text
Selected package
PyPI · black

Choose an installation method:

[1] pipx
    Isolated user CLI environment

[2] uv tool
    Isolated user tool environment

[3] pip
    Current Python environment

Choose [1-3, 0 to cancel]:
```

---

# 5. Paged Interactive Results

Do not print every result at once when the list is large.

The desired behavior is similar to terminal completion and pager behavior in Zsh:

```text
24 results found — showing 1-10
Press Space for more, Enter to select, / to filter, q to cancel
```

Implement a direct interactive result pager/selector for TTY mode.

Do not merely dump all results and rely on terminal scrollback.

## Required controls

At minimum:

```text
Space       Next page
b           Previous page
j / Down    Move selection down
k / Up      Move selection up
Enter       Select highlighted result
<number>    Select result directly
/           Filter current result set
q / Esc     Cancel
?           Show controls
```

A simpler first implementation may use page-by-page numbered output with:

```text
Space       Next page
b           Previous page
<number>    Select
q           Cancel
```

but the interaction must not print the full result set by default.

## Page size

Use terminal height when safely available.

Fallback page size:

```text
10 results
```

Reserve lines for:

- heading;
- warnings;
- controls;
- page status;
- selection prompt.

## Page status

Display:

```text
Showing 1-10 of 37
Page 1 of 4
```

or:

```text
37 results found · Page 1/4
```

## Direct number selection

Result numbers must remain stable across pages.

Example:

```text
Page 1:
[1] ...
[2] ...
...
[10] ...

Page 2:
[11] ...
[12] ...
```

The user may type `17` from any page to select result 17.

Do not renumber from 1 on every page.

---

# 6. Pager Versus Interactive Selector

Use the correct output mode.

## Interactive install/search selection

For:

```bash
allp install <query>
```

when user selection is required, use the interactive paged selector described above.

## Non-selectable large output

For:

```bash
allp list
allp search <query> --all
```

when no immediate selection is required, an external pager such as `less` may be used.

Pager resolution:

1. safe `$PAGER`;
2. `less`;
3. `more`;
4. direct stdout fallback.

Never execute pager commands through a shell string.

## Non-TTY behavior

When stdout or stdin is not a TTY:

- do not open an interactive selector;
- do not wait for Space or arrow keys;
- emit stable plain output or JSON;
- if a required choice is ambiguous, return the ambiguity exit code;
- explain how to use `--from`, `--scope`, `--exact`, or a package ID.

## JSON behavior

JSON must never include:

- terminal prompts;
- ANSI color;
- page controls;
- interactive status lines.

---

# 7. Result Counts and Truncation

Before showing results, display how many were found:

```text
✔ 37 results found
```

If weak fuzzy results are hidden:

```text
✔ 12 relevant results found
ℹ 84 weak fuzzy matches hidden; use --all to include them
```

When results are grouped:

```text
System Packages:       4
Applications:          3
Developer Ecosystems:  5
Total:                 12
```

Do not query thousands of results and keep all of them in an unbounded in-memory UI model when backend-side limiting or streaming can be used safely.

Apply:

- backend query limit;
- normalized candidate limit;
- ranking;
- visibility policy;
- final interactive pagination.

---

# 8. Typo and Fuzzy Query Behavior

The example query:

```bash
sudo allp install chatgbt
```

appears to be a typo or fuzzy request.

Do not silently correct the word and install something.

Possible UX:

```text
⚠ No exact match found for "chatgbt".

Did you mean:
[1] chatgpt
[2] chatgpt-api
[3] openai

Search scope:
Developer Ecosystems
```

If All Sources was selected, maintain category grouping.

Require explicit user selection.

For Python and Node results, show a registry safety warning.

Do not make package trust decisions based only on fuzzy similarity.

---

# 9. Final Execution Flow

The final install flow should be:

```text
CLI parsing
→ runtime privilege-context detection
→ broad scope selection when needed
→ fresh backend discovery
→ capability filtering
→ backend/source queries
→ normalization
→ ranking
→ category grouping
→ interactive paged result selection
→ installer selection when needed
→ scope selection when needed
→ immutable execution-plan construction
→ native command rendering
→ privilege explanation
→ final confirmation
→ execution
→ summary
```

Do not ask for sudo before:

- scope selection;
- search;
- result selection;
- installer selection;
- plan construction;
- final confirmation.

---

# 10. Example Full Flow

Command:

```bash
sudo allp install chatgbt
```

Expected UX:

```text
⚠ Allp is running with administrator privileges.
  System operations may run directly as root.
  User-scoped operations will run as the original user when possible.

Where should Allp search?

[1] Apps and tools
[2] Developer ecosystems
[3] All sources

Choose a search scope [1-3, 0 to cancel]: 3
```

Then:

```text
Searching detected sources...

✔ 14 relevant results found
ℹ 31 weak fuzzy matches hidden; use --all to include them

⚠ No exact match found for "chatgbt".
⚠ Similar names across sources do not imply the same software.
```

Then paged grouped output:

```text
Search Results · Page 1/2

System Packages

APT
[1] chatgpt-shell          Related
[2] python3-openai         Related

Applications

Snap
[3] chatgpt-desktop        Related

Developer Ecosystems

Python · PyPI
[4] openai                 Related
[5] chatgpt-api            Related
[6] chatgpt-cli            Related

Node.js · npm registry
[7] openai                 Related
[8] chatgpt                Related

Showing 1-8 of 14
Space: next page · b: previous · number: select · /: filter · q: cancel
```

The user types:

```text
5
```

Then:

```text
Selected
Python · PyPI · chatgpt-api

⚠ This is a related registry result, not an exact match.
  Review package ownership and metadata before installing.

Choose an installer:

[1] pipx    Isolated user CLI
[2] uv      Isolated user tool
[3] pip     Current Python environment

Choose [1-3, 0 to cancel]: 1
```

Then plan:

```text
Execution Plan

Source:       PyPI
Package:      chatgpt-api
Installer:    pipx
Scope:        Original user
Action:       Install isolated Python CLI
Command:      run as original user: /usr/bin/pipx install chatgpt-api
Privilege:    Original user required
```

Because Allp was invoked through sudo, do not execute pipx as root.

Final confirmation:

```text
Continue? [Y/n]
```

---

# 11. Required Tests

## Scope selector

- no `--from` and no `--scope` shows the three choices;
- `--scope apps` searches system-package and universal-application backends;
- Apps and Tools results are internally grouped into System Packages and Universal Applications;
- `--scope dev` searches only Python and Node;
- `--scope all` searches all eligible domains;
- `--from` remains more precise than `--scope`;
- incompatible `--scope` and `--from` combinations return a clear CLI error.

## Group ordering

For All Sources, assert exact order:

```text
System Packages
Applications
Developer Ecosystems
```

## Number selection

- stable global numbering;
- direct selection by number;
- invalid number handling;
- zero cancels;
- selection on later pages;
- installer sub-selection.

## Interactive pagination

- page size respects terminal height where possible;
- fallback page size works;
- Space moves forward;
- `b` moves backward;
- `q` cancels;
- direct number selection works;
- results are not fully dumped;
- non-TTY never starts the selector.

## Privilege

- `sudo allp ...` never asks to use sudo again;
- root mode never prints a normal-user message;
- root-required result runs without nested sudo;
- user-scoped Python/Node/Homebrew operation de-escalates;
- original-user HOME is used;
- dry run does not invoke sudo or de-escalated execution.

## Typo/fuzzy safety

- no exact result prevents auto-install;
- registry warning appears;
- explicit selection is required;
- no package is silently corrected and installed.

---

# 12. Documentation

Update:

- README.md
- README.fa.md
- docs/CLI_CONTRACT.md
- docs/TERMINAL_UI.md
- docs/PRIVILEGE_MODEL.md
- docs/PYTHON_ECOSYSTEM.md
- docs/NODE_ECOSYSTEM.md

Document:

```bash
allp install <query>
allp install <query> --scope apps
allp install <query> --scope dev
allp install <query> --scope all
```

Explain the difference:

```text
--scope = broad category
--from  = precise backend, source, or ecosystem
```

---

# 13. Quality Gate

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

Run non-destructive interactive tests in a real TTY.

Do not perform destructive real install/remove operations on the primary development machine.

---

# 14. Final Report

Report:

1. Scope-selector implementation.
2. Final user-facing labels.
3. Group ordering.
4. Interactive pager controls.
5. Stable numbering behavior.
6. Non-TTY behavior.
7. Typo/fuzzy safety.
8. Root and original-user privilege behavior.
9. Files changed.
10. Commands run.
11. Actual tests.
12. Remaining limitations.
13. Public-alpha readiness.
