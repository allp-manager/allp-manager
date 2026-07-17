# Backend Contract

The backend contract is capability-based. A backend declares what it supports; operations filter by capabilities before calling backend methods.

## Required Identity

Every backend provides:

- stable ID;
- display name;
- category;
- command requirements;
- optional command requirements;
- aliases for ecosystem selectors;
- capability list.

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

Candidates include a package domain and may include installer choices. Source/registry and installer are separate concepts: PyPI is a source; pip, pipx, and uv are installers. The npm registry is a source; npm, pnpm, and Yarn are installers.

`raw_info` is optional and returns native backend info output for `allp info --raw`. It must be read-only.

## Action Labels

Every mutating plan includes a human action label. Examples:

- `Refresh package metadata`
- `Upgrade installed DNF packages`
- `Refresh installed snaps`

Generic operations render these labels without knowing backend command syntax.
