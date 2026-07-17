# Software Identity

Allp v0.3.3 separates package-name matching from software identity.

Each install/search candidate carries:

- `name_match`: exact, normalized exact, alias, prefix, token, or fuzzy.
- `confidence`: official, verified, probable, unverified, or conflicting.
- `distribution`: official installer, official package, verified third-party package, name-match only, related, or fuzzy.
- `software_type`: package manager, system package, universal application, runtime, registry client, language package, installer, or unknown.
- optional canonical identity and warning text.

The canonical catalog currently covers Homebrew, APT, Pacman, DNF, Zypper, APK, Flatpak, Snap, Python, pip, pipx, uv, Node.js, npm, pnpm, and Yarn.

An exact registry package name is not an exact software identity. For example, npm `homebrew` is an exact npm package name but a conflicting identity when the user asks for the Homebrew package manager.
