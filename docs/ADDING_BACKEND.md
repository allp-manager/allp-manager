# Adding A Backend

Backends are adapters around existing native tools. They must not turn Allp into a package-manager implementation.

## Expected Change Set

A normal backend addition should require:

1. a new backend module;
2. one module export in its category;
3. one registration entry in `src/backends/catalog.rs`;
4. backend-owned tests and fixtures.

Adding npm later should not require changes in:

- `src/operations/search.rs`
- `src/operations/install.rs`
- `src/operations/remove.rs`
- `src/operations/update.rs`
- `src/operations/upgrade.rs`
- `src/operations/list.rs`
- `src/operations/info.rs`

If a generic operation needs a backend-specific `if`, stop and add a domain concept or capability instead.

For package-manager families with similar command shapes, a shared backend-layer adapter is acceptable. Do not move the adapter into `operations`.

## Backend Responsibilities

Each backend owns:

- ID and display name;
- category;
- command requirements;
- optional command requirements for installer choices or enhanced features;
- aliases accepted by `--from`;
- capabilities;
- optional probes;
- native query commands;
- parser code;
- plan construction;
- human action descriptions.

Parser code belongs with the backend. Do not create detached parser folders for one backend's output.

## Capability Rules

Advertise only real supported operations:

- `Search`
- `Install`
- `Remove`
- `Update`
- `Upgrade`
- `List`
- `Info`

Unsupported operations must stay explicit. Allp must not emulate dependency resolution, repository editing, alias mapping, or package-manager policy.

## Execution Plans

Mutating methods return `ExecutionPlan`.

Plans include:

- backend ID and name;
- operation kind;
- action label;
- optional package ID;
- source and scope when known;
- package domain and installer choices when relevant;
- absolute executable path;
- argument vector;
- privilege requirement;
- interactivity.

Never concatenate a shell command string. Never add automatic confirmation flags such as `-y` or `--assumeyes`; `--yes` is only an Allp-level final-confirmation bypass.

## Tests And Fixtures

Backend additions should include:

- fixture output for every parser;
- fake-PATH discovery tests where useful;
- command-plan tests;
- package IDs beginning with `-`;
- JSON output checks for read-only surfaces;
- unsupported-capability behavior.

Fixture directories live under:

```text
tests/fixtures/<backend>/
```

## Development Ecosystems

Development backends require explicit registry/source and installer modeling. Python uses PyPI as source with pip/pipx/uv installers. Node uses the npm registry as source with npm/pnpm/Yarn installers. A development backend must not silently modify project dependencies or lockfiles through generic host-level operations.
