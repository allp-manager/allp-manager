# Search and Selection

[← Wiki home](Home.md) · [فارسی](Search-and-Selection.fa.md)

Allp searches capable backends concurrently in bounded groups. Each parser
returns candidates plus structured issues; unknown non-empty native output is
`unrecognized_output`, not a fabricated no-match.

## Ranking

1. Exact package/display-name matches.
2. Related matches, capped per backend and selected round-robin.
3. Fuzzy matches, hidden unless `--all` is used.

```bash
allp search git --exact
allp search editor --limit 10
allp search editor --all --limit 50
```

Cross-backend identity is descriptive. Verified canonical mappings may form a
group; probable relationships are labeled uncertain; same-name results remain
separate. Allp never chooses a meaningful source automatically.

## Interactive controls

For large result sets, Space/`b` moves pages, `/` filters, a number chooses the
global result, Enter accepts the highlighted/first visible item, and `q` or
Escape cancels. Filtering never renumbers the original global choices.

## Noninteractive rule

Scripts cannot answer scope or source questions. Provide them explicitly:

```bash
allp search git --scope apps --json
allp install git --from apt --dry-run --no-interactive
allp install git --from apt --no-interactive --yes
```

If multiple choices remain, exit code 4 is returned with recovery guidance.
