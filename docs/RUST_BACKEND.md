# Rust / Cargo Backend

Allp treats Cargo as a user-scoped development package manager for binary
crates. It does not use host maintenance to edit `Cargo.toml`, project
dependencies, workspaces, or `Cargo.lock`.

## Detection

The backend is ready when `cargo` is executable. `rustc` and
`cargo-install-update` are optional capabilities. Select it with `rust`,
`cargo`, `crates`, `crates.io`, or `rustlang`.

## Native Commands

| Allp operation | Native Cargo command |
|---|---|
| Search | `cargo search <query> --limit 20` |
| Install | `cargo install -- <crate>` |
| Remove | `cargo uninstall -- <crate>` |
| List | `cargo install --list` |
| Info | `cargo info <crate>` |
| Upgrade | `cargo install-update --all` |
| Update | Not advertised; there is no metadata-only Cargo host action |

Upgrade requires the optional community `cargo-update` crate. When it is not
installed, Allp reports the upgrade action as unavailable and performs no
mutation. Install it explicitly if wanted:

```bash
cargo install cargo-update
```

## Scope And Privilege

Cargo install/remove/upgrade plans are `OriginalUserRequired`. If Allp was
started through `sudo`, the execution layer returns the command to the validated
original user rather than creating root-owned files under that user's Cargo
home. Supported maintenance targets are `global`, `tools`, and `all`; project,
workspace, and environment targets are not applicable.

Cargo compiles crates locally and crates can run build scripts. Allp keeps that
warning on candidates and execution plans, shows the exact argv, and never
silently elevates Cargo.

References: [official Cargo command documentation](https://doc.rust-lang.org/cargo/commands/cargo.html)
and the [cargo-update project](https://github.com/nabijaczleweli/cargo-update).
