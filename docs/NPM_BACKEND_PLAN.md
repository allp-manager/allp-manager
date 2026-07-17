# npm Backend Plan

This historical plan shows how the architecture was expected to accept npm without changing generic operations. The current implementation has superseded the one-backend npm plan with an experimental Node ecosystem backend documented in [NODE_ECOSYSTEM.md](NODE_ECOSYSTEM.md).

## Files to change

```text
src/backends/development/node.rs         implemented adapter
src/backends/development/mod.rs          one module declaration
src/backends/catalog.rs                  one registration line
tests/fixtures/npm/*                     native output fixtures
```

No changes should be required in:

```text
src/operations/search.rs
src/operations/install.rs
src/operations/remove.rs
src/operations/update.rs
src/operations/upgrade.rs
src/operations/list.rs
src/operations/info.rs
src/execution/*
src/cli/*
```

## v0.3.2 capability scope

The current Node ecosystem backend represents npm registry packages with npm, pnpm, and Yarn installer/update choices. Implemented capabilities:

```text
Search
Install
Remove
List
Info
Update
Upgrade
```

Project and global update behavior is documented in [NODE_ECOSYSTEM.md](NODE_ECOSYSTEM.md). Allp never runs project-local npm update operations through a generic host upgrade command; the backend must inspect and plan Node-specific targets explicitly.

## Candidate mapping

An npm search result maps to the shared model:

```text
backend_id      node
category        development
package_id      exact registry package name
display_name    package name
version         latest registry version
source          npm registry
installers      npm, pnpm, Yarn when detected
artifact_kind   development package or CLI tool when known
scope           global
match_kind      exact, related, or fuzzy after generic ranking
```

## Execution planning

Install planning should return a native command equivalent to:

```text
npm install --global <package-id>
pnpm add --global <package-id>
yarn global add <package-id>
```

The backend returns an argument vector. It does not execute the command and does not create a shell string.

## Detection

The backend declares npm as the required registry client and pnpm/Yarn as optional installer choices:

```text
npm  → npm
pnpm → pnpm
yarn → yarn | yarnpkg
```

The existing detector resolves it from the current user's PATH on every Allp execution.

## Remaining open questions

- Should a package be labelled as a CLI tool only when a `bin` entry exists?
- How should custom registries and authentication be displayed without exposing secrets?
- What is the correct global install location for the current user?
- How should Corepack-managed clients be detected?
- Which query output is stable enough for parsing?
- Should project-local installs be completely unsupported until an explicit project scope exists?

These questions belong inside the npm adapter and development-scope model. They must not add npm-specific conditions to generic operations.
