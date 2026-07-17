# Allp v0.3.3 — Software Identity Resolution and Official Bootstrap Sources

Read this file together with all previous Allp specifications. Work directly in the existing repository. Do not create a replacement project.

This specification defines **Allp v0.3.3** and fixes a release-blocking identity-resolution bug.

## 1. Reproduced Bug

```bash
allp install Homebrew
```

Current behavior incorrectly returns the npm package `homebrew` as an Exact match and may plan:

```bash
npm install --global homebrew
```

The npm package named `homebrew` is not the Homebrew package manager.

This is not merely a ranking bug. It is a software-identity resolution bug.

## 2. Version

Set the project version to:

```text
0.3.3
```

Release title:

```text
Allp v0.3.3 — Official Software Resolution
```

Update Cargo metadata, lockfile, CHANGELOG, README files, version output, release docs, and tests.

## 3. Core Rule

Do not treat these concepts as equivalent:

```text
Exact package-name match
Exact software identity match
Official distribution source
Preferred installation method
```

An exact package ID in npm, PyPI, or another registry does not prove that the package is the software the user intended.

For query `Homebrew`:

```text
npm package ID: homebrew
Name match: Exact
Software identity: Conflicting
Official source: No
User-facing relationship: Exact name only / Conflicting name
```

Do not show this result as simply `Exact`.

## 4. Separate Match Dimensions

Add normalized concepts similar to:

```rust
enum NameMatchKind {
    Exact,
    NormalizedExact,
    Prefix,
    Token,
    Fuzzy,
}

enum IdentityConfidence {
    Official,
    Verified,
    Probable,
    Unverified,
    Conflicting,
}

enum DistributionRelationship {
    OfficialInstaller,
    OfficialPackage,
    VerifiedThirdPartyPackage,
    NameMatchOnly,
    Related,
    Fuzzy,
}
```

User-facing labels should include:

```text
Official
Official package
Verified third-party
Exact name only
Related
Fuzzy
Conflicting name
```

## 5. Canonical Software Identity Catalog

Add a small, curated, version-controlled identity catalog for software Allp directly manages or bootstraps.

Initial entries must include at least:

```text
Homebrew
APT
Pacman
DNF
Zypper
APK
Flatpak
Snap
Python
pip
pipx
uv
Node.js
npm
pnpm
Yarn
```

Suggested model:

```rust
struct SoftwareIdentity {
    canonical_id: String,
    display_name: String,
    aliases: Vec<String>,
    software_type: SoftwareType,
    official_domains: Vec<String>,
    supported_platforms: Vec<Platform>,
    official_distribution_methods: Vec<DistributionMethod>,
    known_name_collisions: Vec<NameCollisionRule>,
}
```

Suggested types:

```rust
enum SoftwareType {
    PackageManager,
    LanguageRuntime,
    RegistryClient,
    CliTool,
    DesktopApplication,
    Library,
    UniversalApplication,
}
```

Do not turn this into an AI recommendation database or a universal alias database.

## 6. Identity Resolution Before Registry Results

For install requests use this flow:

```text
normalize query
→ resolve canonical identity and aliases
→ determine software type
→ determine platform compatibility
→ determine official distribution methods
→ search detected sources
→ compare candidates to canonical identity
→ rank official candidates first
→ demote conflicting registry name matches
```

Queries such as:

```text
homebrew
HomeBrew
home brew
brew package manager
```

should resolve to canonical Homebrew identity.

## 7. Homebrew Bootstrap Result

When Homebrew is not detected and the user runs:

```bash
allp install homebrew
```

show:

```text
Recognized Software

Homebrew
Type: Package manager
Platforms: macOS and Linux
Status: Not installed

Official Installation
[1] Homebrew official installer
    Source: brew.sh
    Relationship: Official
    Operation: Bootstrap package manager

Other Name Matches
[2] npm registry · homebrew
    Type: Node.js package
    Relationship: Conflicting name match
    Warning: This is not the Homebrew package manager.
```

The official bootstrap result must rank before the npm result.

## 8. Bootstrap Operation

Add an internal operation such as:

```rust
OperationKind::Bootstrap
```

The CLI may continue accepting:

```bash
allp install homebrew
```

but the execution plan must say:

```text
Operation: Bootstrap package manager
```

Optionally add:

```bash
allp bootstrap homebrew
```

## 9. Official Installer Security

Do not use a hidden shell pipeline:

```text
curl ... | bash
```

Required plan:

1. Resolve the official installer URL from the maintained bootstrap definition.
2. Download through the centralized HTTP/download layer.
3. Write to a secure temporary file.
4. Show canonical software name, official domain, final URL, temporary file, interpreter command, privilege behavior, and expected prefix.
5. Ask for confirmation defaulting to No.
6. Execute directly as executable + argument vector, for example:

```text
program: /bin/bash
args:
  - /secure/temp/homebrew-install.sh
```

