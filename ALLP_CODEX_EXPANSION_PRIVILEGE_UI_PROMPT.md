# Allp — Ecosystem Expansion, Colored UX, and Unified Privilege Policy

Read this file together with:

- `ALLP_CODEX_MASTER_PROMPT.md`
- `ALLP_CODEX_UX_HARDENING_PROMPT.md`

Work directly in the current Allp repository. Do not create a replacement project and do not duplicate the existing engine.

The current backend engine, discovery model, execution-plan model, dry-run behavior, and modular architecture should be preserved where they are correct. This phase expands platform coverage, adds Python and Node ecosystems, improves terminal presentation, and defines one consistent privilege policy for every backend.

---

# 1. Product Direction

Allp is not only a wrapper around Linux system package managers.

Allp is a unified discovery, comparison, selection, and execution engine across:

1. Linux system package managers
2. Universal application managers
3. Homebrew on macOS and Linux
4. Python package sources and installers
5. Node.js package sources and installers

Examples:

```bash
allp search chatgpt --from python
allp install openai --from python
allp install black --from python
allp install typescript --from node
allp install git
```

Allp must search the relevant sources, normalize results, classify them, and let the user choose when multiple meaningful candidates exist.

Never silently install the first fuzzy result.

---

# 2. Terminal Visual Language

Improve the human-readable terminal UI with tasteful colors, status icons, spacing, and hierarchy.

Use both icon and color. Never rely on color alone.

```text
✔ Success / Ready / Completed       green
✖ Error / Failed / Not available    red
⚠ Warning / Partial / Risk          yellow or amber
ℹ Information / Detail              cyan or blue
➜ Selected / Next action            cyan or magenta accent
● Running / In progress              cyan
○ Waiting / Skipped                  dim or neutral
```

Examples:

```text
✔ APT detected
⚠ Related matches may be unofficial or unrelated
✖ Snap search failed: daemon unavailable
ℹ Native command: sudo -- /usr/bin/apt-get update
➜ Selected source: PyPI · openai
```

Rendering rules:

- Use bold headings.
- Color only meaningful labels, icons, and statuses.
- Errors must be red.
- Success must be green.
- Warnings must be yellow or amber.
- Information must be cyan or blue.
- Selected items must use a visible accent.
- Keep native commands visually distinct.
- Avoid excessive animation or decorative noise.
- Never obscure native package-manager output.

Respect:

```text
NO_COLOR
TERM=dumb
--no-color
non-TTY output
JSON output
redirected stdout
```

JSON must contain no ANSI escapes. Color policy belongs in the renderer, never in backend implementations. Add rendering tests for colored and non-colored output.

---

# 3. Linux and macOS Coverage

Do not hardcode behavior by distribution name when the real abstraction is the package-manager family.

Implement or complete these native backend families:

## Debian family

- APT
- Debian
- Ubuntu
- Linux Mint
- Pop!_OS
- elementary OS
- APT-based derivatives

Use `apt-cache`, `apt-get`, and `dpkg-query` where appropriate.

## Arch family

- Pacman
- Arch Linux
- Manjaro
- EndeavourOS
- Garuda
- Pacman-based derivatives

Never run standalone `pacman -Sy` as a normal update operation.

## Fedora / RHEL family

- DNF5
- DNF
- Fedora
- RHEL
- Rocky Linux
- AlmaLinux
- CentOS Stream

Prefer DNF5 when both are available.

## openSUSE family

- Zypper
- openSUSE Leap
- openSUSE Tumbleweed
- SUSE Linux Enterprise

## Alpine family

- APK
- Alpine Linux

## Void Linux

- XBPS
- `xbps-query`
- `xbps-install`
- `xbps-remove`

## Gentoo family

- Portage / emerge
- Gentoo
- Funtoo where compatible

Only advertise capabilities that can be implemented safely.

## Solus

- eopkg

## Clear Linux

- swupd

## Additional systems

Evaluate and document support status for:

- Nix / NixOS
- rpm-ostree immutable systems
- transactional-update systems
- Guix
- Slackware tools

Do not label incomplete support as stable. Use:

