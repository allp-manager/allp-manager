# Allp TODO

Implementation tasks live here. Product direction and larger milestones live in [ROADMAP.md](ROADMAP.md).

## Completed In 0.3.3

- [x] Update repository URL in `Cargo.toml`
- [x] Revalidate Snap install candidates with `snap info` before plan construction
- [x] Treat Snap `snap find` rows as discovery candidates until exact resolution succeeds
- [x] Add classic-confinement Snap install plans
- [x] Normalize decorated Snap publishers into publisher name plus verification state
- [x] Add repository-specific ignore rules and untrack generated command logs
- [x] Add safe root Makefile workflow
- [x] Refresh English and Persian README documentation
- [x] Add source install/reinstall/uninstall Makefile workflow
- [x] Add local-only release prepare/finalize workflow with Git hook
- [x] Add tag-triggered GitHub Release workflow and release-title metadata
- [x] Add temp-repository tests for release automation safety

## Completed In 0.3.4

- [x] Add cross-platform platform context and central executable capability registry
- [x] Add structured backend requirements and Linux bootstrap providers
- [x] Use snapd REST for primary discovery, exact resolution, installation, and change monitoring
- [x] Keep CLI Snap fallback reasoned and block fallback after authoritative REST not-found
- [x] Model Flatpak executable/remotes independently and require explicit Flathub setup
- [x] Exclude failed backends and discard cached results during alternative routing
- [x] Add trusted GitHub self-update with manifest target selection, SHA-256, rollback, and guarded relaunch
- [x] Add `allp doctor`, target-specific CI/release assets, and release-manifest generation

## Remaining Implementation Work

- [ ] Broaden backend parser fixture coverage
  - Priority: P1
  - Reason: Real distro output varies more than fake-path fixtures can cover.
  - Module: `src/backends/*`, `tests/fixtures/*`
  - Target: 0.3.x alpha hardening

- [ ] Validate experimental Linux-family backends on real distributions
  - Priority: P1
  - Reason: Zypper, APK, XBPS, Portage, eopkg, and swupd are implemented but need host validation.
  - Module: `src/backends/system/family.rs`
  - Target: 0.3.x alpha hardening

- [ ] Add richer `detect --verbose` probes
  - Priority: P2
  - Reason: Current probes are intentionally lightweight; richer version/remotes diagnostics would improve support reports.
  - Module: `src/discovery`, backend probes
  - Target: 0.4

- [ ] Validate Homebrew on macOS and Linuxbrew hosts
  - Priority: P1
  - Reason: Homebrew support is implemented but still marked experimental.
  - Module: `src/backends/homebrew.rs`, `src/bootstrap/homebrew.rs`
  - Target: 0.3.x alpha hardening

- [ ] Expand Python PEP 668 and virtual-environment edge-case tests
  - Priority: P1
  - Reason: Python install/update safety depends on environment ownership and policy details.
  - Module: `src/backends/development/python.rs`
  - Target: 0.3.x alpha hardening

- [ ] Harden Node project/workspace mutation policy
  - Priority: P1
  - Reason: Project-scope package changes can modify manifests and lockfiles.
  - Module: `src/backends/development/node.rs`
  - Target: 0.4

- [ ] Add signal forwarding and Ctrl+C process-group tests
  - Priority: P2
  - Reason: Long-running native commands should terminate predictably.
  - Module: `src/execution/runner.rs`
  - Target: 0.4

- [ ] Harden trusted-path validation before root elevation
  - Priority: P1
  - Reason: Root-required child execution should keep tightening executable trust checks.
  - Module: `src/execution/privilege.rs`
  - Target: 0.4

- [ ] Add an interactive Snap channel chooser
  - Priority: P1
  - Reason: v0.3.4 blocks ambiguous or non-stable Snap channels instead of silently choosing; a future UX should let users choose stable tracks and explicitly confirm riskier channels.
  - Module: `src/backends/universal/snap.rs`, CLI prompts
  - Target: 0.4

- [ ] Publish packaged installation instructions
  - Priority: P2
  - Reason: Source builds are documented; release package distribution is not finalized.
  - Module: documentation, release metadata
  - Target: 0.4

## Explicit Non-Goals For 0.3.4

- GUI or TUI mode
- Plugin marketplace
- Telemetry
- Recommendation engine
- Background daemon
- Allp-owned package cache
- Automatic native confirmation flags
- Universal undo
