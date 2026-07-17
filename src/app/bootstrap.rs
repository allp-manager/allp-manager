use crate::{
    backends::builtin_backends,
    bootstrap as official_bootstrap,
    cli::{select_search_scope, Cli, Commands, Renderer},
    discovery::BackendDiscovery,
    domain::{AllpError, AllpResult},
    execution::{privilege::runtime_context, ProcessRunner, StdProcessRunner},
    operations::{self, OperationContext},
};
use std::{io::IsTerminal, sync::Arc};

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

        let discovery = self.detector.discover(self.runner.as_ref());
        let bootstrap_query_available = match &command {
            Commands::Install(args) => official_bootstrap::has_bootstrap_candidate(&args.package),
            Commands::Search(args) => official_bootstrap::has_bootstrap_candidate(&args.query),
            _ => false,
        };

        if discovery.detected.is_empty()
            && !matches!(&command, Commands::Detect(_))
            && !bootstrap_query_available
        {
            return Err(AllpError::BackendNotDetected(
                "no supported package managers were detected".to_owned(),
            ));
        }

        if verbose > 1 && !matches!(&command, Commands::Detect(_)) {
            renderer.detection(&discovery.report, true);
        } else if verbose > 0
            && !matches!(
                &command,
                Commands::Detect(_) | Commands::Update(_) | Commands::Upgrade(_)
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
            Commands::Update(_) => {
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
        }

        Ok(0)
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
