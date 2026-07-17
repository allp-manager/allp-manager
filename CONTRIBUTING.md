# Contributing To Allp

Thank you for helping build Allp.

## Required Checks

Run the full local suite before opening a pull request:

```bash
make quality
```

Do not claim a check passed unless you ran it.

## Local Release Workflow

Release automation is intentionally local-only:

```bash
make hooks-install
make release-prepare BUMP=patch
```

Commit the prepared files with a subject that begins with `release:`. The
post-commit hook creates only a local annotated tag and files under ignored
`dist/`. It never pushes, publishes a GitHub Release, or uploads assets.
Ordinary commits must not change the version or produce release output.

Use `make release-status` to inspect pending state and
`make release-workflow-test` to exercise the release scripts in temporary Git
repositories.

## Architecture Rules

- Keep backend-specific command flags and parsers inside the backend.
- Generic operations must depend on capabilities, not backend IDs.
- Backends must return execution plans instead of spawning mutating processes.
- Never execute through `sh -c`.
- Never add automatic source recommendations.
- Preserve native terminal output for install, remove, update, and upgrade.
- Add fixtures for every parser change.

## Backend Pull Requests

Include:

- supported command versions;
- supported capabilities;
- real output fixtures;
- installation scope behavior;
- privilege behavior;
- known limitations;
- native commands used for search, list, info, install, remove, update, and upgrade.

## Bug Reports

Please include:

```bash
allp detect --json
allp detect --verbose
```

Also include Linux distribution, package-manager version, command executed, expected result, and actual native output.