```text
Stable
Experimental
Detection only
Unsupported
```

Keep Flatpak and Snap as universal application backends.

The goal is broad package-manager-family coverage, not dishonest claims that every Linux distribution is fully supported.

---

# 4. Homebrew Support

Add Homebrew as a first-class backend for macOS and Linuxbrew.

Detect through the normal discovery model, including common locations such as:

```text
/opt/homebrew/bin/brew
/usr/local/bin/brew
/home/linuxbrew/.linuxbrew/bin/brew
```

Do not depend only on those paths.

Support where available:

- Search
- Install
- Remove
- Update
- Upgrade
- List
- Info

Distinguish Formula and Cask.

If both are meaningful, show both and let the user choose.

Homebrew must never run as root. When Allp was invoked with sudo and `SUDO_USER` is available, Homebrew operations must run as the original user. If safe original-user execution cannot be established, abort with a clear error.

---

# 5. Python Ecosystem — Add Now

Only Python and Node development ecosystems are required in this phase.

Support ecosystem and precise selectors:

```bash
--from python
--from pypi
--from pip
--from pipx
--from uv
```

Implement:

- PyPI as registry/source
- pip
- pipx
- uv when installed

The source and installer are separate concepts.

For search results show when available:

- package ID
- display name
- latest version
- description
- registry
- installer choices
- artifact type
- install scope
- owner/publisher
- homepage/repository
- latest release metadata

Do not invent official/unofficial claims.

For vague or misspelled queries such as:

```bash
allp install chatgptapi --from python
```

show Exact, Related, and Fuzzy results, but never auto-install a Fuzzy result.

Show:

```text
⚠ No exact package was found.
  Related results may be unofficial, unrelated, abandoned, or malicious.
  Review the package name, owner, homepage, and source before installing.
```

Model Python scopes:

- active virtual environment
- current Python environment
- user scope
- isolated CLI through pipx
- uv-managed environment

Do not silently install into system Python. Respect PEP 668. Never bypass externally managed environment protection automatically. Do not use sudo for Python packages by default.

---

# 6. Node.js Ecosystem — Add Now

Support:

```bash
--from node
--from npm
--from pnpm
--from yarn
```

Implement:

- npm registry as source
- npm
- pnpm
- Yarn when installed

Do not add other language ecosystems in this phase.

A registry package may have several installer choices. Do not show npm, pnpm, and Yarn as separate products when they refer to the same registry package.

Example:

```text
Package
  npm registry · typescript

Installers
[1] npm
[2] pnpm
[3] yarn
```

Model scopes:

- current project
- global user tool
- workspace

Do not silently modify `package.json` or lockfiles. Project-scoped installation requires explicit intent or a clearly detected project context with confirmation. Do not use sudo for Node installations by default.

---

# 7. Unified Cross-Ecosystem Search and Selection

A command without `--from` may find candidates across several domains.

Example:

```bash
allp install git
```

Possible groups:

```text
System Packages
Universal Applications
Homebrew
Python Packages
Node Packages
```

Example output:

```text
Install Candidates for "git"

System Packages
[1] APT · git · Exact

Universal Applications
[2] Snap · git-scm · Related

Homebrew
[3] Formula · git · Exact

Python Packages
[4] PyPI · gitpython · Related

Node Packages
[5] npm · simple-git · Related

⚠ Matching names across ecosystems do not imply the same software.
```

Rules:

- Never silently choose across ecosystems when multiple meaningful candidates exist.
- Exact matches in separate ecosystems are separate choices.
- Related and Fuzzy results must be labeled.
- Fuzzy-only matches must never auto-install.
- Non-interactive ambiguity must return the stable ambiguity exit code.
- `--from` narrows both search and installation.

---

# 8. Unified Privilege Model

Implement one central privilege policy for every backend and mutating operation.

Do not implement sudo logic separately inside APT, Snap, Brew, Python, Node, or any other backend.

Model runtime context explicitly:

```text
NormalUser
RootDirect
SudoRootWithOriginalUser
```

Model plan-level requirements:

