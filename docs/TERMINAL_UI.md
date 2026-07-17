# Terminal UI

Human output uses icons and color together:

- green `✔` for success, ready, and completed states;
- red `✖` for errors and failed operations;
- yellow `⚠` for warnings, partial coverage, and risky selections;
- cyan `ℹ` for informational details;
- accent styling for headings and selected/next-action content.

Color is disabled when:

- `--no-color` is passed;
- `NO_COLOR` is set;
- stdout is not a TTY;
- JSON output is requested;
- `TERM=dumb`.

JSON output never contains ANSI escape sequences. Native package-manager output is never repainted or hidden behind animation.

## Search Scope Selector

When an interactive `search` or `install` command has no `--from` and no `--scope`, Allp asks:

```text
Where should Allp search?

[1] Apps and tools
[2] Developer ecosystems
[3] All sources
```

Those are the only initial scope choices. `--scope apps`, `--scope dev`, and `--scope all` select the same flows non-interactively.

## Result Selector

Install result selection uses stable global numbers. When the result set is large, Allp does not dump every candidate at once; it opens a direct terminal selector with:

```text
Space       next page
b           previous page
<number>    direct selection by stable result number
/           filter visible results
q / Esc     cancel
Enter       select the highlighted or first visible result where supported
```

Result numbers remain stable across pages and filters. Non-TTY output, redirected stdin/stdout, and JSON output never start the interactive selector.

## Final Confirmation

Every real mutating operation shows the final execution plan before execution confirmation. The prompts are:

- install: `Install this package? [Y/n]`
- remove: `Remove it? [y/N]`
- update batch: `Continue? [Y/n]`
- riskier upgrade batch: `Continue with upgrade? [y/N]`

Dry runs show plans and summaries but never show execution confirmation prompts.
