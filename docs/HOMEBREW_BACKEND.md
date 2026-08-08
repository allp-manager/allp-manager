# Homebrew Backend

Homebrew is registered as an experimental backend for macOS and Linuxbrew.

Discovery resolves `brew` from the active/original user's `PATH`; platform-standard executable directories remain fallback locations. Custom installations are supported through PATH and are not tied to a username or Linuxbrew prefix.

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

Homebrew plans use `OriginalUserRequired`. Homebrew must not run as root. Under `sudo allp ...`, Allp attempts to run Homebrew as `SUDO_USER`; direct-root Homebrew execution is refused.
## Update and upgrade orchestration

Allp treats Homebrew metadata refresh and installed-package upgrade as separate operations. `allp update` capability-probes and prefers `brew update-if-needed`, falling back to `brew update` only when the command is unsupported.

`allp upgrade` establishes metadata freshness, then runs `brew outdated --json=v2` with `HOMEBREW_NO_AUTO_UPDATE=1`. An empty formula/cask result is evidence that no upgrade command is needed. Non-empty results are included in the execution plan; the upgrade also receives `HOMEBREW_NO_AUTO_UPDATE=1`, and Allp queries JSON v2 again afterward to report updated and remaining counts.

Homebrew plans are user-scoped. Under sudo, Allp returns to the original user with `sudo -H -u <user>` and supplies that user's HOME, USER, LOGNAME, PATH, SHELL, and XDG paths. Root-direct Homebrew execution is refused. Homebrew update-lock contention is reported as Busy; Allp never removes Homebrew lock files automatically.
