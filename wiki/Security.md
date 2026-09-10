# Security Model

[← Wiki home](Home.md) · [فارسی](Security.fa.md)

Allp coordinates tools that may change an operating system. Its primary safety
property is a visible plan with a narrow execution boundary—not a claim that
third-party packages are trustworthy.

## Command and privilege boundary

- Native program and arguments are stored separately and executed without a shell.
- Package IDs beginning with `-` are rejected before mutation.
- Root-required executables and their parent directories are canonicalized and
  checked for root ownership and unsafe write permissions.
- Normal use starts as a user. Only the required child is wrapped with `sudo --`.
- Maintenance validates once with `sudo -v`, then uses `sudo -n --` so a password
  prompt cannot corrupt the live UI.
- User-scoped Homebrew, Python, Node, Cargo and Flatpak work is de-escalated to
  the validated original sudo user. Direct-root user-scoped work is refused if
  no original user can be established.

## Confirmation model

```bash
allp install git --from apt --dry-run  # zero mutation, zero sudo
allp install git --from apt            # plan → confirmation → execution
allp install git --from apt --yes      # only skips Allp's final confirmation
```

Bootstrap is a separate mutation. Unattended bootstrap requires both `--yes`
and `--allow-bootstrap` after its exact plan is printed.

## Registry and output trust

Package-manager output is untrusted data. Python, Node and Rust registry names
are not treated as official, and fuzzy matches are not auto-installed. Cargo
build scripts or installer hooks may execute during a real native install, but
never during Allp dry-run.

## Self-update trust chain

Allp accepts only its compiled official repository identity. HTTPS requests are
bounded. Manifest identity, target, archive name, size and SHA-256 are checked;
unsafe paths and links are rejected. The staged binary's build identity and
bytes are rechecked across helper boundaries. Replacement retains a rollback
backup until post-install verification succeeds. A dpkg/rpm/Pacman-owned Allp
binary is handed back to its native package source and is not overwritten.

## Privacy and reporting

Allp has no telemetry, daemon, stored sudo password or credential store. Report
command injection, privilege escalation, unsafe resolution, credential leakage
or misleading JSON through the repository's private security advisory channel,
not a public issue.

The alpha is not security-audited. Read [SECURITY.md](../SECURITY.md) and the
[normative security model](../docs/SECURITY_MODEL.md) before production use.
