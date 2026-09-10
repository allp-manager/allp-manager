# Automation and JSON

[← Wiki home](Home.md) · [فارسی](Automation-and-JSON.fa.md)

Use explicit scopes and backends in unattended code. JSON is supported for
read-only commands and maintenance dry runs; human prompts, colors and spinners
are never mixed into JSON stdout.

```bash
allp detect --json
allp search git --from apt --json
allp list --from flatpak --json
allp info git --from apt --json
allp update --dry-run --json
allp upgrade --dry-run --json
```

## Envelope

```json
{
  "schema_version": 2,
  "command": "search",
  "complete": true,
  "results": {
    "query": "git",
    "effective_scope": "apps_and_tools",
    "candidates": [],
    "groups": [],
    "backends": []
  },
  "issues": []
}
```

Treat `schema_version` as the compatibility boundary and `complete` as a data
quality signal. `complete: false` can coexist with useful candidates when one
backend returned partial or unrecognized output; inspect `issues` before acting.

## Safe shell example

```bash
report="$(mktemp)"
if allp search git --from apt --json >"$report"; then
  jq -e '.schema_version == 2 and .complete == true' "$report" >/dev/null
  jq '.results.candidates[] | {backend_id, package_id, match_kind}' "$report"
fi
rm -f "$report"
```

For mutation automation, first archive a dry-run report, then use a fully
resolved backend/package with `--no-interactive --yes`. Never assume that
`--yes` approves source selection or prerequisite bootstrap.

```bash
allp install git --from apt --dry-run
allp install git --from apt --no-interactive --yes
```

Bootstrap additionally requires `--allow-bootstrap`. Exit code `8` means a
multi-backend batch partially failed; do not treat it as total success.

See [JSON_SCHEMA.md](../docs/JSON_SCHEMA.md) for every field.
