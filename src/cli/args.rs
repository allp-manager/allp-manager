use clap::{ArgAction, Args, Parser, Subcommand};
use std::{ffi::OsString, path::PathBuf};

use crate::{
    domain::{DeveloperTarget, SearchScope},
    self_update::UpdateChannel,
};

#[derive(Debug, Parser)]
#[command(
    name = "allp",
    version,
    about = "A transparent package-manager orchestrator for Linux",
    long_about = "Allp detects package managers already installed on the system, delegates work to their native commands, and keeps the user in control."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Args, Default)]
pub struct CommonOptions {
    /// Emit machine-readable JSON.
    #[arg(long)]
    pub json: bool,

    /// Disable ANSI colors and spinner animation.
    #[arg(long)]
    pub no_color: bool,

    /// Increase diagnostic output. Repeat for more detail.
    #[arg(short = 'v', long, action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Clone, Args, Default)]
pub struct BackendOptions {
    /// Restrict the operation to one backend, for example: --from apt.
    #[arg(long = "from")]
    pub backend: Option<String>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct ScopeOptions {
    /// Broad search scope: apps, dev, or all.
    #[arg(long, value_parser = parse_search_scope)]
    pub scope: Option<SearchScope>,
}

#[derive(Debug, Clone, Args, Default)]
pub struct MutationOptions {
    /// Print native commands without executing mutating operations.
    #[arg(long)]
    pub dry_run: bool,

    /// Never show an interactive selection prompt.
    #[arg(long)]
    pub no_interactive: bool,

