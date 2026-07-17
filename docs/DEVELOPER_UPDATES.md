# Developer Updates

Allp v0.3.2 includes Python and Node targets in `update` and `upgrade`.

Supported target values:

- `project`
- `workspace`
- `global`
- `environment`
- `tools`
- `all`

Node:

- npm project inspection: `npm outdated --json`
- npm project update: `npm update`
- npm global inspection: `npm outdated --global --depth=0 --json`
- npm global update: `npm update --global`
- pnpm project update: `pnpm update`
- pnpm latest upgrade: `pnpm update --latest`
- Yarn detects major version before planning and uses Yarn 1 or modern Yarn commands.
- Allp never generates `npx update`.

Python:

- pip active-environment inspection: `python -m pip list --outdated --format=json`
- pip execution: `python -m pip install --upgrade <packages...>`
- pipx tools: `pipx upgrade-all`
- uv tools: `uv tool upgrade --all`
- Allp never uses sudo for original-user Python tools and never adds `--break-system-packages`.

Every skipped target must include a precise reason such as backend not installed, no project manifest, no active Python environment, externally managed Python environment, no tools found, unsupported detected backend version, or original user cannot be recovered safely.
