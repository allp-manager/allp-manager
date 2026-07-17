# Python Ecosystem

Python support is experimental.

Allp models PyPI as the source/registry and pip, pipx, and uv as installer choices. These selectors narrow to the Python backend:

```bash
allp search openai --from python
allp search openai --scope dev
allp install openai --from pypi --dry-run
allp install openai --scope dev --dry-run
allp install black --from pipx --dry-run
allp install ruff --from uv --dry-run
```

Search currently uses installed Python/pip tooling rather than a direct Allp-owned PyPI client. Results are normalized into package ID, version, description, source, installer choices, artifact type, and scope.

Scopes:

- active virtual environment when `VIRTUAL_ENV` is set;
- current Python environment otherwise;
- isolated CLI install through pipx;
- uv-managed tool install.

When several Python installers are available, Allp asks the user to choose one unless `--from pip`, `--from pipx`, or `--from uv` already selected it.

Allp does not use sudo for Python packages by default and never adds PEP 668 bypass flags such as `--break-system-packages`. Fuzzy or related Python registry matches are never installed silently; they require explicit selection and show registry safety warnings.

If Allp was launched through sudo, Python user-scoped plans require the original sudo user context and must not create root-owned files in that user's home, project, environment, or caches.

## Update And Upgrade

Python participates in `allp update` and `allp upgrade`.

```bash
allp update --from pip --target environment --dry-run
allp update --from pipx --target tools --dry-run
allp update --from uv --target tools --dry-run
allp upgrade --from pipx --target tools --dry-run
allp upgrade --from uv --target tools --dry-run
```

pip environment updates require an active virtual environment. Allp inspects packages with:

```text
python -m pip list --outdated --format=json
```

When outdated packages are selected for the active environment, Allp builds:

```text
python -m pip install --upgrade <packages...>
```

pip does not distinguish compatible update from latest upgrade with a separate bulk command, so the execution plan says when update and upgrade map to the same native operation.

pipx tools use:

```text
pipx upgrade-all
```

uv tools use:

```text
uv tool upgrade --all
```

Skip reasons are target-specific: no active Python environment, externally managed Python environment, backend not installed, no pipx tools, no uv tools, or original user cannot be recovered safely. Allp never adds `--break-system-packages` automatically.
