# Homebrew Backend

Homebrew is registered as an experimental backend for macOS and Linuxbrew.

Discovery checks `brew` on `PATH` and common prefixes:

- `/opt/homebrew/bin/brew`
- `/usr/local/bin/brew`
- `/home/linuxbrew/.linuxbrew/bin/brew`

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
