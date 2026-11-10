Allp 0.6.5 adds WinGet on Windows and native package management inside Termux. This Stable release also improves Linux maintenance, Python project environments, and recovery after an update.

## Windows and Termux

- **Windows** uses the WinGet community source for search, package details, installed inventory, install, removal, metadata refresh, and upgrade. Use `--from winget` to select it explicitly. WinGet comes from Microsoft's App Installer.
- **Termux** uses the package manager inside the app prefix and runs as the Termux app user. Use `--from termux` or `--from pkg` to select it. The release provides Android binaries for ARM64 and x86-64.

## Package managers

- **APT** now reports packages that were not upgraded even when APT supplies only a count. The installed list skips residual configuration entries, and a failed repository fetch makes the metadata refresh fail visibly.
- **Pacman** uses `pacman -Syu` for a full repository sync and upgrade. On Arch, `allp upgrade` selects Pacman automatically. A standalone `allp update` no longer runs `pacman -Sy`. Package installs retain the repository selected in search.
- **DNF** reads both the older search format and the tab-delimited output from newer DNF5. Search keeps dotted names such as `python3.14` intact.
- **Zypper** reads exact package names, versions, and repositories from the native tables used for search and installed inventory.
- **Flatpak** searches user and system installations separately and keeps the chosen scope, remote, and branch when installing. The upgrade summary names each scope and reports an already current installation as up to date. If a search fails in one scope, the other scope's results and the failure both remain visible.
- **Snap** can use the native CLI with the selected channel when a user-local Allp cannot use its privileged snapd helper. Snap diagnostics handle truncated UTF-8 and malformed chunked responses safely.
- **Homebrew** surfaces failures when listing formulae or casks. Catalog name similarity alone no longer labels a Homebrew result as verified.

## Node, Python, and profiles

- **Node** lists and removes global tools through npm, pnpm, or Yarn Classic, according to the original installer. An unconfigured pnpm global directory does not hide inventory from the other managers.
- **Node project updates** respect the current Git checkout and select one package manager from project metadata. pnpm workspaces are planned once from their root, and projects with no outdated dependencies are skipped. Invalid npm and pnpm reports surface as errors. Local CLI versions and update availability are shown separately. pnpm checks its global configuration and outdated packages before offering an upgrade.
- **Python** searches public PyPI and shows package details even when system Python has no pip. Allp finds the current project's `.venv` and uses its interpreter consistently for installation, inventory, removal, and outdated checks. Isolated CLI tools retain their pipx or uv installer.
- **Profiles** retain the installer for Node and Python packages and the scope, remote, and branch for Flatpak applications. Existing profiles can still be read. Restore selects the recorded package details, and profile and local state writes use atomic replacement.

## CLI and update reliability

- Unattended installation requires an exact package match. Unattended install and removal surface incomplete backend results before acting on them.
- Native command capture uses a bounded queue and reports oversized probe output instead of treating a partial response as complete. Timed-out Unix probes clean up the process group.
- Fixed version flag parsing around the `--` separator and user lookup by UID on macOS. Build metadata refreshes when the checked-out commit changes.
- Self-update retains the previous executable after successful replacement and restores it if the new executable fails verification. Fixed argument handling in the replacement helper.
- The stable updater falls back to tagged stable releases when GitHub's Latest entry points at a continuous build. The release installer can also recover when the Latest manifest is unavailable.
- The Unix release installer supports `--sync-existing` and repeatable `--replace-existing` for standalone copies. It checks all destinations before writing, keeps private backups, and rolls back replacements if a later step fails. Self-update on Linux and macOS reports conflicting copies before replacement.

- Selecting Cargo explicitly explains when its optional `cargo-update` helper is missing. Cargo project dependencies are outside host package maintenance.
- Stable binaries display `0.6.5`. The fourth component in Continuous builds is a workflow run number. Public Continuous snapshots require an explicit workflow dispatch.

## Packaging and documentation

- The release contains Linux x86-64 and ARM64, macOS Intel and Apple Silicon, Windows x86-64, and Android x86-64 and ARM64 binaries. It includes a source archive, the Unix installer, SHA-256 checksums, and a Stable update manifest.
- Source builds declare Rust 1.85 as the minimum. The English and Persian installation guides cover Cargo prerequisites, source builds, user installation, PATH setup, and the supported Termux installation path.
- Added an on-demand GitHub Actions workflow and PKGBUILD for building and optionally publishing already published **stable** versions to AUR.
- GitHub Actions saves test output by platform and saves native package-manager search reports for diagnosing failures.

## Validation

GitHub Actions builds and tests Ubuntu, macOS, and Windows. Separate native checks cover WinGet, Termux, APT, Pacman, DNF5, Flatpak, and Snap. The release workflow checks the archives and publishes their SHA-256 files.

Choose the archive for your platform below. Compatible older standalone installs can select `allp self-update --update-channel stable`. To switch from a 0.6.5 preview or a Continuous build with the same base version, install the Stable asset directly and keep a backup of the earlier binary.

[Full code changes since 0.6.4](https://github.com/allp-manager/allp-manager/compare/e14f3acdbf89742507209d7f29412b78cdaf3f3e...v0.6.5)
