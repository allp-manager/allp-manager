# Allp Architecture

Allp is a transparent orchestration layer over package managers already installed on the system. It does not resolve package dependencies, maintain a package database, or replace native package-manager behavior. The only Allp-owned artifact download is the verified official self-update path.

## Core Rules

1. Discovery runs on every invocation.
2. Operations filter by declared backend capabilities.
3. Backend-specific command syntax and parsers stay inside backend modules.
4. Mutating backend methods return immutable `ExecutionPlan` values.
5. The central runner executes native processes directly, never through `sh -c`.
6. Native mutating stdin, stdout, and stderr are inherited.
7. Multiple meaningful sources require user choice.
8. Partial read-only failures are reported and prevent false uniqueness.

## Runtime Flow

```text
CLI
  -> PlatformContext
  -> CapabilityRegistry / RequirementSet
  -> app bootstrap
  -> BackendDiscovery
  -> DetectedBackendSet
  -> operation use case
  -> backend query or ExecutionPlan
  -> renderer
  -> process runner
```

The self-update flow is separate:

```text
GitHubReleaseSource -> ReleaseManifest -> staged verification
  -> platform replacement -> rollback/post-check -> guarded re-execution
```

`main.rs` only parses CLI arguments, calls the app bootstrap, and maps errors to stable alpha exit codes.

## Modules

### `domain`

Pure shared models:

- `Capability`
- `BackendCategory`
- `PackageDomain`
- `MatchKind`
- `PackageCandidate`
- `InstalledPackage`
- `PackageInfo`
- `NativeCommand`
- `ExecutionPlan`
- `PrivilegeRequirement`
- `RuntimePrivilegeContext`
- reports, issues, operation statuses, errors, and exit codes

Domain code does not know native package-manager command syntax.

### `platform`, `capabilities`, and `requirements`

`PlatformContext` normalizes OS/distribution family, architecture/libc, WSL/container state, users, platform data paths, and the current executable's ownership/writability. `CapabilityRegistry` resolves shared executable capabilities once per operation. Backends expose structured requirement sets, including sockets, services, remotes, permissions, and network needs.

### `bootstrap`

Prerequisite installation is separate from the operation that requested it. APT, DNF, Pacman, Zypper, and APK providers map known requirements to immutable plans. Executable installation, service enablement, remote addition, configuration changes, and elevation are distinct mutations. Verification refreshes capabilities before the original operation can continue.

### `alternatives`

Alternative routing carries an explicit excluded-backend set. A failed exact Snap result can be excluded before fresh workers run, preventing cached Snap candidates from reappearing. Unrestricted search clears the exclusion and starts again.

### `self_update`, `release`, `state`, and `diagnostics`

Self-update owns the trusted GitHub source, strict SemVer checks, manifest target selection, checksum/archive/binary verification, replacement, rollback, and guarded relaunch. `state` writes channel/ETag/update state atomically in the platform directory. `diagnostics` combines platform, capabilities, backend state, Snap socket, Flatpak remotes, and release-target information for `allp doctor`.

### `discovery`

`Detector` owns per-invocation discovery. It searches `PATH` directly in Rust, resolves executable paths, and returns:

- `DiscoveryReport` for diagnostics and JSON output;
- `DetectedBackendSet` containing only ready backends.

Detection states are explicit:

- `Ready`
- `NotFound`
- `FoundButUnavailable`
- `FoundButUnconfigured`
- `UnsupportedVersion`
- `ProbeFailed`

Normal commands use fresh lightweight discovery. Backends may also run read-only probes through the central runner; Snap uses this to avoid reporting `Ready` when the binary exists but snapd is unusable. Discovery never invokes sudo.

### `backends`

Each backend owns:

- identity and category;
- command requirements;
- declared capabilities;
- native query commands and parsers;
- execution-plan construction;
- human action descriptions.

Backends do not spawn mutating processes.

Backends may expose raw native info output through `raw_info`; default CLI info remains curated unless the user asks for `--raw`.

Backends may also perform read-only install-planning preflight. Snap uses this hook for separate exact resolution through snapd REST, with a reasoned CLI fallback only when REST transport/compatibility permits it. It replaces the discovery row with canonical metadata before generic install planning can render a plan.