    /// Bypass only Allp's final confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Permit an explicitly planned prerequisite bootstrap with --yes.
    #[arg(long)]
    pub allow_bootstrap: bool,
}

#[derive(Debug, Clone, Args, Default)]
pub struct TargetOptions {
    /// Development update target: project, workspace, global, environment, tools, or all.
    #[arg(long, value_parser = parse_developer_target)]
    pub target: Option<DeveloperTarget>,
}

#[derive(Debug, Clone, Args)]
pub struct DetectArgs {
    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct SearchArgs {
    /// Package or application name.
    pub query: String,

    /// Only show exact package or display-name matches.
    #[arg(long)]
    pub exact: bool,

    /// Maximum visible non-exact results. Exact matches are always shown.
    #[arg(long, default_value_t = 25)]
    pub limit: usize,

    /// Include weak fuzzy matches in the visible result set.
    #[arg(long = "all")]
    pub all: bool,

    #[command(flatten)]
    pub backend: BackendOptions,

    #[command(flatten)]
    pub scope: ScopeOptions,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct PackageMutationArgs {
    /// Package or application name.
    pub package: String,

    #[command(flatten)]
    pub backend: BackendOptions,

    #[command(flatten)]
    pub scope: ScopeOptions,

    #[command(flatten)]
    pub mutation: MutationOptions,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct MaintenanceArgs {
    #[command(flatten)]
    pub backend: BackendOptions,

    #[command(flatten)]
    pub scope: ScopeOptions,

    #[command(flatten)]
    pub target: TargetOptions,

    #[command(flatten)]
    pub mutation: MutationOptions,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct UpdateArgs {
    #[command(flatten)]
    pub backend: BackendOptions,

    #[command(flatten)]
    pub scope: ScopeOptions,

    #[command(flatten)]
    pub target: TargetOptions,

    #[command(flatten)]
    pub mutation: MutationOptions,

    /// Skip Allp's GitHub self-update check.
    #[arg(long)]
    pub skip_self_update: bool,

    /// Check or update Allp itself, then stop.
    #[arg(long)]
    pub self_only: bool,

    /// Check for updates without changing Allp or backend state.
    #[arg(long)]
    pub check_only: bool,

    /// Do not contact GitHub or remote package sources.
    #[arg(long)]
    pub offline: bool,

    /// Select stable or prerelease Allp updates.
    #[arg(long, value_parser = parse_update_channel)]
    pub update_channel: Option<UpdateChannel>,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct SelfUpdateArgs {
    /// Check for a newer Allp release without changing the installation.
    #[arg(long)]
    pub check_only: bool,

    /// Do not contact GitHub.
    #[arg(long)]
    pub offline: bool,

    /// Select stable or prerelease Allp updates.
    #[arg(long, value_parser = parse_update_channel)]
    pub update_channel: Option<UpdateChannel>,

    #[command(flatten)]
    pub mutation: MutationOptions,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct InternalSnapdInstallArgs {
    #[arg(long)]
    pub socket: PathBuf,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub channel: String,
    #[arg(long)]
    pub classic: bool,
    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct InternalReplaceArgs {
    #[arg(long)]
    pub staged: PathBuf,
    #[arg(long)]
    pub destination: PathBuf,
    #[arg(long)]
    pub version: String,
    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct InternalDeferredReplaceArgs {
    #[arg(long)]
    pub staged: PathBuf,
    #[arg(long)]
    pub destination: PathBuf,
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub cleanup_dir: PathBuf,
    #[arg(last = true, allow_hyphen_values = true)]
    pub continuation: Vec<OsString>,
    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct ListArgs {
    /// Show only installed packages whose ID or display name contains this text.
    #[arg(long)]
    pub filter: Option<String>,

    /// Maximum number of installed packages to show after filtering.
    #[arg(long)]
    pub limit: Option<usize>,

    /// Print directly instead of using a pager for long human-readable output.
    #[arg(long)]
    pub no_pager: bool,

    #[command(flatten)]
    pub backend: BackendOptions,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Clone, Args)]
pub struct InfoArgs {
    /// Package or application name.
    pub package: String,

    /// Show normalized extended metadata.
    #[arg(long)]
    pub full: bool,

    /// Show native backend info output when supported.
    #[arg(long)]
    pub raw: bool,

    #[command(flatten)]
    pub backend: BackendOptions,

    #[command(flatten)]
    pub common: CommonOptions,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Detect supported package managers available in the current environment.
    Detect(DetectArgs),

    /// Search all detected package managers.
    Search(SearchArgs),

    /// Search, select a source when needed, and install through the native manager.
    Install(PackageMutationArgs),

    /// Find installed copies and remove the selected one through its native manager.
    Remove(PackageMutationArgs),

    /// Run each detected backend's native update action.
    Update(UpdateArgs),

    /// Run each detected backend's native upgrade action.
    Upgrade(MaintenanceArgs),

    /// List installed packages grouped by backend.
    List(ListArgs),

    /// Show package information from the owning or selected backend.
    Info(InfoArgs),

    /// Report platform, capability, backend, and self-update diagnostics.
    Doctor(DoctorArgs),

    /// Securely check for and install an official Allp release.
    SelfUpdate(SelfUpdateArgs),

    #[command(hide = true)]
    InternalSnapdInstall(InternalSnapdInstallArgs),

    #[command(hide = true)]
    InternalReplace(InternalReplaceArgs),

    #[command(hide = true)]
    InternalDeferredReplace(InternalDeferredReplaceArgs),
}

impl Commands {
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            Self::Install(_)
                | Self::Remove(_)
                | Self::Update(_)
                | Self::Upgrade(_)
                | Self::SelfUpdate(_)
                | Self::InternalSnapdInstall(_)
                | Self::InternalReplace(_)
                | Self::InternalDeferredReplace(_)
        )
    }

    pub fn common(&self) -> &CommonOptions {
        match self {
            Self::Detect(args) => &args.common,
            Self::Search(args) => &args.common,
            Self::Install(args) | Self::Remove(args) => &args.common,
            Self::Update(args) => &args.common,
            Self::Upgrade(args) => &args.common,
            Self::List(args) => &args.common,
            Self::Info(args) => &args.common,
            Self::Doctor(args) => &args.common,
            Self::SelfUpdate(args) => &args.common,
            Self::InternalSnapdInstall(args) => &args.common,
            Self::InternalReplace(args) => &args.common,
            Self::InternalDeferredReplace(args) => &args.common,
        }
    }

    pub fn backend_filter(&self) -> Option<&str> {
        match self {
            Self::Search(args) => args.backend.backend.as_deref(),
            Self::Install(args) | Self::Remove(args) => args.backend.backend.as_deref(),
            Self::Update(args) => args.backend.backend.as_deref(),
            Self::Upgrade(args) => args.backend.backend.as_deref(),
            Self::List(args) => args.backend.backend.as_deref(),
            Self::Info(args) => args.backend.backend.as_deref(),
            Self::Detect(_)
            | Self::Doctor(_)
            | Self::SelfUpdate(_)
            | Self::InternalSnapdInstall(_)
            | Self::InternalReplace(_)
            | Self::InternalDeferredReplace(_) => None,
        }
    }

    pub fn search_scope(&self) -> Option<SearchScope> {
        match self {
            Self::Search(args) => args.scope.scope,
            Self::Install(args) => args.scope.scope,
            Self::Update(args) => args.scope.scope,
            Self::Upgrade(args) => args.scope.scope,
            _ => None,
        }
    }

    pub fn target(&self) -> Option<DeveloperTarget> {
        match self {
            Self::Update(args) => args.target.target,
            Self::Upgrade(args) => args.target.target,
            _ => None,
        }
    }

    pub fn dry_run(&self) -> bool {
        match self {
            Self::Install(args) | Self::Remove(args) => args.mutation.dry_run,
            Self::Update(args) => args.mutation.dry_run || args.check_only,
            Self::Upgrade(args) => args.mutation.dry_run,
            Self::SelfUpdate(args) => args.mutation.dry_run || args.check_only,
            _ => false,
        }
    }

    pub fn no_interactive(&self) -> bool {
        match self {
            Self::Install(args) | Self::Remove(args) => args.mutation.no_interactive,
            Self::Update(args) => args.mutation.no_interactive,
            Self::Upgrade(args) => args.mutation.no_interactive,
            Self::SelfUpdate(args) => args.mutation.no_interactive,
            _ => false,
        }
    }

    pub fn yes(&self) -> bool {
        match self {
            Self::Install(args) | Self::Remove(args) => args.mutation.yes,
            Self::Update(args) => args.mutation.yes,
            Self::Upgrade(args) => args.mutation.yes,
            Self::SelfUpdate(args) => args.mutation.yes,
            _ => false,
        }
    }

    pub fn allow_bootstrap(&self) -> bool {
        match self {
            Self::Install(args) | Self::Remove(args) => args.mutation.allow_bootstrap,
            Self::Update(args) => args.mutation.allow_bootstrap,
            Self::Upgrade(args) => args.mutation.allow_bootstrap,
            Self::SelfUpdate(args) => args.mutation.allow_bootstrap,
            _ => false,
        }
    }

    pub fn json(&self) -> bool {
        self.common().json
    }

    pub fn no_color(&self) -> bool {
        self.common().no_color
    }

    pub fn verbose(&self) -> u8 {
        self.common().verbose
    }
}

fn parse_search_scope(value: &str) -> Result<SearchScope, String> {
    value.parse()
}

fn parse_developer_target(value: &str) -> Result<DeveloperTarget, String> {
    value.parse()
}

fn parse_update_channel(value: &str) -> Result<UpdateChannel, String> {
    match value.to_ascii_lowercase().as_str() {
        "stable" => Ok(UpdateChannel::Stable),
        "prerelease" | "pre" => Ok(UpdateChannel::Prerelease),
        _ => Err("update channel must be stable or prerelease".to_owned()),
    }
}
