# Name Collisions

Allp treats package names and software identities as different facts.

When a registry package has the same name as a known piece of software but is not verified as that software, Allp labels it as `Exact package name` or `Conflicting name` instead of treating it as an exact identity match.

Homebrew is the release-blocking example for v0.3.3:

- Query: `Homebrew`
- Official identity: Homebrew package manager
- Official candidate: Homebrew official bootstrap installer
- Registry collision: npm package named `homebrew`

The official Homebrew bootstrap candidate ranks first. npm `homebrew` remains visible, but it is labeled as a conflicting exact-name match with a warning that it is not the Homebrew package manager.

Real installation of a conflicting identity requires an additional default-No confirmation. `--yes` does not bypass that confirmation.
