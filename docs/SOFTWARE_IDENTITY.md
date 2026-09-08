# Software Identity

Allp separates package-name matching from software identity.

Each install/search candidate carries:

- `name_match`: exact, normalized exact, alias, prefix, token, or fuzzy.
- `confidence`: official, verified, probable, unverified, or conflicting.
- `distribution`: official installer, official package, verified third-party package, name-match only, related, or fuzzy.
- `software_type`: package manager, system package, universal application, runtime, registry client, language package, installer, or unknown.
- optional canonical identity, confidence provenance, and warning text.

The canonical catalog covers the package-manager/runtime identities plus an
initial cross-backend Firefox mapping. APT/Pacman/DNF `firefox`, Flatpak
`org.mozilla.firefox`, and Snap `firefox` share canonical ID `firefox` with
backend-specific provenance.

`CandidateGroup` is built after candidate ordering. Official and verified
canonical IDs may form a confirmed group; probable relationships are displayed
in a separate uncertainty region. Unverified or conflicting same-name results
remain separate. Group selection numbers are the original stable one-based
candidate numbers. Grouping never authorizes automatic source selection.

An exact registry package name is not an exact software identity. For example, npm `homebrew` is an exact npm package name but a conflicting identity when the user asks for the Homebrew package manager.
