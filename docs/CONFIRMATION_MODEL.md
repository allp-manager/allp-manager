# Confirmation Model

Allp v0.3.2 requires final Allp-level confirmation for every real mutating operation after all choices are resolved and after the execution plan is rendered.

This applies to install, remove, update, upgrade, project dependency changes, global tool changes, lockfile changes, and environment changes. A single exact result is selection, not execution permission.

Prompt defaults:

- install: `Install this package? [Y/n]`
- remove: `Remove it? [y/N]`
- update: `Continue? [Y/n]`
- risky upgrade: `Continue with upgrade? [y/N]`

`--yes` / `-y` bypasses only this final Allp confirmation. It never adds native package-manager auto-confirm flags, bypasses ambiguity, auto-selects fuzzy registry results, bypasses PEP 668, or bypasses ownership and root-safety checks.

Dry runs build and show real plans but ask no execution confirmation and execute nothing.
