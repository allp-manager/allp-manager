# Command Semantics

All commands perform fresh lightweight discovery before doing work.

## Canonical Shape

```text
allp <command> [arguments] [options]
```

Do document command-first examples:

```bash
allp update --dry-run
allp install git --from apt --dry-run
```

Do not present mutation flags before the command as the primary form.

## `detect`

```bash
allp detect
allp detect --json
allp detect --verbose
```

Shows every built-in backend, detection state, resolved executable paths, and capability names.

## `search <query>`

```bash
allp search git
allp search git --from apt
allp search git --from brew
allp search openai --from python
allp search typescript --from node
allp search git --exact
allp search git --limit 10
allp search git --all
allp search git --json
```

Search uses detected backends with `Search` capability. Backends produce normalized candidates; generic search assigns `Exact`, `Related`, or `Fuzzy`.

Default policy:

- show all exact matches;
- show at most five related matches per backend;
- show at most 25 visible results;
- hide fuzzy matches unless `--all` is used.

Backend failures are reported as issues. A completed search can still return zero results.

Without `--from`, search may span system packages, universal applications, Homebrew, Python packages, and Node packages. Matching names across ecosystems do not imply the same software.

## `install <query>`

```bash
allp install git
allp install git --from apt
allp install git --from apt --dry-run
allp install black --from pipx --dry-run
allp install typescript --from pnpm --dry-run
allp install git --no-interactive
allp install git --from apt --yes
allp install pycharm --from snap --dry-run
```

Flow:

1. discover;
2. search eligible `Search + Install` backends;
3. rank candidates;
4. show meaningful choices;
5. construct an immutable execution plan;
6. print the exact native command;
7. execute unless `--dry-run` is set.

Related matches are kept visible when multiple backends have meaningful candidates. Allp does not assume similar names represent the same software.

When exactly one strong candidate remains and every eligible backend completed successfully, Allp may select that package for planning. Selection is not execution permission: real installs still render the execution plan and ask `Install this package? [Y/n]` unless `--yes` is supplied.

Python and Node registry packages show source/registry separately from installer choices. Fuzzy Python or Node matches are never installed automatically.

Snap install planning is metadata-gated:

1. search results come from `snap find`;
2. the selected candidate is revalidated with `snap info`;
3. Allp uses the canonical `name`, not the display title;
4. publisher decorations such as `**` are stored as verification state;
5. architecture, stable channel, confinement, and installed state are inspected;
6. classic confinement adds `--classic`;
7. stale or unavailable results fail before any install child process starts.

If a Snap has no stable channel, or multiple stable tracks without a safe default, Allp refuses to silently choose candidate, beta, edge, or an arbitrary track.

## `remove <query>`

```bash
allp remove git
allp remove git --from apt
allp remove git --from apt --dry-run
```

Remove queries installed inventories only. It does not search remote repositories first. If multiple installed copies match, the user chooses the backend-owned installation to remove.

## `update`

```bash
allp update
allp update --from apt
allp update --dry-run
allp update --dry-run --json
allp update --scope dev --target all --dry-run
allp update --from npm --target project --dry-run
allp update --from npm --target global --dry-run
allp update --from pip --target environment --dry-run
allp update --from pipx --target tools --dry-run
allp update --from uv --target tools --dry-run
```

Runs each detected backend's declared update action. Semantics are backend-owned and shown in the action label.

Examples:

- APT refreshes package metadata.
- DNF refreshes metadata cache.
- Snap refreshes installed snaps.
- Flatpak updates installed apps/runtimes.
- Pacman does not advertise APT-style `Update`.
- npm project update uses `npm update` after `npm outdated --json` inspection.
- npm global update uses `npm update --global` after global outdated inspection.
- pnpm project/workspace/global update uses native `pnpm update` forms.
- Yarn update detects the Yarn major version and uses Yarn 1 or modern Yarn commands.
- pip active-environment update inspects `python -m pip list --outdated --format=json`.
- pipx tools update uses `pipx upgrade-all`.
- uv tools update uses `uv tool upgrade --all`.

Mutating backend operations run sequentially and continue after failures. Any failure returns exit code `8`.

For real execution, Allp first renders the complete plan, explains child-only privilege elevation for root-required plans, and prompts once for the batch. `--no-interactive` cannot provide that confirmation; use `--dry-run`, run interactively, or provide fully resolved choices with `--yes`.

## `upgrade`

```bash
allp upgrade
allp upgrade --from apt
allp upgrade --dry-run
allp upgrade --dry-run --json
allp upgrade --scope dev --target all --dry-run
allp upgrade --from pnpm --target project --dry-run
allp upgrade --from yarn --target project --dry-run
```

Runs each detected backend's declared bulk-upgrade action. Unsupported backends are reported as skipped; Allp does not invent upgrade-all behavior.

Upgrade prompts default to No for riskier batches because they may cross constraints, alter manifests, update lockfiles, or change application behavior.

## `list`

```bash
allp list
allp list --from apt
allp list --from apt --filter git
allp list --from apt --limit 50
allp list --from apt --no-pager
allp list --json
```

Lists installed packages grouped by backend. Filtering happens before limiting. Large human-readable output is paged automatically for interactive terminals via `$PAGER`, `less -FRSX`, or `more`; redirected output, JSON, `--no-pager`, and small result sets print directly.

## `info <query>`

```bash
allp info git
allp info git --from apt
allp info git --from apt --full
allp info git --from apt --raw
allp info git --json
```

First inspects installed inventories. If no installed match exists, it queries remote information through searchable backends.

Default info output is curated: backend, package ID, display name, version, installed state, important normalized fields such as architecture/homepage when available, source, scope, artifact type, and description.

`--full` includes normalized extended metadata. `--raw` prints native backend info output when supported. `--json` returns the structured normalized model.
