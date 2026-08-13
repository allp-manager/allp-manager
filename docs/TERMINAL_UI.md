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

JSON output never contains ANSI escape sequences. In the classic stream, native
package-manager output is printed directly. The live maintenance dashboard
described below keeps that output visible while adding only a safe terminal
projection around it; it never rewrites the planned native command, its
arguments, or its plan-level privilege requirement.

## Live Maintenance Dashboard

Real, interactive `update` and `upgrade` execution uses an inline live
dashboard during the execution phase. It is deliberately **not** a full-screen
or alternate-screen application: the terminal keeps its ordinary scrollback and
Ctrl+C does not need to restore terminal mode. Administrator authentication is
completed before the dashboard starts; no sudo password prompt is permitted
while it is rendering.

![Illustrative live update dashboard](assets/tui-maintenance.svg)

The dashboard has three parts:

- Native stdout and stderr continue to scroll in the normal terminal buffer.
  Stdout lines use a subtle `›` marker and stderr lines use a warning `!`
  marker so their origin stays clear.
- A boxed card is emitted when an operation starts and when it resolves. Failed
  operations use the error color; deferred, protected, and busy results use a
  warning color; successful results use the success color. The final summary is
  also a card, so an error never disappears into a long package-manager log.
- The single footer line at the bottom is redrawn in place. It shows the active
  backend, exact action label, elapsed time, and `Queue: completed/total`.
  The bar advances only as queued operations complete; it does not treat elapsed
  time as package progress. A metadata refresh can discover a follow-up upgrade,
  in which case the queue and footer total grow visibly instead of pretending
  the original count was final.

The dashboard starts only for a real maintenance run when all of the following
are true:

- human (non-JSON) output is selected;
- stdin, stdout, and stderr are terminals;
- `TERM` is not `dumb`;
- the command is interactive (not `--no-interactive`); and
- `--no-tui` was not supplied.

Dry runs retain their complete plan and normal summary rather than starting a
live dashboard. Redirected output, JSON, non-interactive maintenance, and
`TERM=dumb` retain the established classic stream so scripts keep a stable
contract. `--no-color` only removes color: it does not disable the dashboard.

To force the classic stream in an interactive terminal, use:

```bash
allp update --no-tui
allp upgrade --no-tui
```

### Execution and safety boundary

The process runner remains the only component that prepares a native command,
applies the plan's privilege boundary, and starts the child process. The
dashboard is an observer: it receives output and timing events after that work
has been decided and cannot rewrite, approve, or elevate a command.

If any selected operation needs administrator access, Allp performs one
interactive `sudo -v` preflight after the final confirmation and before the
dashboard is created. Its standard streams stay attached to the real terminal;
Allp never reads or stores a password. During dashboard execution, privileged
children use `sudo -n -- …`. Before a later root operation, Allp checks the
cached credential with `sudo -n -v`. If it has expired, the footer is cleared,
an interactive `sudo -v` owns the normal terminal, and the footer is redrawn
only after authentication succeeds. A failed or unavailable revalidation
produces a structured blocked result rather than mixing a password prompt into
the dashboard output.

For safety, the on-screen log is a terminal-safe projection of untrusted native
output: control sequences are removed before display. The runner keeps the
captured native result for status classification, while the dashboard only
renders readable text. If dashboard output itself fails, it relinquishes the
stream and the runner falls back to ordinary stdout/stderr forwarding without
interrupting the package-manager operation.

The running card's command preview is a readable rendering of the validated
plan. It is not a promise that the displayed text is a byte-for-byte shell
wrapper: for example, an original-user Homebrew plan may be launched through a
validated `sudo -H -u` boundary with a sanitized `env -i` environment. The
native executable, arguments, plan privilege, and safety checks remain the
ones that were confirmed before execution.

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
