use crate::{
    backends::builtin_backends,
    bootstrap::{
        self as official_bootstrap,
        providers::{select_provider, BootstrapProvider},
    },
    capabilities::{CapabilityAvailability, CapabilityRegistry},
    cli::{confirm_execution, select_search_scope, Cli, Commands, ConfirmationRequest, Renderer},
    diagnostics::DoctorReport,
    discovery::{BackendDiscovery, DetectionState, DiscoveryResult},
    domain::{AllpError, AllpResult, ExecutionPlan, OperationKind, PrivilegeRequirement},
    execution::{privilege::runtime_context, ProcessRunner, StdProcessRunner},
    operations::{self, OperationContext},
    platform::PlatformContext,
    release::Version,
    requirements::bootstrap_requirement_for_backend,
    self_update::{
        apply_replacement, stage_release, CurlHttpClient, GitHubReleaseSource, ReplacementOutcome,
        SelfUpdateState, SelfUpdater, UpdateAvailability, UpdateChannel, SELF_UPDATE_COMPLETED_ENV,
        SELF_UPDATE_VERSION_ENV,
    },
    state,
};
use std::{ffi::OsString, fs, io::IsTerminal, path::Path, process::Command, sync::Arc};

pub struct App {
    detector: BackendDiscovery,
    runner: Arc<dyn ProcessRunner>,
}

impl App {
    pub fn new() -> Self {
        Self {
            detector: BackendDiscovery::new(builtin_backends()),
            runner: Arc::new(StdProcessRunner),
        }
    }

