# Security Policy

Allp executes native package-manager commands and may request elevated privileges for child processes. Security reports are high priority.

## Report Privately

Do not open a public issue for vulnerabilities involving:

- command injection;
- privilege escalation;
- unsafe executable resolution;
- unsafe argument handling;
- credential exposure;
- JSON output contamination that could mislead automation.

Use the repository's private security advisory channel.

## Security Principles

- Native commands are executed without a shell.
- Program and arguments are stored separately.
- Allp should run as the normal user.
- Root elevation applies only to the child execution plan.
- If Allp is invoked through sudo, user-scoped plans run as the original sudo user when possible.
- Homebrew, Python user tooling, Node user tooling, and Flatpak user operations must not create root-owned files in a user's home, project, cache, environment, or prefix.
- Package-manager output is not treated as trusted code.
- Python and Node registry results are not treated as official merely because names look familiar.
- Allp does not collect telemetry.
- Allp does not store sudo passwords.
- Allp does not add automatic confirmation flags.
- `--yes` bypasses only Allp's final confirmation and never adds native auto-confirm flags.
- Allp explains root-required child commands before sudo can prompt.
- Dry runs never invoke sudo.
- Dry runs never execute npm lifecycle scripts or Python installer hooks.

## Known Alpha Limitations

Before a stable release, Allp still needs:

- deeper trusted-path validation before root elevation;
- signal forwarding and process-group tests;
- broader package-manager parser fixture coverage;
- distro matrix validation;
- Homebrew validation on macOS and Linuxbrew hosts;
- Python PEP 668 edge-case coverage;
- npm/pnpm/Yarn project-scope ownership tests;
- review by security-focused maintainers.

The alpha release is not security-audited.
