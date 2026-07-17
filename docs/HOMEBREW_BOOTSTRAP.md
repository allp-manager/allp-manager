# Homebrew Bootstrap

`allp install Homebrew` resolves to the canonical Homebrew package-manager identity.

If `brew` is not installed, Allp can still produce an official bootstrap candidate:

- Source: `https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh`
- Relationship: official installer
- Privilege: original/current user
- Root behavior: Allp refuses to run the user-scoped bootstrap as direct root
- Dry-run behavior: no download, no sudo, no password prompt, no execution

The native command shown by the plan downloads the installer to a temporary file with `curl` or `wget`, then runs `/bin/bash` on that file. It does not use `curl | bash`.

The Homebrew installer itself may ask for sudo as part of Homebrew's normal setup. Allp does not add native yes flags and does not suppress native prompts.