    pub fn run(&self, cli: Cli) -> AllpResult<u8> {
        let Cli { command } = cli;
        let dry_run = command.dry_run();
        let json = command.json();
        let no_interactive = command.no_interactive();
        let yes = command.yes();
        let allow_bootstrap = command.allow_bootstrap();
        let no_color = command.no_color();
        let verbose = command.verbose();
        let backend_filter = command.backend_filter().map(str::to_owned);
        let mut search_scope = command.search_scope();
        let target = command.target();

        if json && command.is_mutating() && !dry_run {
            return Err(AllpError::InvalidInput(
                "--json is supported for mutating commands only together with --dry-run in v0.3.3"
                    .to_owned(),
            ));
        }

        let renderer = Renderer::new(no_color, json);
        let privilege_context = runtime_context();
        if let Commands::InternalSnapdInstall(args) = &command {
            validate_internal_socket(&args.socket)?;
            crate::backends::universal::snap::run_snapd_install(
                &args.socket,
                &args.name,
                &args.channel,
                args.classic,
            )?;
            return Ok(0);
        }
        if let Commands::InternalReplace(args) = &command {
            run_internal_replacement(&args.staged, &args.destination, &args.version)?;
            return Ok(0);
        }
        if let Commands::InternalDeferredReplace(args) = &command {
            run_internal_deferred_replacement(
                &args.staged,
                &args.destination,
                &args.version,
                &args.cleanup_dir,
                &args.continuation,
            )?;
            return Ok(0);
        }
        let platform = PlatformContext::detect(&privilege_context);
        cleanup_deferred_update(&platform);
        persist_deferred_update_success(&platform)?;
        let mut capabilities = CapabilityRegistry::probe_defaults(&platform);
        let no_interactive = no_interactive || json || !std::io::stdin().is_terminal();
        let mut root_context_notice_shown = false;

        if search_scope.is_none()
            && backend_filter.is_none()
            && matches!(&command, Commands::Search(_) | Commands::Install(_))
            && !no_interactive
        {
            if privilege_context.is_root() {
                renderer.runtime_context_notice(&privilege_context);
                root_context_notice_shown = true;
            }
            search_scope = Some(select_search_scope(no_interactive)?);
        }

        let mut discovery = self.detector.discover(self.runner.as_ref());
        if matches!(&command, Commands::Install(_)) {
            if let Some(filter) = backend_filter.as_deref() {
                match self.bootstrap_explicit_backend(
                    filter,
                    &renderer,
                    &platform,
                    &mut capabilities,
                    &mut discovery,
                    dry_run,
                    no_interactive,
                    yes,
                    allow_bootstrap,
                    &privilege_context,
                )? {
                    PrerequisiteOutcome::NotNeeded => {}
                    PrerequisiteOutcome::DryRunComplete => return Ok(0),
                    PrerequisiteOutcome::Installed => {}
                }
            }
        }
        let bootstrap_query_available = match &command {
            Commands::Install(args) => official_bootstrap::has_bootstrap_candidate(&args.package),
            Commands::Search(args) => official_bootstrap::has_bootstrap_candidate(&args.query),
            _ => false,
        };

        if discovery.detected.is_empty()
            && !matches!(
                &command,
                Commands::Detect(_)
                    | Commands::Doctor(_)
                    | Commands::SelfUpdate(_)
                    | Commands::Update(_)
            )
            && !bootstrap_query_available
        {
            return Err(AllpError::BackendNotDetected(
                "no supported package managers were detected".to_owned(),
            ));
        }

        if verbose > 1
            && !matches!(
                &command,
                Commands::Detect(_) | Commands::Doctor(_) | Commands::SelfUpdate(_)
            )
        {
            renderer.detection(&discovery.report, true);
        } else if verbose > 0
            && !matches!(
                &command,
                Commands::Detect(_)
                    | Commands::Doctor(_)
                    | Commands::SelfUpdate(_)
                    | Commands::Update(_)
                    | Commands::Upgrade(_)
            )
        {
            renderer.detected_summary(&discovery.report);
        }

        let context = OperationContext {
            backends: &discovery.detected,
            discovery: &discovery.report,
            runner: self.runner.as_ref(),
            renderer: &renderer,
            privilege_context: &privilege_context,
            dry_run,
            no_interactive,
            yes,
            allow_bootstrap,
            verbose,
            backend_filter: backend_filter.as_deref(),
            search_scope,
            target,
            root_context_notice_shown,
        };

        match command {
            Commands::Detect(args) => {
                operations::detect::run(&renderer, &discovery.report, args.common.verbose > 0)
            }
            Commands::Search(args) => {
                operations::search::run(&context, &args.query, args.exact, args.limit, args.all)?;
            }
            Commands::Install(args) => {
                operations::install::run(&context, &args.package)?;
            }
            Commands::Remove(args) => {
                operations::remove::run(&context, &args.package)?;
            }
            Commands::Update(args) => {
                let self_update_completed = std::env::var_os(SELF_UPDATE_COMPLETED_ENV).is_some();
                let test_offline = cfg!(debug_assertions)
                    && std::env::var_os("ALLP_SELF_UPDATE_TEST_OFFLINE").is_some();
                let self_check_offline = args.offline || test_offline;
                if !args.skip_self_update && !self_update_completed {
                    renderer.phase("Phase 1: Allp self-update check");
                    match self.run_self_update(
                        &renderer,
                        &platform,
                        args.update_channel,
                        args.check_only || dry_run,
                        self_check_offline,
                        no_interactive,
                        yes,
                        !json,
                    ) {
                        Ok(SelfUpdatePhase::Updated) => {
                            if args.self_only {
                                return Ok(0);
                            }
                            return reexecute_after_self_update();
                        }
                        Ok(SelfUpdatePhase::Deferred) => return Ok(0),
                        Ok(SelfUpdatePhase::NoChange) => {}
                        Err(error) if args.self_only => return Err(error),
                        Err(error) => {
                            renderer.warn(&format!("Allp self-update check failed: {error}"));
                            if !no_interactive
                                && !confirm_execution(
                                    false,
                                    false,
                                    ConfirmationRequest {
                                        prompt: "Continue with backend updates?".to_owned(),
                                        default_yes: true,
                                        non_interactive_hint: String::new(),
                                    },
                                )?
                            {
                                renderer.info_message("Update cancelled");
                                return Ok(0);
                            }
                        }
                    }
                } else {
                    renderer.info_message(if args.skip_self_update {
                        "Allp self-update check skipped by --skip-self-update."
                    } else {
                        "Allp self-update already completed in this process chain."
                    });
                }
                if args.self_only {
                    return Ok(0);
                }
                if args.offline {
                    renderer.info_message(
                        "Offline mode: backend metadata updates were not contacted or executed.",
                    );
                    return Ok(0);
                }
                renderer.phase("Phase 2: Platform and capability refresh");
                renderer.info_message(&format!(
                    "{} · {} · {}",
                    platform.os.label(),
                    platform.architecture.as_str(),
                    platform
                        .target_triple()
                        .unwrap_or_else(|| "unsupported release target".to_owned())
                ));
                renderer.phase("Phase 3: Backend metadata/developer update planning");
                let report = operations::update::run(&context)?;
                if report.has_failures() {
                    return Ok(crate::domain::AllpExitCode::PartialFailure.code());
                }
            }
            Commands::Upgrade(_) => {
                let report = operations::upgrade::run(&context)?;
                if report.has_failures() {
                    return Ok(crate::domain::AllpExitCode::PartialFailure.code());
                }
            }
            Commands::List(args) => {
                operations::list::run(&context, args.filter.as_deref(), args.limit, args.no_pager)?;
            }
            Commands::Info(args) => {
                operations::info::run(&context, &args.package, args.full, args.raw)?;
            }
            Commands::Doctor(_) => {
                let report = DoctorReport::collect(
                    platform,
                    &capabilities,
                    &discovery.report,
                    &discovery.detected,
                    self.runner.as_ref(),
                    &crate::backends::universal::snap::snapd_socket_path(),
                );
                renderer.doctor(&report);
            }
            Commands::SelfUpdate(args) => {
                if std::env::var_os(SELF_UPDATE_COMPLETED_ENV).is_some() {
                    renderer.info_message("Allp self-update completed in this process chain.");
                } else {
                    self.run_self_update(
                        &renderer,
                        &platform,
                        args.update_channel,
                        args.check_only || args.mutation.dry_run,
                        args.offline,
                        no_interactive,
                        yes,
                        true,
                    )?;
                }
            }
            Commands::InternalSnapdInstall(_)
            | Commands::InternalReplace(_)
            | Commands::InternalDeferredReplace(_) => unreachable!(),
        }

        Ok(0)
    }

