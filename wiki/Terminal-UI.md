# Terminal UI and Interaction

[← Wiki home](Home.md) · [فارسی](Terminal-UI.fa.md)

Real interactive `update` and `upgrade` runs can show an inline APT-style
progress footer. Native stdout/stderr remains normal scrollback; the footer
shows percentage when detectable, active backend/action, elapsed time and queue
completion. It is an observer and cannot change planned argv or privilege.

```bash
allp update
allp upgrade
allp update --no-tui     # classic streaming
allp update --no-color   # layout without ANSI color
```

The live view is disabled for JSON, dry-run, redirected/non-TTY output,
`TERM=dumb`, and `--no-interactive`. Terminal control sequences from untrusted
native output are sanitized before UI projection. Width fitting prevents the
footer from wrapping into prompts.

When root children are planned, sudo authentication completes before the live
renderer starts. Credential expiry suspends/clears the footer and revalidates
outside it; failure is classified as a blocked operation.
