# Command Reference

[← Wiki home](Home.md) · [فارسی](Command-Reference.fa.md)

Canonical syntax is `allp <command> [arguments] [options]`. Add `-v` (repeatable)
for diagnostics. Common output flags are `--json`, `--no-color`, and `--no-tui`.

| Command | Purpose | Example |
|---|---|---|
| `detect` | Report every built-in backend and its state | `allp detect --json` |
| `search <query>` | Search eligible sources | `allp search firefox --scope apps` |
| `install <package>` | Resolve, plan, confirm and install | `allp install git --from apt --dry-run` |
| `remove <package>` | Find installed copies and remove one | `allp remove git --from apt` |
| `update` | Refresh metadata and optionally self-update | `allp update --dry-run` |
| `upgrade` | Upgrade installed software | `allp upgrade --from flatpak` |
| `list` | List installed packages by backend | `allp list --from apt --filter git` |
| `info <package>` | Show curated or native metadata | `allp info git --full` |
| `doctor [backend]` | Diagnose platform or one backend | `allp doctor homebrew` |
| `profile` | Save, inspect, import/export and apply inventories | `allp profile save dev` |
| `self-update` | Verify and install an official Allp build | `allp self-update --check-only` |

## Search and source selection

```bash
allp search ripgrep --exact
allp search editor --all --limit 50
allp search black --scope dev
allp search pycharm --from snap
```

The default human view contains exact and bounded related results. `--all`
includes weak fuzzy matches. Noninteractive/JSON requests default to the apps
scope unless `--scope` or `--from` is explicit.

## Mutations

```bash
allp install org.mozilla.firefox --from flatpak --dry-run
allp install black --from pipx --no-interactive --yes
allp remove ripgrep --from cargo --dry-run
```

Important mutation flags:

- `--dry-run`: plan and validate but do not execute.
- `--no-interactive`: never display a selector or prompt.
- `--yes`: bypass only the final Allp confirmation after choices are resolved.
- `--allow-bootstrap`: permit a separately planned prerequisite when combined
  with `--yes` in unattended use.

## Maintenance

```bash
allp update --skip-self-update
allp update --self-only
allp update --check-only
allp update --offline
allp update --scope dev --target tools --dry-run
allp upgrade --scope dev --target all --dry-run
allp upgrade --allow-stale-metadata  # explicit recovery only
```

Developer targets are `project`, `workspace`, `global`, `environment`, `tools`,
and `all`. Unsupported target/backend combinations are reported, never guessed.

## Inventory and information

```bash
allp list --from apt --filter openssl --limit 20 --no-pager
allp info firefox --from flatpak
allp info git --full
allp info git --from apt --raw
```

`--raw` asks the backend for native output. `--full` expands normalized fields.

## Stable exit codes

| Code | Meaning |
|---:|---|
| 0 | Success |
| 2 | Invalid CLI or input |
| 3 | Package not found |
| 4 | Ambiguous/noninteractive selection required |
| 5 | Backend not detected or not configured |
| 6 | Unsupported operation |
| 7 | Native command or validation failed |
| 8 | Partial multi-backend failure |
| 9 | Timeout or cancellation |
| 10 | Internal/parse/I/O error |
| 11 | Backend busy or native lock held |

See the [canonical command contract](../docs/COMMANDS.md) for backend-specific
maintenance semantics.