    #[allow(clippy::too_many_arguments)]
    fn run_self_update(
        &self,
        renderer: &Renderer,
        platform: &PlatformContext,
        requested_channel: Option<UpdateChannel>,
        check_only: bool,
        offline: bool,
        no_interactive: bool,
        yes: bool,
        render_check: bool,
    ) -> AllpResult<SelfUpdatePhase> {
        let state_path = platform.state_dir.join("self-update.json");
        let persisted = state::read_json::<SelfUpdateState>(&state_path)?.unwrap_or_default();
        let channel = requested_channel.unwrap_or(persisted.update_channel);
        let client = CurlHttpClient::default();
        let source = GitHubReleaseSource::official_with_etag(&client, persisted.etag.as_deref());
        let updater = SelfUpdater::new(&source, platform, state_path);
        let check = updater.check(channel, offline)?;
        if render_check {
            renderer.self_update_check(&check);
        }
        if check.availability != UpdateAvailability::Available || check_only {
            return Ok(SelfUpdatePhase::NoChange);
        }
        let release = check
            .release
            .as_ref()
            .expect("available update has a release descriptor");
        let asset = check
            .asset
            .as_ref()
            .expect("available update has a compatible asset");
        let confirmed = confirm_execution(
            no_interactive,
            yes,
            ConfirmationRequest {
                prompt: "Update Allp before continuing?".to_owned(),
                default_yes: true,
                non_interactive_hint:
                    "Review with `allp self-update --check-only`, then run `allp self-update --yes`."
                        .to_owned(),
            },
        )?;
        if !confirmed {
            renderer.info_message("Allp self-update cancelled");
            return Ok(SelfUpdatePhase::NoChange);
        }
        updater.mark_attempted(release.version)?;
        let staged = stage_release(release, asset, platform)?;
        let outcome = apply_replacement(&staged, platform)?;
        match outcome {
            ReplacementOutcome::Replaced => {}
            ReplacementOutcome::RequiresElevation { command } => {
                let plan = ExecutionPlan {
                    backend_id: "allp-self-update".to_owned(),
                    backend_name: "Allp".to_owned(),
                    operation: OperationKind::Update,
                    action: "Atomically replace the installed Allp binary".to_owned(),
                    package_id: Some(format!("allp {}", release.version)),
                    source: Some("official GitHub release".to_owned()),
                    scope: Some(platform.current_executable.display().to_string()),
                    details: vec![("Rollback".to_owned(), "Enabled".to_owned())],
                    command,
                    privilege: PrivilegeRequirement::RootRequired,
                    requires_root: true,
                    interactive: false,
                };
                renderer.planned_operations(std::slice::from_ref(&plan), &runtime_context());
                let status = self.runner.execute(&plan)?;
                if !status.success {
                    return Err(AllpError::CommandFailed {
                        backend: "Allp self-update".to_owned(),
                        command: "privileged atomic replacement".to_owned(),
                        code: status.code,
                        stderr: if status.stderr.trim().is_empty() {
                            status.stdout
                        } else {
                            status.stderr
                        },
                    });
                }
            }
            ReplacementOutcome::DeferredForWindows { staged_binary } => {
                debug_assert_eq!(staged_binary, staged.binary_path);
                let continuation = std::env::args_os().skip(1).collect::<Vec<_>>();
                crate::self_update::schedule_deferred_replacement(
                    &staged,
                    platform,
                    &continuation,
                )?;
                renderer.info_message(
                    "Verified Windows replacement scheduled; Allp will continue after this process exits.",
                );
                return Ok(SelfUpdatePhase::Deferred);
            }
        }
        updater.mark_successful(release.version)?;
        let _ = fs::remove_dir_all(&staged.staging_dir);
        renderer.success_message(&format!("Allp {} installed successfully.", release.version));
        Ok(SelfUpdatePhase::Updated)
    }

