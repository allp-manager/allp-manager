# Node Ecosystem

Node support is experimental.

Allp models the npm registry as the source and npm, pnpm, and Yarn as installer choices. These selectors narrow to the Node backend:

```bash
allp search typescript --from node
allp search typescript --scope dev
allp install typescript --from npm --dry-run
allp install typescript --scope dev --dry-run
allp install typescript --from pnpm --dry-run
allp install typescript --from yarn --dry-run
```

A registry package is shown once with available installers instead of as separate npm/pnpm/Yarn products.

Default install plans use global user-tool style commands:

- `npm install --global <package>`
- `pnpm add --global <package>`
- `yarn global add <package>`

When npm, pnpm, and/or Yarn are available for the same registry package, Allp asks the user to choose one unless `--from npm`, `--from pnpm`, or `--from yarn` already selected it.

Project-scope dependency modification is not silently performed. Allp does not use sudo for Node packages by default. Fuzzy or related Node registry matches are never installed silently, registry safety warnings are shown, and dry runs never execute lifecycle scripts.

If Allp was launched through sudo, Node user or project plans require the original sudo user context and must not create root-owned files in that user's home, project, caches, or lockfiles.

## Update And Upgrade

Node participates in `allp update` and `allp upgrade`.

```bash
allp update --from npm --target project --dry-run
allp update --from npm --target global --dry-run
allp update --from pnpm --target project --dry-run
allp update --from pnpm --target workspace --dry-run
allp update --from yarn --target project --dry-run
allp upgrade --from pnpm --target project --dry-run
allp upgrade --from yarn --target project --dry-run
```

npm project targets are inspected with:

```text
npm outdated --json
```

and planned with:

```text
npm update
```

npm global targets are inspected with:

```text
npm outdated --global --depth=0 --json
```

and planned with:

```text
npm update --global
```

Allp never generates `npx update`.

pnpm project update uses `pnpm update`; latest upgrade uses `pnpm update --latest`. pnpm global update uses `pnpm update --global`; latest global upgrade adds `--latest`. Workspace targets are explicit and show workspace manifests plus `pnpm-lock.yaml` as affected files.

Yarn major version is detected before planning. Yarn 1 uses `yarn upgrade` and `yarn upgrade --latest`; modern Yarn uses version-appropriate `yarn up` plans. Affected manifests and lockfiles are shown in the target field.

Skip reasons are target-specific: no project manifest, no workspace manifest, backend not installed, no globally outdated npm packages, unsupported Yarn global capability, or original user cannot be recovered safely.
