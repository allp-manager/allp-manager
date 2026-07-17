# CLI Contract

Official shape:

```text
allp <command> [arguments] [options]
```

Root-level behavior is limited to help and version.

## Commands

```bash
allp detect
allp detect --json
allp detect --verbose

allp search git
allp search git --from apt
allp search git --scope apps
allp search openai --scope dev
allp search chatgpt --scope all
allp search git --exact
allp search git --limit 10
allp search git --all
allp search git --json

allp install git
allp install git --from apt
allp install git --scope apps
allp install openai --scope dev
allp install chatgpt --scope all
allp install git --from apt --dry-run
allp install git --from apt --yes
allp install git --no-interactive
allp install pycharm --from snap --dry-run

allp remove git
allp remove git --from apt
allp remove git --from apt --dry-run

allp update
allp update --from apt
allp update --dry-run
allp update --scope dev --target all --dry-run
allp update --from npm --target project --dry-run
allp update --from pipx --target tools --dry-run

allp upgrade
allp upgrade --from apt
allp upgrade --dry-run
allp upgrade --scope dev --target all --dry-run

allp list
allp list --from apt
allp list --from apt --filter git
allp list --from apt --limit 50
allp list --from apt --no-pager
allp list --json

allp info git
allp info git --from apt
allp info git --from apt --full
allp info git --from apt --raw
allp info git --json

allp search openai --from python
allp install black --from pipx --dry-run
allp search typescript --from node
allp install typescript --from pnpm --dry-run
allp search git --from brew
```

`--from` accepts backend IDs and documented aliases. Examples: `python`, `pypi`, `pip`, `pipx`, `uv`, `node`, `npm`, `pnpm`, `yarn`, `brew`, `homebrew`, and `linuxbrew`.

`--scope` is a broad category selector:

- `apps`: Apps and tools. Searches system package managers, universal application managers, and Homebrew formula/cask sources.
- `dev`: Developer ecosystems. Searches Python/PyPI and Node.js/npm-registry sources.
- `all`: All sources. Displays results in this order: System Packages, Universal Applications, Developer Ecosystems.

`--from` is more precise than `--scope` and selects a backend, source, ecosystem, or installer. Incompatible combinations, such as `--scope dev --from apt`, are rejected with a CLI error. Without `--from` or `--scope`, interactive search and install ask the user to choose exactly one of: Apps and tools, Developer ecosystems, All sources.

Allp never silently chooses between meaningful candidates across ecosystems.

Snap install candidates have an additional validation step after selection. A raw `snap find` row is never an install plan. Allp revalidates with `snap info`, resolves the canonical package name, publisher verification, confinement, architecture availability, channels, stable availability, and installed state, then builds the immutable plan. Classic snaps include `--classic` only when metadata requires it.

For install selection, every result has a stable global number. Large interactive result sets use the built-in selector:

```text
Space       next page
b           previous page
<number>    select that stable result number
/           filter
q / Esc     cancel
Enter       select the highlighted/first visible result where supported
```

Non-TTY and JSON output never start the interactive selector.

Mutating commands support `--yes` / `-y`. This bypasses only Allp's final confirmation prompt after all package, installer, scope, and target choices are resolved. It never adds native auto-confirm flags and never bypasses ambiguity, PEP 668, Homebrew root protection, ownership checks, or registry safety checks.

Development maintenance supports `--target`:

- `project`
- `workspace`
- `global`
- `environment`
- `tools`
- `all`

## Stable Exit Codes

```text
0   Success
2   Invalid CLI or input
3   Package not found
4   Ambiguous selection / source required
5   Requested backend not detected
6   Unsupported operation
7   Native command failed
8   Partial multi-backend failure
9   Timeout or cancellation
10  Internal error
11  Package manager busy
```

## Error UX

Errors should include recovery steps, such as:

- run `allp detect --verbose`;
- use `--from <backend>`;
- use an exact package ID;
- retry with `allp search <query> --all`.
