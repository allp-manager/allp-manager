# Security Model

Allp executes native package-manager commands. The security model is intentionally conservative.

## Command Execution

- Commands are represented as executable path plus argument vector.
- Allp does not execute through `sh -c`, `bash -c`, or shell interpolation.
- Rendered command strings are for display only.
- Package IDs beginning with `-` are rejected before native mutation.

## Privilege

Run Allp as a normal user:

```bash
allp update
```

Allp elevates only the child command whose plan declares `RootRequired`.

Before execution, Allp renders the planned native commands and marks root-required operations. Every real mutating operation asks for final Allp confirmation after the privilege explanation and before any sudo prompt can appear. In `--no-interactive` mode, real mutation that requires confirmation is refused before native execution unless choices are fully resolved and `--yes` is supplied.

Discovery and dry runs never invoke sudo.

When Allp is already root, it does not add nested sudo. When Allp was launched through sudo and `SUDO_USER` is available, plans marked `OriginalUserRequired` run as that original user. This protects Homebrew prefixes, Python environments, Node projects, Flatpak user installations, and user caches from root ownership.

Direct-root user-scoped operations are refused when no original user can be established.

Python and Node registry packages may be malicious or abandoned. Allp does not infer official status, does not automatically install fuzzy registry matches, and does not run installer hooks during dry run.

## Native Output

Mutating native stdin, stdout, and stderr are inherited directly. Allp does not repaint package-manager transactions.

## Non-Goals

Allp does not provide:

- telemetry;
- a background daemon;
- package cache ownership;
- universal rollback;
- automatic source recommendation;
- automatic confirmation flags.

`--yes` is an Allp-only final-confirmation bypass. It never adds native `-y`, `--assumeyes`, or equivalent flags.

## Alpha Limitations

The alpha still needs deeper trusted-path validation, real-host de-escalation validation, and package-registry security review before claiming security hardening. It is not security-audited.
