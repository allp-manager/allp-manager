# Prerequisites And Bootstrap

Platform detection and `CapabilityRegistry` run once per operation. Backends declare `RequirementSet` values instead of embedding prerequisite installation in search or install code.

Requirements distinguish executable installation, service activation, remote addition, configuration changes, permissions, elevation, and network access. These are separate mutations and never share an implicit confirmation.

Initial package bootstrap providers map supported requirements through APT, DNF, Pacman, Zypper, and APK. Unknown distribution/package mappings return manual guidance. The plan always identifies the provider, package, exact argv, system scope, and root requirement before execution.

`--yes` bypasses ordinary Allp confirmation only. Prerequisite or repository changes require `--yes --allow-bootstrap` in non-interactive mode. After an approved executable install, Allp refreshes capability discovery, reruns backend probing (including version/usability checks), and stops if verification fails.
