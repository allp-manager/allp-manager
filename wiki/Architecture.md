# Architecture

[← Wiki home](Home.md) · [فارسی](Architecture.fa.md)

Allp is a Rust 2021 application organized around domain models, backend
capabilities and immutable execution plans.

```text
CLI
 └─ App bootstrap
    ├─ PlatformContext + RuntimePrivilegeContext
    ├─ CapabilityRegistry + RequirementSet
    ├─ Detector → DetectedBackendSet
    └─ Operation → Backend query/plan → Renderer → ProcessRunner

Self-update
 └─ trusted GitHub source → manifest → staged verification
    → platform replacement → post-check/rollback → guarded continuation
```

## Module responsibilities

| Module | Responsibility |
|---|---|
| `domain` | Pure package, report, capability, error and execution models |
| `platform` | OS, distro family, architecture/libc, WSL/container, users and data paths |
| `capabilities`, `requirements` | Resolve executables and structured prerequisites |
| `discovery` | Fresh backend detection and explicit readiness states |
| `backends` | Native argv, parsers, capabilities and plan construction |
| `operations` | Source-agnostic use cases and selection flow |
| `execution` | Direct process spawning, timeouts, streaming and privilege boundary |
| `cli` | Clap arguments, prompts, JSON, paging and live maintenance UI |
| `identity` | Cross-backend software relationships without automatic source choice |
| `profiles` | Validated, atomic TOML inventory persistence |
| `self_update`, `release`, `state` | Trusted release discovery, verification and replacement |

## Architectural invariants

1. Discovery is fresh on every invocation.
2. Generic operations select by capability, not hard-coded backend ID.
3. Native flags and parsers stay inside backend modules.
4. Mutating backend methods return `ExecutionPlan`; they do not spawn.
5. The runner uses `std::process::Command`, never `sh -c`.
6. Multiple meaningful sources require a user decision.
7. Unknown parser output is not converted into a false “no results”.

## Search pipeline

Backends are queried with bounded concurrency. Results are normalized into
Exact, Related and Fuzzy matches. Exact results are always visible; related
results are selected round-robin across backends; fuzzy results require `--all`.
Verified identity groups, probable relationships and same-name coincidences
remain visibly distinct.

## Adding a backend

A backend implements the contract in `src/backends/contract.rs`, declares
requirements and capabilities, owns all native parsing/planning, is registered
once in `src/backends/catalog.rs`, and adds fixtures plus tests. Run:

```bash
make quality
bash scripts/check-architecture.sh
```

See [ARCHITECTURE.md](https://github.com/allp-manager/allp-manager/blob/main/ARCHITECTURE.md) and
[ADDING_BACKEND.md](https://github.com/allp-manager/allp-manager/blob/main/docs/ADDING_BACKEND.md) for the normative rules.