Do not use `sh -c`, `bash -c`, or a pipe.

Dry run must download nothing and execute nothing.

## 10. Homebrew Privilege Policy

Homebrew commands must not run as root.

For initial Linux bootstrap, the official installer may request sudo for limited setup work.

Allp must:

- launch bootstrap as the normal/original user;
- explain that the official installer may request sudo;
- never run installed `brew` as root;
- de-escalate when Allp was invoked through sudo;
- refuse if no safe original user can be recovered;
- avoid root-owned Homebrew prefixes.

Example:

```text
Privilege Notice

Allp will launch the official installer as:
  User: hossein

The installer may request administrator access for initial setup.
After installation, Homebrew commands must run without sudo.
```

## 11. Ranking Policy

For a recognized identity, rank:

```text
1. Official installer
2. Official package from a trusted detected backend
3. Verified third-party distribution
4. Exact registry-name-only match
5. Related
6. Fuzzy
```

Exact string equality must not outrank official identity.

For unknown software, keep exact package-name ranking but label it:

```text
Exact package name
```

not verified identity.

## 12. Collision Warning

When the user selects npm `homebrew`:

```text
⚠ Name collision detected

The npm package "homebrew" is not the Homebrew package manager.

Requested software:
  Homebrew · Package manager

Selected result:
  npm · homebrew · Node.js package

Continue reviewing this registry package? [y/N]
```

It must not be selected through default Enter.

If deliberately selected, require another final confirmation defaulting to No.

## 13. Software-Type Awareness

Every candidate should carry a normalized type where known:

```text
Package manager
System package
Universal application
Desktop application
CLI tool
Language library
Node.js package
Python package
Runtime
Installer
```

For Homebrew:

```text
Requested: Homebrew · Package manager
Candidate: npm homebrew · Node.js package
Type compatibility: Conflicting
```

Type helps ranking and warnings but does not prove identity by itself.

## 14. Registry Source and Installer Separation

Registry search must not depend on a local installer when the registry can be queried independently.

Required corrections:

- PyPI source/search is separate from pip availability.
- npm registry source/search is separate from npm/pnpm/Yarn availability.

If pip is missing:

```text
Python Registry
✔ PyPI source available
○ pip installer unavailable
○ pipx installer unavailable
○ uv installer unavailable
```

Do not fail all Python search with `No module named pip`.

Instead explain that results can be found but no compatible installer is available.

## 15. npm Global Permission Preflight

Before planning:

```bash
npm install --global <package>
```

inspect:

- effective user;
- original user;
- npm global prefix;
- prefix writability;
- Node version-manager context;
- whether global scope is appropriate.

If prefix is not writable:

```text
✖ npm global installation is not writable for the selected user.

Prefix:
  /usr/local

Allp will not retry with sudo.

Recommended actions:
  - use a Node version manager;
  - configure a user-writable npm prefix;
  - choose project-local installation when appropriate.
```

Never fix npm EACCES by silently adding sudo.

## 16. Partial Search Failures

A backend failure must not erase successful results, but must be explicit:

```text
Search completed with partial coverage

✔ APT        Completed
✖ DNF        Failed · RPM database unavailable
✔ npm        Completed
✔ PyPI       Completed
```

When coverage is incomplete:

- do not claim global uniqueness;
- do not auto-select;
- require confirmation;
- set `complete: false` in JSON.

## 17. DNF Diagnostic

For `rpmdb open failed`:

```text
✖ DNF could not access the RPM database.

The RPM database may be unavailable, locked, damaged, or incompatible
with the current environment.

Run:
  allp detect --verbose --from dnf
```

If `doctor` exists:

```bash
allp doctor --from dnf
```

Do not automatically rebuild or delete RPM database files.

## 18. Missing Python Installer Diagnostic

```text
⚠ Python was detected, but pip is unavailable.

PyPI search may still be used.
Installing Python packages requires one of:
  pip
  pipx
  uv
```

Respect PEP 668 and do not modify system Python automatically.

## 19. Expected Homebrew Flow

```bash
allp install Homebrew
```

Expected result:

```text
Recognized Software

Homebrew
Type: Package manager
Platforms: macOS, Linux
Status: Not installed

Official Installation
[1] Homebrew official installer
    Source: https://brew.sh
    Relationship: Official
    Operation: Bootstrap package manager

Other Name Matches
[2] npm registry · homebrew
    Type: Node.js package
    Relationship: Conflicting name match
    Warning: Not the Homebrew package manager

Choose a result [1-2, 0 to cancel]:
```

Selecting official installer:

```text
Bootstrap Plan

Software: Homebrew
Type: Package manager
Source: Official Homebrew installer
Platform: Linux
Run as: Original user
Installer URL: <official URL>
Expected prefix: /home/linuxbrew/.linuxbrew
Privilege: Installer may request sudo for initial setup

Run the official Homebrew installer? [y/N]
```

