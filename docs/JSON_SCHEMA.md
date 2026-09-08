# JSON Schema

JSON stdout uses a versioned envelope.

```json
{
  "schema_version": 2,
  "command": "search",
  "complete": true,
  "results": {
    "query": "firefox",
    "effective_scope": "apps_and_tools",
    "candidates": [],
    "groups": [],
    "backends": []
  },
  "issues": []
}
```

## Commands

Required JSON surfaces:

- `allp detect --json`
- `allp search git --json`
- `allp list --json`
- `allp info git --json`
- `allp update --dry-run --json`
- `allp upgrade --dry-run --json`

Human logs, spinners, and prompts must not be written to JSON stdout.

## Envelope Fields

| Field | Type | Meaning |
|---|---|---|
| `schema_version` | number | JSON contract version |
| `command` | string | command name |
| `complete` | boolean | false when one or more eligible backends failed |
| `results` | array or object | command-specific result payload |
| `issues` | array | backend or operation issues |

## Search Result Fields

Search schema v2 uses an object payload. `effective_scope` records the scope
actually searched; JSON and other noninteractive calls without `--from` or
`--scope` default to `apps_and_tools`. `candidates` includes backend identity,
package ID, display name, version, source/registry, installer choices, artifact
type, scope, description, backend category, package domain, and match kind.
`groups` exposes stable one-based `selection_numbers`, canonical identity when
known, and confidence. `backends` exposes per-backend result count and parser
state, including `partial_results` and `unrecognized_output`.

Issues include `kind`, optional `stage`, backend identity, and a human-readable
message. Any partial or unrecognized parser output sets envelope `complete` to
`false` while preserving candidates that were parsed safely.

`match_kind` serializes as:

- `exact`
- `related`
- `fuzzy`

## Maintenance Dry Run

`update --dry-run --json` and `upgrade --dry-run --json` return operation records with:

- backend identity;
- action;
- command;
- status;
- message when present.

Dry-run records use status `dry_run`.

Maintenance envelopes also include:

- `requires_confirmation`
- `confirmation_bypassed`
- `targets`
- `plans`
- `skips`

Execution-plan JSON includes the rendered native command and human privilege label. Human labels are stable for alpha UX, while `schema_version` is the compatibility boundary for automation.