    #[allow(clippy::too_many_arguments)]
    fn bootstrap_explicit_backend(
        &self,
        filter: &str,
        renderer: &Renderer,
        platform: &PlatformContext,
        capabilities: &mut CapabilityRegistry,
        discovery: &mut DiscoveryResult,
        dry_run: bool,
        no_interactive: bool,
        yes: bool,
        allow_bootstrap: bool,
        privilege_context: &crate::domain::RuntimePrivilegeContext,
    ) -> AllpResult<PrerequisiteOutcome> {
        let Some(entry) = discovery.report.entries.iter().find(|entry| {
            entry.backend_id.eq_ignore_ascii_case(filter)
                || entry.backend_name.eq_ignore_ascii_case(filter)
                || entry
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(filter))
        }) else {
            return Ok(PrerequisiteOutcome::NotNeeded);
        };
        if entry.state == DetectionState::Ready {
            return Ok(PrerequisiteOutcome::NotNeeded);
        }
        let Some(requirement) = bootstrap_requirement_for_backend(&entry.backend_id) else {
            return Ok(PrerequisiteOutcome::NotNeeded);
        };
        let provider = select_provider(&requirement, platform, capabilities).ok_or_else(|| {
            AllpError::BackendNotDetected(format!(
                "{} is unavailable and no safe bootstrap provider is configured for this distribution; install {} manually and retry",
                entry.backend_name, requirement.id
            ))
        })?;
        let bootstrap = provider
            .plan(&requirement, platform, capabilities)
            .map_err(AllpError::InvalidInput)?;
        renderer.planned_operations(
            std::slice::from_ref(&bootstrap.execution),
            privilege_context,
        );
        if dry_run {
            renderer.success_message("Dry run complete; prerequisite bootstrap was not executed.");
            return Ok(PrerequisiteOutcome::DryRunComplete);
        }
        let confirmed = confirm_execution(
            no_interactive,
            yes && allow_bootstrap,
            ConfirmationRequest {
                prompt: "Install this required component?".to_owned(),
                default_yes: false,
                non_interactive_hint: "Prerequisite installation requires `--yes --allow-bootstrap` after reviewing the plan."
                    .to_owned(),
            },
        )?;
        if !confirmed {
            return Err(AllpError::InvalidInput(
                "prerequisite bootstrap cancelled".to_owned(),
            ));
        }
        let status = self.runner.execute(&bootstrap.execution)?;
        if !status.success {
            return Err(AllpError::CommandFailed {
                backend: bootstrap.execution.backend_name.clone(),
                command: crate::execution::render_execution_plan_with_context(
                    &bootstrap.execution,
                    privilege_context,
                ),
                code: status.code,
                stderr: if status.stderr.trim().is_empty() {
                    status.stdout
                } else {
                    status.stderr
                },
            });
        }
        let refreshed = capabilities.refresh_executable(&requirement.id);
        *discovery = self.detector.discover(self.runner.as_ref());
        let verified = refreshed.availability == CapabilityAvailability::Available
            && discovery.report.entries.iter().any(|entry| {
                (entry.backend_id.eq_ignore_ascii_case(filter)
                    || entry
                        .aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(filter)))
                    && entry.state == DetectionState::Ready
            });
        if !verified {
            return Err(AllpError::BackendNotDetected(format!(
                "{} bootstrap command succeeded, but capability refresh did not verify a ready backend; service activation or manual configuration may still be required",
                requirement.id
            )));
        }
        renderer.success_message(&format!(
            "{} installed and verified through {}.",
            requirement.id,
            provider.id()
        ));
        Ok(PrerequisiteOutcome::Installed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelfUpdatePhase {
    NoChange,
    Updated,
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrerequisiteOutcome {
    NotNeeded,
    DryRunComplete,
    Installed,
}

fn validate_internal_socket(path: &Path) -> AllpResult<()> {
    if !path.is_absolute() {
        return Err(AllpError::InvalidInput(
            "internal snapd socket path must be absolute".to_owned(),
        ));
    }
    Ok(())
}

fn run_internal_replacement(staged: &Path, destination: &Path, version: &str) -> AllpResult<()> {
    if !staged.is_absolute() || !destination.is_absolute() {
        return Err(AllpError::InvalidInput(
            "internal replacement paths must be absolute".to_owned(),
        ));
    }
    let current = std::env::current_exe()?;
    if current != destination {
        return Err(AllpError::InvalidInput(format!(
            "internal replacement destination does not match the running executable: {}",
            destination.display()
        )));
    }
    let version = version
        .parse::<Version>()
        .map_err(AllpError::InvalidInput)?;
    crate::self_update::replace_binary_atomically(staged, destination, version)
}

fn run_internal_deferred_replacement(
    staged: &Path,
    destination: &Path,
    version: &str,
    cleanup_dir: &Path,
    continuation: &[OsString],
) -> AllpResult<()> {
    if !staged.is_absolute() || !destination.is_absolute() || !cleanup_dir.is_absolute() {
        return Err(AllpError::InvalidInput(
            "internal deferred replacement paths must be absolute".to_owned(),
        ));
    }
    let version = version
        .parse::<Version>()
        .map_err(AllpError::InvalidInput)?;
    crate::self_update::run_deferred_replacement(
        staged,
        destination,
        version,
        cleanup_dir,
        continuation,
    )
}

fn cleanup_deferred_update(platform: &PlatformContext) {
    let Some(path) = std::env::var_os("ALLP_SELF_UPDATE_CLEANUP_DIR").map(std::path::PathBuf::from)
    else {
        return;
    };
    let safe_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(".allp-update-"));
    if !safe_name || !path.starts_with(&platform.cache_dir) {
        return;
    }
    for _ in 0..20 {
        match fs::remove_dir_all(&path) {
            Ok(()) => break,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
                ) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(_) => break,
        }
    }
}