Backends can declare optional command requirements. Python uses this to detect pip, pipx, and uv as installer choices while keeping PyPI as the registry/source. Node uses it to detect pnpm and Yarn while keeping the npm registry as the source.

The catalog currently includes:

- stable alpha system/universal backends: APT, Pacman, DNF/DNF5, Flatpak, Snap;
- experimental system-family backends: Zypper, APK, XBPS, Portage/emerge, eopkg, swupd;
- experimental Homebrew/Linuxbrew;
- experimental Python and Node ecosystem backends.

### `operations`

Use-case modules coordinate capabilities only:

- `detect`
- `search`
- `install`
- `remove`
- `update`
- `upgrade`
- `list`
- `info`

The architecture check fails when generic operation source contains registered backend IDs.

Generic operations may branch on domain-level safety concepts, such as refusing automatic fuzzy Python/Node installs, but they do not contain native command syntax or backend-specific IDs.

### `execution`

The execution layer owns:

- command rendering;
- captured read-only query execution;
- per-query timeout;
- inherited stdio for mutating commands;
- child-only privilege elevation;
- original-user de-escalation for sudo-invoked user-scoped plans;
- direct `std::process::Command` invocation.

Commands are represented as executable path plus argument vector.

Plan-level privilege is represented as:

- `NoElevation`
- `RootRequired`
- `OriginalUserRequired`
- `Conditional`

Runtime context is represented as normal user, direct root, or sudo-root with original user. The runner, not the backends, decides whether to prefix `sudo --`, run directly, or use `sudo -u <SUDO_USER> --`.

### `cli`

The CLI layer owns Clap parsing, command-specific options, prompts, human rendering, JSON envelopes, colors, and spinners.

## Search Ranking

Backends normalize raw native results into `PackageCandidate`. Generic search then assigns:

- `Exact`
- `Related`
- `Fuzzy`

Default visibility:

- all exact matches;
- up to five related matches per backend;
- at most 25 visible results;
- fuzzy matches hidden unless `--all` is used.

Sorting is deterministic:

1. match class;
2. generic rank score inside the match class;
3. package domain and backend category;
4. backend display name;
5. package ID.

Visible related results are selected round-robin across backends after exact matches so one verbose backend cannot consume a small `--limit`.

Read-only backend queries run with bounded concurrency.

## Update And Upgrade Semantics

`update` and `upgrade` are backend-defined actions. Allp does not force equivalent semantics across managers.

v0.3.3 policy:

| Backend | Update | Upgrade |
|---|---|---|
| APT | refresh package metadata | upgrade installed APT packages |
| Pacman | unsupported | `pacman -Syu` full sync and upgrade |
| DNF/DNF5 | refresh metadata cache | upgrade installed DNF packages |
| Flatpak | update installed apps/runtimes | same native action |
| Snap | refresh installed snaps | same native action |
| Zypper | repository refresh | package update |
| APK | index update | package upgrade |
| XBPS | index sync | sync and upgrade |
| Portage | tree sync | world update |
| eopkg | repository update | package upgrade |
| swupd | update check | system bundle update |
| Homebrew | metadata update | package upgrade |

Mutating multi-backend operations run sequentially, continue after failures, and return exit code `8` when any backend fails.

All maintenance operations are planned before execution. The UI displays detected-ready backends separately from selected backends, renders every native command, explains root-required child elevation, and asks for confirmation before real interactive `update` / `upgrade` execution.

## Paging And Info

Human-readable `list` output is buffered and sent to a directly spawned pager for large interactive output. No shell pipeline is used. JSON, redirected stdout, `--no-pager`, and small result sets bypass the pager.

Default `info` output is curated. `--full` shows normalized extended metadata, while `--raw` asks the selected backend for native output.

## Adding A Backend

A future backend should require:

1. one backend module;
2. one category module declaration;
3. one registration entry in `backends/catalog.rs`;
4. backend-specific tests and fixtures.

Adding npm later must not require changing generic search, install, remove, update, upgrade, list, or info operations.