Selecting npm collision:

```text
⚠ Conflicting software identity

The npm package "homebrew" is not the Homebrew package manager.
Review this package separately? [y/N]
```

No automatic global npm installation.

## 20. JSON Contract

Candidate output must distinguish:

```text
name_match
identity_confidence
distribution_relationship
software_type
official_source
canonical_identity
```

Example collision:

```json
{
  "package_id": "homebrew",
  "source": "npm",
  "name_match": "exact",
  "identity_confidence": "conflicting",
  "distribution_relationship": "name_match_only",
  "software_type": "node_package",
  "canonical_identity": "homebrew-package-manager",
  "warning": "This npm package is not the Homebrew package manager."
}
```

## 21. Grouping

For `--scope all`, display:

```text
Recognized Software / Official Installers
System Packages
Universal Applications
Developer Ecosystems
```

The previous required ordering among the three generic groups remains unchanged.

With:

```bash
allp install homebrew --from npm
```

Allp may restrict to npm, but must still show the identity collision warning.

## 22. Confirmation

Preserve all v0.3.2 confirmation rules.

Additional rules:

- bootstrap defaults to No;
- conflicting identity defaults to No;
- no install immediately after selection;
- `--yes` must not bypass identity ambiguity;
- `--yes` may bypass only final confirmation after identity/source resolution;
- fuzzy and conflicting results require explicit selection.

## 23. Architecture

Suggested modules:

```text
src/domain/software_identity.rs
src/domain/distribution.rs
src/identity/catalog.rs
src/identity/resolver.rs
src/bootstrap/
src/bootstrap/homebrew.rs
```

Rules:

- registry backends return normalized metadata;
- identity resolver evaluates candidate relationships;
- renderer displays identity status;
- bootstrap implementation builds plans;
- execution layer executes plans;
- npm backend must not hardcode Homebrew collision rules;
- collision knowledge belongs in identity catalog/resolver.

## 24. Required Tests

### Identity

- Homebrew aliases resolve to canonical Homebrew;
- npm `homebrew` is exact name but conflicting identity;
- official bootstrap ranks first;
- registry exact name does not override official identity;
- unknown exact package names still work;
- software-type mismatch is shown;
- `--from npm` keeps collision warning.

### Bootstrap

- Linux plan;
- macOS plan;
- unsupported platform error;
- secure temporary download plan;
- no shell pipeline;
- direct interpreter execution;
- confirmation defaults to No;
- dry run downloads/executes nothing;
- sudo invocation de-escalates;
- brew never runs as root.

### Registry/installer separation

- PyPI search works without pip;
- npm registry search is independent from installer;
- missing installer is actionable.

### npm permissions

- writable prefix passes;
- unwritable prefix blocked before execution;
- no automatic sudo retry;
- project-local option offered where appropriate.

### Partial failures

- DNF failure does not hide official/npm results;
- incomplete coverage prevents auto-selection;
- issues appear in JSON.

## 25. Documentation

Update:

- README.md
- README.fa.md
- CHANGELOG.md
- docs/CLI_CONTRACT.md
- docs/SECURITY_MODEL.md
- docs/NODE_ECOSYSTEM.md
- docs/PYTHON_ECOSYSTEM.md
- docs/TERMINAL_UI.md
- docs/CAPABILITY_MATRIX.md

Add:

- docs/SOFTWARE_IDENTITY.md
- docs/OFFICIAL_BOOTSTRAP.md
- docs/NAME_COLLISIONS.md
- docs/HOMEBREW_BOOTSTRAP.md
- docs/V0_3_3_TEST_PLAN.md

Document prominently:

```text
Exact package name != verified software identity
```

## 26. Quality Gate

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

Use fake registries, fake executables, and temporary directories.

Do not run the real Homebrew installer in automated tests.
Do not install npm `homebrew` during tests.

## 27. Manual Non-Destructive Tests

```bash
allp install Homebrew --dry-run
allp install homebrew --scope all --dry-run
allp install homebrew --scope dev --dry-run
allp install homebrew --from npm --dry-run
allp search homebrew
allp search homebrew --json
sudo allp install homebrew --dry-run
```

Verify:

- official Homebrew ranks first;
- npm collision is clearly labeled;
- no automatic npm install;
- no EACCES operation begins;
- no nested sudo;
- dry run executes nothing;
- JSON preserves identity dimensions.

## 28. Final Report

Report:

1. Root cause.
2. Identity model.
3. Catalog entries.
4. Homebrew bootstrap implementation.
5. Collision behavior.
6. Registry/installer separation.
7. npm permission preflight.
8. DNF and missing-pip diagnostics.
9. Files changed.
10. Commands run.
11. Actual tests.
12. Remaining identity limitations.
13. Public-alpha readiness for v0.3.3.

Do not call v0.3.3 ready if npm `homebrew` is still shown as the Homebrew package manager or can be installed without a collision warning.
