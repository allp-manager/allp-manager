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

## Bootstrap And Remotes

Installing an executable, enabling a service, adding a remote, changing configuration, and elevating privilege are separate plans. No prerequisite is installed silently. `--yes --allow-bootstrap` is required for non-interactive bootstrap; exact commands remain visible.

## Self-Update

- Repository identity is a trusted constant, not a user URL.
- Metadata and downloads are HTTPS-only with bounded time, redirects, and size.
- Release tag/version and manifest identity must agree.
- Asset selection matches platform target; the first arbitrary asset is never used.
- Every binary archive is checked against manifest SHA-256 before extraction.
- Unsafe paths, links, foreign URLs, and staged-version mismatch are rejected.
- Replacement keeps a rollback backup until post-install verification succeeds.
- State files contain channel/ETag/version timestamps only, never credentials.

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
