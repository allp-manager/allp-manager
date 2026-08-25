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
package-manager output is printed directly. The live maintenance progress line
described below keeps that output visible while adding only a safe terminal
projection around it; it never rewrites the planned native command, its
arguments, or its plan-level privilege requirement.

## Live Maintenance Progress

Real, interactive `update` and `upgrade` execution uses an inline live
progress line during the execution phase. It is deliberately **not** a
full-screen or alternate-screen application: the terminal keeps its ordinary
scrollback and Ctrl+C does not need to restore terminal mode. Administrator
authentication is completed before progress rendering starts; no sudo password
prompt is permitted while it owns the current line.

The display follows the compact style used by APT:

```text
Progress: [ 42%] [########............] APT · Upgrade packages · 0/1 · 8s
```

Native stdout and stderr continue to scroll without cards or marker prefixes.
The renderer clears only its own unterminated line before native output, a
confirmation, or a sudo prompt is written, then redraws it below that content.
It reads the actual terminal width and truncates its own status text before the
last column so the line cannot wrap and corrupt prompt placement. Percentages
reported by a native package manager are reflected in the bar; otherwise the
bar advances as queued operations complete. The normal maintenance summary is
printed after the live line is removed.

The progress line starts only for a real maintenance run when all of the following
are true:

- human (non-JSON) output is selected;
- stdin, stdout, and stderr are terminals;
- `TERM` is not `dumb`;
- the command is interactive (not `--no-interactive`); and
- `--no-tui` was not supplied.

Dry runs retain their complete plan and normal summary rather than starting live
progress. Redirected output, JSON, non-interactive maintenance, and `TERM=dumb`
retain the established classic stream so scripts keep a stable contract.
`--no-color` only removes color: it does not disable the progress line.

To force the classic stream in an interactive terminal, use:

```bash
allp update --no-tui
allp upgrade --no-tui
```

### Execution and safety boundary

The process runner remains the only component that prepares a native command,
applies the plan's privilege boundary, and starts the child process. The
progress renderer is an observer: it receives output and timing events after
that work has been decided and cannot rewrite, approve, or elevate a command.

If any selected operation needs administrator access, Allp performs one
interactive `sudo -v` preflight after the final confirmation and before the
progress renderer is created. Its standard streams stay attached to the real terminal;
Allp never reads or stores a password. During dashboard execution, privileged
children use `sudo -n -- …`. Before a later root operation, Allp checks the
cached credential with `sudo -n -v`. If it has expired, the footer is cleared,
an interactive `sudo -v` owns the normal terminal, and the footer is redrawn
only after authentication succeeds. A failed or unavailable revalidation
produces a structured blocked result rather than mixing a password prompt into
the progress output.

For safety, native output shown while progress is active is a terminal-safe
projection: control sequences are removed but readable content is not prefixed
or placed in UI cards. If progress output itself fails, it relinquishes the
stream and the runner falls back to ordinary stdout/stderr forwarding without
interrupting the package-manager operation.

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
