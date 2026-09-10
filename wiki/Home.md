# Allp Wiki

**Allp 0.6.2 · Public alpha · [فارسی](Home.fa.md)**

Allp is a transparent orchestrator for package managers already installed on a
machine. It offers one command surface without hiding where software comes from,
which native command will run, or which privilege boundary is required.

## Documentation map

| Area | Guide |
|---|---|
| First install and safe first run | [Getting started](Getting-Started.md) |
| Every public command and useful flag | [Command reference](Command-Reference.md) |
| APT, Pacman, DNF, Bazzite, Flatpak, Snap, Homebrew, Python, Node and Cargo | [Backends](Backends.md) |
| Portable TOML inventories | [Package profiles](Package-Profiles.md) |
| Scripts, CI and structured output | [Automation and JSON](Automation-and-JSON.md) |
| Runtime layers and extension points | [Architecture](Architecture.md) |
| Trust, sudo and self-update boundaries | [Security](Security.md) |
| Build, test, release and contribution workflow | [Development](Development.md) |
| Symptoms, diagnostics and recovery | [Troubleshooting](Troubleshooting.md) |

## The mental model

```text
request → discover installed managers → query capable backends
        → show choices → build immutable native plan
        → show exact command → confirm → execute directly
```

Allp does not own dependency resolution or a package database. APT, Pacman,
DNF, Flatpak, Snap and the other native tools remain authoritative.

## Safety promise

- Package-manager commands are program-plus-argv values, not shell strings.
- A mutation is planned and displayed before it executes.
- `--dry-run` never performs a mutation or invokes sudo.
- `--yes` skips only Allp's final confirmation; it does not blindly inject
  confirmation flags into native tools.
- Root access is applied to the required child only. User-scoped tools remain
  in the original user's context when Allp was started through sudo.
- Allp collects no telemetry and stores no credentials.

## Canonical engineering references

This wiki explains the product. The repository's detailed contracts remain
authoritative: [architecture](../ARCHITECTURE.md), [command semantics](../docs/COMMANDS.md),
[backend contract](../docs/BACKEND_CONTRACT.md), [JSON schema](../docs/JSON_SCHEMA.md),
and [security model](../docs/SECURITY_MODEL.md).
