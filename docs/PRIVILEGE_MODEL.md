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

When Allp is already root, it never adds nested sudo and never claims to be running as a normal user. If `SUDO_USER` is available, user-scoped Homebrew, Python, Node, and Flatpak-user plans are executed as that original user. If no safe original user exists, those plans fail before execution.

Dry runs never invoke sudo, never request passwords, and never execute native installers.

When Allp is launched through sudo and an interactive scope selector is needed, it prints one concise administrator-context notice before search scope selection. It does not ask to use sudo again and root-required system plans never receive nested sudo.

`--yes` bypasses only Allp's final confirmation. It does not add native auto-confirm flags, does not bypass package ambiguity, and does not bypass Python, Node, Homebrew, or original-user safety checks.

When Allp de-escalates a user-scoped plan from sudo, it invokes the native command as the original sudo user and restores that user's HOME from the local passwd database when available.