```text
NoElevation
RootRequired
OriginalUserRequired
Conditional
```

Examples:

```text
APT install         RootRequired
APT update          RootRequired
Snap refresh        RootRequired
Flatpak --user      NoElevation
Flatpak --system    RootRequired
Homebrew            OriginalUserRequired / NoElevation
pip in venv         NoElevation
pipx user install   NoElevation
npm project install NoElevation
```

Privilege must be plan-specific, not merely backend-specific.

---

# 9. Normal-User Behavior

Example:

```bash
allp update
```

Flow:

1. Detect as the normal user.
2. Search/select/build all plans.
3. Show native commands.
4. Identify root-required plans.
5. Ask once before elevation.

Example:

```text
⚠ Administrator access is required for 2 operations.

Allp itself is running as your normal user.
Only the native child commands listed above will be elevated.

Continue with sudo? [Y/n]
```

If accepted, elevate only RootRequired children. If rejected, cancel root-required operations clearly. Do not invoke sudo during discovery, search, ranking, selection, or plan construction. Do not ask during dry run.

---

# 10. Behavior When Allp Is Already Running as Root

Example:

```bash
sudo allp install git
```

When effective UID is root:

- do not say Allp is running as a normal user;
- do not ask permission to use sudo;
- do not prefix root-required commands with another sudo;
- execute root-required commands directly;
- show one concise notice:

```text
⚠ Allp is running with administrator privileges.
  Root-required system operations will run directly.
```

Do not repeat this warning before every child command.

If `SUDO_USER` and `SUDO_UID` are present, preserve the original-user identity.

User-scoped operations must run as the original user, including:

- Homebrew
- pipx user operations
- Python user/virtual-environment operations
- npm/pnpm/Yarn user or project operations
- Flatpak user scope

Never create root-owned files in the user's home, project, Python environment, Node project, caches, or Homebrew prefix.

Use a central de-escalation mechanism. Do not use shell strings. If safe de-escalation is unavailable, refuse the affected user-scoped operation and explain why.

---

# 11. Root-Not-Required Behavior

If every selected plan is NoElevation:

- do not mention sudo;
- do not ask for administrator permission;
- do not run `sudo -v`;
- execute normally.

Examples include user-scoped pipx, Node project operations, and Flatpak user scope.

---

# 12. Privilege Confirmation UX

Show plans before any privilege prompt.

Normal-user example:

```text
Planned Operations

1. APT
   Action: Install system package
   Package: git
   Command: sudo -- /usr/bin/apt-get install -- git
   Privilege: Administrator access required

2. pipx
   Action: Install isolated Python CLI
   Package: black
   Command: /usr/bin/pipx install black
   Privilege: Current user

⚠ One operation requires administrator access.
  Only that native child command will be elevated.

Continue? [Y/n]
```

Root example:

```text
1. APT
   Command: /usr/bin/apt-get install -- git
   Privilege: Already running as administrator

2. pipx
   Command: run as original user: pipx install black
   Privilege: Original user context
```

Never display a false normal-user message when effective UID is root.

---

# 13. Dry Run

`--dry-run` must:

- detect;
- search;
- rank;
- select;
- build real plans;
- render privilege behavior;
- render exact commands;
- execute nothing;
- never request a password;
- never invoke sudo;
- never modify a project or environment.

Example:

```text
✔ Dry run completed
  2 operations planned
  0 commands executed
  0 privilege prompts triggered
```

---

# 14. Registry Security

Python and Node registries may contain malicious, abandoned, or typosquatted packages.

Required:

- never auto-install a Fuzzy result;
- warn when there is no Exact match;
- show registry clearly;
- show owner and repository when available;
- do not infer official status from package name;
- require explicit selection for ambiguity;
- do not execute lifecycle scripts in dry run;
- warn that npm installation may execute lifecycle scripts for untrusted or fuzzy selections.

---

# 15. Architecture

Introduce or refine generic concepts such as:

```rust
enum PackageDomain {
    System,
    Universal,
    Homebrew,
    Python,
    Node,
}

enum PrivilegeRequirement {
    NoElevation,
    RootRequired,
    OriginalUserRequired,
    Conditional,
}

enum RuntimePrivilegeContext {
    NormalUser,
    RootDirect,
    SudoRootWithOriginalUser,
}
```

