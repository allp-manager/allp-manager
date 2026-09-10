# Configuration and Data

[← Wiki home](Home.md) · [فارسی](Configuration-and-Data.fa.md)

Allp follows platform data-directory conventions. On Linux the effective paths
normally resolve under `~/.config/allp`, `~/.local/state/allp`, and
`~/.cache/allp`. Never assume a path in automation; read `allp doctor` because
platform and environment can change it.

| Data | Purpose | Credentials? |
|---|---|---|
| Config | Profiles and explicit tool configuration | No |
| State | Update channel/provenance, ETag metadata, validated locator state | No |
| Cache | Bounded self-update staging and temporary artifacts | No |

Profiles are in the config directory's `profiles/*.toml`. State writes are
atomic. Homebrew locator records are revalidated before use; a stale executable
path is rejected and discovery continues.

`allp doctor` prints effective paths, executable ownership/writability, current
and original users, and release target without dumping unrelated environment
variables or tokens.

Environment variables prefixed `ALLP_TEST_` and internal continuation variables
are implementation/test boundaries, not public configuration. Prefer documented
CLI flags and profiles for user-facing behavior.
