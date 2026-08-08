# Privilege Model

Allp should normally be run without sudo:

```bash
allp update
```

Every mutating backend returns an immutable `ExecutionPlan` with a plan-level privilege requirement:

- `NoElevation`: run as the current user.
- `RootRequired`: run as root, using `sudo --` only when Allp itself is not already root.
- `OriginalUserRequired`: run as the invoking user when Allp was started through sudo.
- `Conditional`: reserved for backends whose scope decides at plan time.

Runtime context is detected once per invocation:

- `NormalUser`
- `RootDirect`
- `SudoRootWithOriginalUser`

Normal-user root-required plans are shown first, then Allp explains that only native child commands will be elevated and asks for confirmation before real mutating execution.

Install and remove follow the same confirmation rule as update and upgrade:

1. Select the exact result, installer, and scope when needed.
2. Build the immutable execution plan.
3. Show the native command and privilege behavior.
4. Ask for final confirmation.
5. Execute only after confirmation.

When Allp is already root, it never adds nested sudo and never claims to be running as a normal user. `SUDO_USER`, `SUDO_UID`, and `SUDO_GID` are treated only as claims: Allp resolves the named account in the system account database and rejects inconsistent IDs. If a validated original user is available, user-scoped Homebrew, Python, Node, and Flatpak-user plans are executed with `sudo -H -u <user>` as that original user. The reusable privilege boundary supplies the target user's `HOME`, `USER`, `LOGNAME`, `PATH`, `SHELL`, and XDG directories so user-scoped package managers never inherit root's home. If no safe original user exists, those plans fail before execution.

Homebrew discovery uses the same boundary for `brew --version` and `brew --prefix`; discovery may inspect candidate files while elevated, but it never executes Homebrew as root. The selected executable owner must match the validated original user under sudo. This shared validated installation is then reused by detect, diagnostics, and package operations.

Owner-sensitive probes use `UserContextExecutor`, which accepts an exact `UserAccount` containing name, UID, GID, home, and shell. It revalidates that tuple against the account database at process preparation time, rejects root as a target, and either runs with a cleared environment as the already-matching user or de-escalates through `sudo -H -u <validated-owner> -- /usr/bin/env -i ...`. Only deterministic user paths, identity variables, XDG directories, `LC_ALL=C`, `LANG=C`, and explicit command-specific overrides are supplied. This prevents a path owner selected during discovery from being replaced by a generic or newly changed `SUDO_USER` at probe time and prevents sudo `env_keep` values such as `BASH_ENV` or `RUBYOPT` from reaching probes.

The generic `OriginalUserRequired` path uses that same validated-account executor whenever Allp is elevated, for captured probes as well as mutating plans. Normal-user Python and Node commands continue to inherit the invoking user's ordinary environment. Allp resolves the privilege helper from fixed `/usr/bin/sudo` and `/bin/sudo` candidates before absolute PATH candidates, canonicalizes it, and accepts it only when the executable and every canonical ancestor are root-owned and not group/world-writable. This prevents a PATH-shadowed helper from collecting credentials or crossing the privilege boundary.

`RootRequired` has a separate, stricter executable policy from user-context de-escalation. Before either `sudo -- <program>` or already-root direct execution, Allp canonicalizes the planned program and requires the canonical target to be a root-owned executable regular file whose every ancestor is a root-owned directory without group or world write permission. The canonical path is the path actually executed, so a package manager found through a user-controlled `PATH` cannot cross the root boundary. Debug integration builds have an explicit `ALLP_TEST_SUDO_EXECUTABLE`-gated fake-executable boundary; that boundary is not compiled into release builds.

Dry runs never invoke sudo, never request passwords, and never execute native installers.

When Allp is launched through sudo and an interactive scope selector is needed, it prints one concise administrator-context notice before search scope selection. It does not ask to use sudo again and root-required system plans never receive nested sudo.

`--yes` bypasses only Allp's final confirmation. It does not add native auto-confirm flags, does not bypass package ambiguity, and does not bypass Python, Node, Homebrew, or original-user safety checks.

Prerequisite installation, service activation, and remote/repository setup are separate mutations. `--yes` alone cannot approve them in non-interactive mode; `--allow-bootstrap` is also required after the complete plan is displayed.

Self-update determines writability from effective UID/group permissions, not merely the presence of any write bit. Writable Linux/macOS installs replace through same-directory staging. Non-writable or root-owned installs display a minimal internal replacement plan and elevate only that helper, preserving destination mode and ownership. Windows replacement is deferred until the running process exits.

When Allp de-escalates a user-scoped plan from sudo, it invokes the native command as the original sudo user and restores that user's HOME from the local passwd database when available.
