# Backend Contract

The backend contract is capability-based. A backend declares what it supports; operations filter by capabilities and operation semantics before calling backend methods.

## Required Identity

Every backend provides:

- stable ID;
- display name;
- category;
- command requirements;
- optional command requirements;
- aliases for ecosystem selectors;
- capability list.

Maintenance capabilities are intentionally separate from operation semantics. A backend that exposes `Update`/`Upgrade` for UI or compatibility must also classify the native operation as one of:

- `MetadataRefresh`
- `InstalledPackageUpgrade`
- `CombinedRefreshAndUpgrade`
- `SelfUpdate`
- `Unsupported`

`allp update` only runs metadata-only refresh operations. Backends whose native tool combines refresh and installed-package upgrade report `Not applicable` during `allp update`; their mutation is handled by `allp upgrade`.

Categories:

- `System`
- `Universal`
- `Development`

## Optional Methods

Backends may implement:

- `search`
- `list_installed`
- `info`
- `plan_install`
- `plan_remove`
- `plan_update`
- `plan_upgrade`
- `raw_info`

Default methods return unsupported-operation errors.

## Planning Rule

Backend plans; central runner executes.

Backends return `ExecutionPlan` for mutation. They do not call `std::process::Command` for install, remove, update, or upgrade.

Every plan declares `PrivilegeRequirement`. Backends do not add sudo themselves; the execution layer decides whether to elevate, run directly as root, or de-escalate to the original sudo user.

## Query Rule

Backends may execute native read-only commands and parse output into:

- `PackageCandidate`
- `InstalledPackage`
- `PackageInfo`

Prefer stable machine-readable native output where available.

`search` returns a `BackendSearchReport`, not a bare candidate vector. Its
observable states are:

- candidates with no issues: `Matches`;
- no candidates and no issues after recognizing the native no-match form:
  `NoMatches`;
- no candidates plus `UnrecognizedOutput`: parser drift, never a no-match;
- candidates plus one or more issues: partial results with `complete=false`.

Issues identify their kind (`unrecognized_output`, `command_failed`, or
`incomplete_metadata`) and may identify the failing stage. A backend that runs
multiple read-only commands keeps valid candidates when one stage fails and
records the failure; it must not discard good results or silently claim full
coverage. Every stable parser change requires sanitized valid, no-match, and
malformed fixtures. Multi-stage parsers also require a partial-result fixture
or an equivalent injected-command test.

Candidates include a package domain and may include installer choices. Source/registry and installer are separate concepts: PyPI is a source; pip, pipx, and uv are installers. The npm registry is a source; npm, pnpm, and Yarn are installers. crates.io is a source and Cargo is its installer.

`raw_info` is optional and returns native backend info output for `allp info --raw`. It must be read-only.

## Action Labels

Every mutating plan includes a human action label. Examples:

- `Refresh package metadata`
- `Upgrade installed DNF packages`
- `Refresh installed snaps`

Generic operations render these labels without knowing backend command syntax.
