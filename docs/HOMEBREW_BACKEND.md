# Homebrew Backend

Homebrew is registered as an experimental backend for macOS and Linuxbrew.

Homebrew discovery is not a generic `PATH` lookup. One shared locator supplies the executable used by detect, doctor, search, install, remove, update, and upgrade. Its deterministic precedence is:

1. an explicit path in Allp's `homebrew.json` configuration;
2. the current process `PATH`;
3. a previously validated `homebrew-installation.json` state record;
4. a trustworthy absolute `HOMEBREW_PREFIX`;
5. deterministic paths below the validated original user's home;
6. official platform prefixes.

The official fallbacks are `/home/linuxbrew/.linuxbrew/bin/brew` on Linux, `/opt/homebrew/bin/brew` on Apple Silicon macOS, and `/usr/local/bin/brew` on Intel macOS. They are candidates, not assumptions. Custom installations remain supported through configuration, PATH, environment prefix, and validated persisted identity. Allp never recursively scans the filesystem for files named `brew`.

Every existing candidate is statted and canonicalized, checked for executable permissions and ownership, then probed with `brew --version` and `brew --prefix` through an owner-specific `UserContextExecutor`. The executor re-resolves the exact non-root account from the system account database instead of consulting ambient sudo variables again. The reported prefix must exist and contain the resolved executable. A failing probe is reported as installed but unavailable, wrong-owner, permission, or broken-installation state instead of being collapsed into "not installed". A successful record persists executable, prefix, owner IDs, version, and validation time; persisted records are always revalidated before use. Relative configuration, persisted executable paths, relative PATH entries, and relative `HOMEBREW_PREFIX` values are rejected rather than resolved against Allp's current directory.

Under sudo, the resolved executable must be owned by the validated original user. A normal non-root invocation may use a custom installation owned by another account when its mode permits execution by the current user's primary or supplementary groups (or by other users); probes still execute as the current validated account. This flexibility does not weaken the strict sudo owner match.

Supported operations:

- search;
- install;
- remove;
- update;
- upgrade;
- list;
- info;
- raw info.

Formula and cask search are queried separately where possible. Formula and cask candidates are distinct choices because they may install different artifacts.

Homebrew probes and plans use `OriginalUserRequired`. Homebrew must not run as root. Under `sudo allp ...`, Allp validates `SUDO_USER`, `SUDO_UID`, and `SUDO_GID` against the system account database, requires that identity to match the Homebrew owner, and runs the command through the reusable original-user executor. Direct-root Homebrew execution is refused.

Both capture operations and mutations travel through the same elevated `OriginalUserRequired` builder. It revalidates the complete account tuple immediately before spawning, invokes a canonical root-owned and non-writable sudo helper, and starts Homebrew behind `/usr/bin/env -i` with only deterministic owner context plus explicit command overrides such as `HOMEBREW_NO_AUTO_UPDATE=1`. Ambient `BASH_ENV`, `RUBYOPT`, and `HOMEBREW_*` variables cannot leak through sudo's environment policy.

`allp --verbose detect` records every provider attempt and every validated candidate. `allp doctor homebrew` scopes discovery and output to Homebrew and reports the selected and resolved executable, version, prefix, owner, current Allp user, original sudo user, and the shared backend state; unrelated Snap, Flatpak, executable, and backend sections are omitted and their native diagnostics are not invoked.

## Update and upgrade orchestration

Allp treats Homebrew metadata refresh and installed-package upgrade as separate operations. `allp update` capability-probes and prefers `brew update-if-needed`, falling back to `brew update` only when the command is unsupported. Refreshes set `HOMEBREW_NO_UPDATE_REPORT_NEW=1` to avoid Homebrew's potentially enormous post-update formula and cask report; update errors and normal progress remain visible.

`allp upgrade` establishes metadata freshness, then runs `brew outdated --json=v2` with `HOMEBREW_NO_AUTO_UPDATE=1`. An empty formula/cask result is evidence that no upgrade command is needed. Non-empty results are included in the execution plan; the upgrade also receives `HOMEBREW_NO_AUTO_UPDATE=1`, and Allp queries JSON v2 again afterward to report updated and remaining counts.

Homebrew plans are user-scoped. Under sudo, Allp returns to the original user with `sudo -H -u <user>` and supplies that user's HOME, USER, LOGNAME, PATH, SHELL, and XDG paths. Root-direct Homebrew execution is refused. Homebrew update-lock contention is reported as Busy; Allp never removes Homebrew lock files automatically.

The initial validated discovery result is the process-local Homebrew capability cache and also updates the capability registry's `brew` entry, including path, version, and owner. There is no persisted `ready=true` flag. Immediately before every Homebrew install, remove, metadata update, or package upgrade process is spawned, Allp revalidates that exact planned executable through the same owner-specific version/prefix validation. A missing, replaced, permission-changed, wrong-owner, or unusable executable invalidates the plan; Allp does not silently redirect the approved plan to another installation.
