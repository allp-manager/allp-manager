# JSON Schema

JSON stdout uses a versioned envelope.

```json
{
  "schema_version": 1,
  "command": "search",
  "complete": true,
  "results": [],
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

Search results include backend identity, package ID, display name, version, source/registry, installer choices, artifact type, scope, description, backend category, package domain, and match kind.

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
