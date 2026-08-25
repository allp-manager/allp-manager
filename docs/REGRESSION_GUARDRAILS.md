# Regression Guardrails

This is the maintainer record for cross-cutting behavior that has previously
regressed or has a high cost of regression. Keep component design detail in its
own document; use this file to record the invariant, its owner, and the proof
that must remain in place.

## Recording a behavior change

For every behavior-changing pull request:

1. Add a concise user-visible entry under `Unreleased` in `CHANGELOG.md`.
2. Add or update the relevant guardrail below, including its owning module and
   automated regression test.
3. Run `make quality`; run any guardrail-specific command shown below when the
   change touches that area.
4. Do not weaken a trust, privilege, or downgrade check merely to make an
   update appear available. Document the new, narrowly scoped exception and
   test both the allowed and rejected cases.

## Active guardrails

| Area | Invariant | Owner and automated proof | Required focused check |
| --- | --- | --- | --- |
| Unix process execution | A captured Unix child has its own process group so timeout/cancellation can clean up descendants. The trait providing `process_group` must remain imported on Unix. | `src/execution/runner.rs`; `execution::runner::tests::capture_finishes_when_detached_descendant_holds_pipes`. | `make release` and the runner test through `make quality`. |
| Local reinstall and self-update | `make reinstall` creates a non-official development identity with local revision `1`. It must not be classified as newer than a verified official continuous build merely because that local marker collides with a CI revision. The install target must print the full identity so a short version cannot hide a changed commit. | `src/build_identity.rs`; `verified_continuous_build_replaces_local_reinstall_on_revision_collision`, `unverified_revision_collision_cannot_replace_local_development_build`, and `self_update::tests::local_reinstall_detects_verified_continuous_main_build`. | Build/install a local binary, compare the printed commit with `git rev-parse HEAD`, then run `allp self-update --check-only` after a successful continuous publication. |
| Installed-binary resolution | A system reinstall must not imply that a PATH-shadowing user-local binary was updated. `make install` and `make reinstall` print the installed identity and warn when `command -v allp` resolves outside the selected system target. | `Makefile`; `install-resolution-warning` and `install-check`. | Run `make install-resolution-warning` and `make install-check` after every reinstall; deliberately use `make install-user` when the user-local binary is the intended executable. |
| Update trust and downgrade safety | The local-development exception applies only to an official continuous candidate. Published identity conflicts remain errors, and an older remote build must never replace the installed build. | `src/build_identity.rs`; the collision, integrity-error, and no-downgrade tests named above. | `make quality`; inspect the continuous manifest/workflow identity when changing the source. |
| Homebrew under sudo on macOS | Homebrew runs as the validated original non-root user. GUI accounts absent from `/etc/passwd` are resolved through Directory Services; no ambient `SUDO_*` value alone is trusted. | `src/execution/privilege.rs`, `src/discovery/homebrew.rs`; Directory Services parser tests and Homebrew discovery tests. | `cargo check --all-targets --target x86_64-apple-darwin` and `cargo check --all-targets --target aarch64-apple-darwin`. |
| Cargo host scope | Cargo install/remove/upgrade runs as the selected/original user and manages only installed binary crates. Host maintenance must never run project `cargo add`/`cargo update` or silently write manifests and lockfiles. | `src/backends/development/rust.rs`; backend unit tests and `rust_from_cargo_uses_crates_io_and_never_mutates_a_project_manifest`. | `cargo test --lib backends::development::rust` and the focused CLI fake-PATH test. |
| Bazzite host routing | Bazzite host mutations use transactional rpm-ostree plans. DNF is unavailable on Bazzite, layering is labeled as a last resort, and Allp never reboots automatically. | `src/backends/system/dnf.rs`, `src/backends/system/rpm_ostree.rs`; backend unit tests and `rpm_ostree_bazzite_alias_plans_transactional_host_layering`. | `cargo test --lib rpm_ostree` and the focused CLI fake-PATH test. |
| Live maintenance dashboard | The dashboard is presentation-only: it must not change native argv, plan privilege, confirmation, exit classification, inherited stdin, or JSON/classic output behavior. In an eligible interactive terminal, native logs remain visible above boxed outcomes and a footer tracks active work. On UI I/O failure, normal stream forwarding resumes. | `src/execution/runner.rs`, `src/cli/tui.rs`, `src/operations/maintenance.rs`; `execution_observer_receives_both_streams_without_changing_final_status`, TUI unit tests, and the PTY integration tests. | `cargo test --lib tui` and `cargo test --test cli_fake_path maintenance_tui -- --nocapture`. |

## Change record

### 2026-08-13 — local reinstall update detection, Unix runner, and macOS Homebrew

- Imported Unix `CommandExt` for `Command::process_group`, restoring release
  builds that use the captured-process cancellation path.
- Allowed a non-official local development build from `make reinstall` to take
  a newer verified continuous build, including the local revision-`1`
  collision. The exception does not permit unverified candidates, published
  identity conflicts, or downgrades.
- Added a macOS Directory Services fallback for Homebrew's validated original
  user lookup, including supplementary group lookup, so normal GUI accounts do
  not disappear when Allp is invoked with `sudo`.
- Made install, reinstall, and install-check print the complete build identity
  and expected source commit instead of a short local version that cannot
  distinguish two revision-`1` development builds.
- Added an explicit PATH-shadow warning after system install/reinstall, because
  a separate `~/.local/bin/allp` must be rebuilt deliberately rather than being
  mistaken for the just-updated `/usr/local/bin/allp` binary.
- Proof recorded for this change: `make quality`, `make release-workflow-test`,
  the macOS target checks above, a temporary-path `make reinstall`, and a live
  `self-update --check-only` against the verified continuous artifact.

### 2026-08-13 — live maintenance dashboard

- Added an inline dashboard for real interactive `update` and `upgrade`
  execution. It preserves normal scrollback and package-manager stdin instead
  of taking over the terminal with an alternate screen.
- Routed runner output/timing through a presentation-only observer so native
  commands, arguments, privilege preparation, and result classification remain
  centralized in the runner. Terminal control bytes are removed from the
  dashboard projection.
- Kept JSON, dry-run, redirected/non-TTY, `TERM=dumb`, non-interactive, and
  explicit `--no-tui` runs on the classic output contract. A failed dashboard
  writer gives output handling back to the runner rather than stopping a native
  mutation.
- Proof recorded for this change: TUI unit tests, runner observer tests, and
  PTY integration coverage for native logs, state cards, progress footer, and
  `--no-tui` fallback.