Exact names may vary.

Rules:

- Generic operations remain backend-agnostic.
- Execution coordinator decides privilege behavior.
- Renderer decides color and icons.
- Ranking remains generic.
- Registry search and installer selection are separate for Python and Node.
- Never use `sh -c` or `bash -c`.

---

# 16. Required Tests

## Color

- success green;
- error red;
- warning amber/yellow;
- information cyan/blue;
- `NO_COLOR` disables ANSI;
- `--no-color` disables ANSI;
- JSON has no ANSI;
- redirected output remains clean;
- icons remain understandable without color.

## Backend discovery

Use fake executables and fixtures for:

- APT
- Pacman
- DNF5/DNF
- Zypper
- APK
- XBPS
- emerge
- eopkg
- swupd
- Flatpak
- Snap
- Homebrew macOS
- Linuxbrew

## Python

- `--from python` expansion;
- PyPI normalization;
- pip/pipx/uv detection;
- virtual environment behavior;
- PEP 668 behavior;
- fuzzy-only result never auto-installs;
- no sudo by default;
- root invocation de-escalates user-scoped work.

## Node

- npm registry search;
- npm/pnpm/Yarn detection;
- one package with several installer choices;
- project/global scope;
- no silent manifest or lockfile modification;
- no sudo by default;
- root invocation protects ownership.

## Cross ecosystem

- exact name in APT and Brew;
- Related names in Python and Node;
- no hidden preference;
- `--from` narrows correctly;
- non-interactive ambiguity fails.

## Privilege

- normal user + RootRequired asks once;
- normal user + NoElevation does not mention sudo;
- root invocation does not ask for sudo;
- root invocation does not prefix sudo;
- root invocation never prints “normal user”;
- original sudo user is preserved;
- Homebrew never runs as root;
- user-scoped Python/Node work does not create root-owned files;
- dry run never invokes sudo;
- mixed-privilege batches handle each plan correctly.

---

# 17. Documentation

Update:

- README.md
- README.fa.md
- ARCHITECTURE.md
- ROADMAP.md
- CHANGELOG.md
- SECURITY.md
- docs/CLI_CONTRACT.md
- docs/CAPABILITY_MATRIX.md
- docs/SECURITY_MODEL.md
- docs/ADDING_BACKEND.md

Add:

- docs/PRIVILEGE_MODEL.md
- docs/PYTHON_ECOSYSTEM.md
- docs/NODE_ECOSYSTEM.md
- docs/HOMEBREW_BACKEND.md
- docs/LINUX_COVERAGE.md
- docs/TERMINAL_UI.md

Document that `allp update` is preferred over `sudo allp update`, but sudo-invoked behavior must still be correct.

---

# 18. Scope Control

Add now:

- broader Linux backend coverage;
- Homebrew;
- Python;
- Node;
- unified privilege behavior;
- colored terminal UX.

Do not add now:

- Rust/Cargo ecosystem;
- Go modules;
- Composer;
- RubyGems;
- Maven/Gradle;
- NuGet;
- GUI;
- TUI;
- daemon;
- AI-based package selection;
- automatic recommendation engine.

---

# 19. Build and Quality Gate

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

Then run non-destructive CLI tests. Do not perform destructive real installs/removals on the primary developer machine.

---

# 20. Final Report

Report:

1. Linux backend families added.
2. Homebrew behavior on macOS and Linux.
3. Python source/installers implemented.
4. Node source/installers implemented.
5. Cross-ecosystem selection behavior.
6. Colored terminal UI and `NO_COLOR` behavior.
7. Normal-user privilege behavior.
8. Root and sudo-invoked behavior.
9. Original-user de-escalation behavior.
10. Files changed.
11. Commands run.
12. Actual test results.
13. Unsupported distributions/capabilities.
14. Public-alpha readiness judgment.

Never claim every Linux distribution is supported unless the capability matrix proves it. Report coverage honestly by package-manager family.
