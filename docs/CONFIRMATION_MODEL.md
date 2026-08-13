# Confirmation Model

Allp v0.3.2 requires final Allp-level confirmation for every real mutating operation after all choices are resolved and after the execution plan is rendered.

This applies to install, remove, update, upgrade, project dependency changes, global tool changes, lockfile changes, and environment changes. A single exact result is selection, not execution permission.

Prompt defaults:

- install: `Install this package? [Y/n]`
- remove: `Remove it? [y/N]`
- update: `Continue? [Y/n]`
- risky upgrade: `Continue with upgrade? [y/N]`

`--yes` / `-y` bypasses only this final Allp confirmation. It does not
indiscriminately add native package-manager auto-confirm flags, bypass
ambiguity, auto-select fuzzy registry results, bypass PEP 668, or bypass
ownership and root-safety checks. Operation-specific choices remain explicit:
APT upgrades use `-y`, while metadata refreshes do not.

Dry runs build and show real plans but ask no execution confirmation and execute nothing.