fn persist_deferred_update_success(platform: &PlatformContext) -> AllpResult<()> {
    if std::env::var_os(SELF_UPDATE_COMPLETED_ENV).is_none() {
        return Ok(());
    }
    let Some(version) = std::env::var_os(SELF_UPDATE_VERSION_ENV) else {
        return Ok(());
    };
    let version = version
        .to_string_lossy()
        .parse::<Version>()
        .map_err(AllpError::InvalidInput)?;
    let state_path = platform.state_dir.join("self-update.json");
    let mut persisted = state::read_json::<SelfUpdateState>(&state_path)?.unwrap_or_default();
    persisted.last_successful_version = Some(version);
    state::write_json_atomically(&state_path, &persisted)
}

fn reexecute_after_self_update() -> AllpResult<u8> {
    let executable = std::env::current_exe()?;
    let args = std::env::args_os().skip(1).collect::<Vec<OsString>>();
    let mut command = Command::new(executable);
    command
        .args(args)
        .env_clear()
        .env(SELF_UPDATE_COMPLETED_ENV, "1");
    for key in [
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "SHELL",
        "LANG",
        "LC_ALL",
        "TERM",
        "NO_COLOR",
        "XDG_CONFIG_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
        "SUDO_USER",
        "SUDO_UID",
        "SUDO_GID",
        "NVM_DIR",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "PYENV_ROOT",
        "PIPX_HOME",
        "PIPX_BIN_DIR",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let status = command.status()?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
