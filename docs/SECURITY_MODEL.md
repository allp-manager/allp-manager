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

Before a root-required child is launched, Allp canonicalizes its executable and requires the target to be a root-owned executable regular file. Every directory in the canonical path must also be root-owned and not group/world-writable. The canonical path is passed to `sudo --` for ordinary elevation, or `sudo -n --` after a maintenance privilege preflight, or executed directly when Allp is already root; a user-controlled `PATH` entry therefore cannot substitute a package-manager executable at the privilege boundary. This policy is deliberately separate from validation of user-owned executables run while dropping privileges.

Before execution, Allp renders the planned native commands and marks root-required operations. Every real mutating operation asks for final Allp confirmation after the privilege explanation and before any sudo prompt can appear. Maintenance runs then validate sudo once before the live dashboard owns terminal rendering. A later expired credential clears the dashboard footer and is revalidated interactively outside it; failed, cancelled, timed-out, or unavailable revalidation is classified as blocked instead of allowing an interactive prompt inside the UI. In `--no-interactive` mode, real mutation that requires confirmation is refused before native execution unless choices are fully resolved and `--yes` is supplied.

Dry runs never request elevation or execute mutation. Discovery normally executes without elevation; when an already-elevated `sudo allp` process validates a user-scoped Homebrew installation, it may invoke `sudo -H -u <validated-owner>` solely to drop privileges for read-only `brew --version` and `brew --prefix` probes. It never executes Homebrew as root.

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
- Stable release tag/version and manifest identity must agree.
- Continuous candidates must bind to the trusted workflow path, successful main-branch run, exact 40- or 64-hex commit/run identity, and an unexpired non-empty Actions artifact with GitHub SHA-256 digest metadata. The artifact name also contains the SHA-256 of the exact mirrored manifest bytes.
- Asset selection matches platform target; the first arbitrary asset is never used.
- Every binary archive is checked against manifest SHA-256 before extraction.
- Unsafe paths, links, foreign URLs, and staged build-identity mismatch are rejected. Continuous staging verifies display version, commit, build ID, target, channel, and official provenance at initial staging, elevated replacement, and deferred replacement.
- Extracted binary SHA-256 and byte size are carried through every helper boundary and rechecked before diagnostic execution, copying, and replacement. A same-version staged-file substitution is rejected before execution.
- When Allp is effectively root, self-update helpers such as `curl` and `tar` are resolved from fixed system locations first and must canonicalize to root-owned, non-group/world-writable files whose ancestors satisfy the same trust policy; an inherited user-owned PATH hit is rejected.
- Replacement keeps a rollback backup until post-install verification succeeds.
- Temporary response headers use exclusive owner-only files, and Unix update staging uses owner-only directories.
- State files contain channel, legacy ETag metadata, version/build identity, and timestamps only, never credentials. An ETag is not sent without cached verified response data.

## Non-Goals

Allp does not provide:

- telemetry;
- a background daemon;
- package cache ownership;
- universal rollback;
- automatic source recommendation;
- automatic confirmation flags.

`--yes` is an Allp-only final-confirmation bypass. It does not indiscriminately
add native `-y`, `--assumeyes`, or equivalent flags; operation-specific choices
remain explicit in the reviewed plan. APT upgrades use `-y`, while metadata
refreshes do not.

## Alpha Limitations

The alpha still needs broader cross-platform ACL validation, real-host de-escalation validation, and package-registry security review before claiming security hardening. It is not security-audited.
