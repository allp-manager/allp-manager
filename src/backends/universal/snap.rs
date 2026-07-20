mod rest;

use self::rest::{percent_encode, SnapdClient, SnapdRestError};
use crate::execution::render_native_command;
use crate::{
    backends::{
        contract::{
            command_path, BackendOperationCapability, InstallPreflight, InstallPreflightRecovery,
            InstallPreflightStatus, InstallPreflightWarning,
        },
        util::{capture_checked, match_kind},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, BackendOperationRecord, Capability,
        DeveloperTarget, ExecutionPlan, InstalledPackage, MaintenancePlan, NativeCommand,
        OperationKind, OperationStatus, PackageCandidate, PackageDomain, PackageInfo,
        PrivilegeRequirement,
    },
    execution::{CommandOutput, ProcessRunner, ProcessStatus},
    platform::PlatformContext,
    requirements::{snap_requirements, BackendRequirements, RequirementSet},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct SnapBackend;

impl BackendRequirements for SnapBackend {
    fn requirements(&self, context: &PlatformContext) -> RequirementSet {
        snap_requirements(context)
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapAvailability {
    Discovered,
    Resolving,
    Available,
    Unavailable,
    Stale,
    BackendError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CapabilityStatus {
    Operational,
    Degraded(String),
    Unavailable(String),
}

impl CapabilityStatus {
    fn is_usable(&self) -> bool {
        matches!(self, Self::Operational | Self::Degraded(_))
    }

    fn reason(&self) -> Option<&str> {
        match self {
            Self::Operational => None,
            Self::Degraded(reason) | Self::Unavailable(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone)]
struct SnapCapabilities {
    daemon: CapabilityStatus,
    discovery: CapabilityStatus,
    metadata_resolution: CapabilityStatus,
    new_installation: CapabilityStatus,
    installed_refresh: CapabilityStatus,
}

impl SnapAvailability {
    fn as_str(self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Resolving => "resolving",
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::Stale => "stale",
            Self::BackendError => "backend_error",
        }
    }
}

const SNAP_AVAILABILITY_KEY: &str = "snap.availability";
const SNAP_DISCOVERY_QUERY_KEY: &str = "snap.discovery.query";
const SNAP_DISCOVERY_VERSION_KEY: &str = "snap.discovery.version";
const SNAP_DISCOVERY_STATUS_KEY: &str = "snap.discovery.status";
const SNAP_DISCOVERY_PUBLISHER_KEY: &str = "snap.discovery.publisher";
const SNAP_DISCOVERY_PUBLISHER_VERIFICATION_KEY: &str = "snap.discovery.publisher_verification";
const SNAP_DISCOVERY_SUMMARY_KEY: &str = "snap.discovery.summary";
const SNAP_DISCOVERY_NOTES_KEY: &str = "snap.discovery.notes";
const SNAP_TRANSPORT_KEY: &str = "snap.transport";
const SNAP_SOCKET_KEY: &str = "snap.socket";
const SNAP_FALLBACK_REASON_KEY: &str = "snap.fallback_reason";

const CAPABILITIES: &[Capability] = &[
    Capability::Search,
    Capability::Install,
    Capability::Remove,
    Capability::Update,
    Capability::Upgrade,
    Capability::List,
    Capability::Info,
];
const REQUIREMENTS: &[CommandRequirement] = &[];
const OPTIONAL_REQUIREMENTS: &[CommandRequirement] = &[CommandRequirement {
    key: "snap",
    alternatives: &["snap"],
}];

impl Backend for SnapBackend {
    fn id(&self) -> &'static str {
        "snap"
    }
    fn display_name(&self) -> &'static str {
        "Snap"
    }
    fn category(&self) -> BackendCategory {
        BackendCategory::Universal
    }
    fn capabilities(&self) -> &'static [Capability] {
        CAPABILITIES
    }
    fn command_requirements(&self) -> &'static [CommandRequirement] {
        REQUIREMENTS
    }

    fn optional_command_requirements(&self) -> &'static [CommandRequirement] {
        OPTIONAL_REQUIREMENTS
    }

    fn operation_capability(&self, capability: Capability) -> BackendOperationCapability {
        match capability {
            Capability::Update => BackendOperationCapability::Unsupported,
            Capability::Upgrade => BackendOperationCapability::CombinedRefreshAndUpgrade,
            _ => BackendOperationCapability::Unsupported,
        }
    }

    fn operation_not_applicable_message(
        &self,
        capability: Capability,
        operation_capability: BackendOperationCapability,
    ) -> String {
        if capability == Capability::Update
            && operation_capability == BackendOperationCapability::Unsupported
        {
            return "Not applicable during metadata-only update; installed snap refresh is handled by `allp upgrade`".to_owned();
        }
        let operation = capability.label().to_ascii_lowercase();
        match operation_capability {
            BackendOperationCapability::Unsupported => format!("{operation} is not supported"),
            BackendOperationCapability::MetadataRefresh => {
                format!("{operation} only refreshes metadata for this backend")
            }
            BackendOperationCapability::InstalledPackageUpgrade => {
                format!("{operation} only upgrades installed packages for this backend")
            }
            BackendOperationCapability::CombinedRefreshAndUpgrade => {
                format!("{operation} is handled as a combined refresh and upgrade operation")
            }
            BackendOperationCapability::SelfUpdate => {
                format!("{operation} is handled by Allp self-update")
            }
        }
    }

    fn probe(&self, commands: &CommandMap, runner: &dyn ProcessRunner) -> AllpResult<()> {
        let capabilities = inspect_snap_capabilities(self, commands, runner);
        if capabilities.daemon.is_usable()
            || capabilities.discovery.is_usable()
            || capabilities.metadata_resolution.is_usable()
            || capabilities.new_installation.is_usable()
            || capabilities.installed_refresh.is_usable()
        {
            return Ok(());
        }

        let reason = capabilities
            .daemon
            .reason()
            .or_else(|| capabilities.discovery.reason())
            .or_else(|| capabilities.metadata_resolution.reason())
            .or_else(|| capabilities.new_installation.reason())
            .or_else(|| capabilities.installed_refresh.reason())
            .unwrap_or("Snap daemon and CLI are unavailable");
        Err(AllpError::BackendNotDetected(format!(
            "Snap unavailable: {reason}"
        )))
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        Ok(search_snap_candidates(self, commands, runner, query)?
            .candidates
            .into_iter()
            .map(|candidate| candidate.into_package_candidate(self, query))
            .collect())
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let snap = command_path(self, commands, "snap")?;
        let output = capture_checked(self, runner, NativeCommand::new(snap).arg("list"))?;
        Ok(output
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with("Name ") {
                    return None;
                }
                let columns: Vec<&str> = line.split_whitespace().collect();
                let package_id = columns.first()?;
                Some(InstalledPackage {
                    backend_id: self.id().to_owned(),
                    backend_name: self.display_name().to_owned(),
                    category: self.category(),
                    domain: PackageDomain::Universal,
                    package_id: (*package_id).to_owned(),
                    display_name: (*package_id).to_owned(),
                    version: columns.get(1).map(|value| (*value).to_owned()),
                    description: columns.get(5).map(|value| format!("notes: {value}")),
                    source: columns.get(4).map(|value| (*value).to_owned()),
                    scope: Some("system".to_owned()),
                })
            })
            .collect())
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        let snap = command_path(self, commands, "snap")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(snap).args(["info", package_id]),
        )?;
        let info = parse_snap_info(&output, package_id)?;
        Ok(PackageInfo {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::Universal,
            package_id: info.name.clone(),
            display_name: info.title.clone().unwrap_or_else(|| info.name.clone()),
            version: info.version.clone(),
            description: info.summary.clone().or_else(|| info.description.clone()),
            source: info.publisher.as_ref().map(SnapPublisher::human_label),
            scope: Some("system".to_owned()),
            artifact_kind: Some("universal application".to_owned()),
            installed: None,
            extra: snap_info_extra(&info),
        })
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        let snap = command_path(self, commands, "snap")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(snap).args(["info", package_id]),
        )
    }

    fn preflight_plan_install(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        candidate: &PackageCandidate,
    ) -> AllpResult<InstallPreflight> {
        let validated = validate_snap_candidate(self, commands, runner, candidate)?;
        let info = &validated.info;
        validate_snap_architecture(self, candidate, info)?;
        let selected_channel = select_snap_install_channel(self, candidate, info)?;

        let installed = if let Some(installed) = info.installed.clone() {
            Some(installed)
        } else if let Some(snap) = commands.get("snap") {
            inspect_snap_installed(snap, runner, &info.name)?
        } else {
            None
        };
        if let Some(installed) = installed {
            let candidate_version = candidate_version_label(info, selected_channel.as_ref());
            return Ok(InstallPreflight::AlreadyInstalled {
                package_id: info.name.clone(),
                installed_version: installed.version_label(),
                candidate_version,
            });
        }

        let warnings = info
            .validation_warning
            .as_ref()
            .map(|message| InstallPreflightWarning {
                title: "Snap warning".to_owned(),
                message: message.clone(),
            })
            .into_iter()
            .collect();

        Ok(InstallPreflight::UseCandidate {
            candidate: Box::new(resolve_snap_candidate(
                candidate,
                validated,
                selected_channel,
            )),
            warnings,
        })
    }

    fn install_preflight_status(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<Option<InstallPreflightStatus>> {
        let (command, display_command) = snap_resolution_status_command(commands, candidate)?;
        Ok(Some(InstallPreflightStatus {
            stage: "Validating Snap package...".to_owned(),
            command,
            display_command: Some(display_command),
        }))
    }

    fn recover_install_preflight_failure(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        candidate: &PackageCandidate,
        error: AllpError,
        no_interactive: bool,
    ) -> AllpResult<InstallPreflightRecovery> {
        if !matches!(
            error,
            AllpError::ValidationFailed { .. }
                | AllpError::CandidateUnavailable { .. }
                | AllpError::ValidationStartFailed { .. }
                | AllpError::MetadataParseFailed { .. }
        ) {
            return Err(error);
        }
        if no_interactive {
            return Err(error);
        }

        println!("✖ {error}");

        loop {
            println!();
            println!("[1] Search again");
            println!("[2] Try another installer");
            println!("[3] Show Snap diagnostics");
            println!("[0] Cancel");
            print!("Choose an action [1-3, 0 to cancel]: ");
            io::stdout().flush()?;

            let input =
                read_snap_action_line("input closed before a Snap validation action was selected")?;

            match input.as_str() {
                "1" => return Ok(InstallPreflightRecovery::RetrySearch),
                "2" => return Ok(InstallPreflightRecovery::TryAlternativeInstallers),
                "3" => {
                    print_snap_diagnostics(self, commands, runner, candidate);
                    loop {
                        println!();
                        println!("[1] Retry validation");
                        println!("[2] Search again");
                        println!("[3] Try another installer");
                        println!("[0] Cancel");
                        print!("Choose an action [1-3, 0 to cancel]: ");
                        io::stdout().flush()?;
                        match read_snap_action_line(
                            "input closed before a Snap validation action was selected",
                        )?
                        .as_str()
                        {
                            "1" => return Ok(InstallPreflightRecovery::RetryValidation),
                            "2" => return Ok(InstallPreflightRecovery::RetrySearch),
                            "3" => {
                                return Ok(InstallPreflightRecovery::TryAlternativeInstallers);
                            }
                            "0" => return Ok(InstallPreflightRecovery::Cancelled),
                            _ => eprintln!("Please enter 1, 2, 3, or 0."),
                        }
                    }
                }
                "0" => return Ok(InstallPreflightRecovery::Cancelled),
                _ => eprintln!("Please enter 1, 2, 3, or 0."),
            }
        }
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        if candidate
            .metadata
            .get(SNAP_AVAILABILITY_KEY)
            .map_or(true, |availability| {
                availability != SnapAvailability::Available.as_str()
            })
        {
            return Err(AllpError::CandidateUnavailable {
                backend: self.display_name().to_owned(),
                message: "Snap installation requires successful exact resolution".to_owned(),
            });
        }
        if candidate
            .metadata
            .get(SNAP_TRANSPORT_KEY)
            .is_some_and(|transport| transport == SnapTransport::Rest.as_str())
        {
            return snapd_install_plan(self, candidate);
        }
        let snap = command_path(self, commands, "snap")?;
        let mut args = vec!["install".to_owned(), candidate.package_id.clone()];
        if candidate
            .metadata
            .get("snap.confinement")
            .is_some_and(|value| value.eq_ignore_ascii_case("classic"))
        {
            args.push("--classic".to_owned());
        }
        if let Some(channel) = candidate
            .metadata
            .get("snap.channel")
            .filter(|channel| channel.as_str() != "latest/stable")
        {
            args.push(format!("--channel={channel}"));
        }
        let mut plan = snap_plan(
            self,
            snap,
            PlanSpec {
                operation: OperationKind::Install,
                action: "Install Snap application",
                package_id: Some(candidate.package_id.clone()),
                source: candidate.source.clone(),
                scope: candidate.scope.clone(),
                args,
            },
        );
        plan.details = snap_plan_details(candidate);
        Ok(plan)
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let snap = command_path(self, commands, "snap")?;
        Ok(snap_plan(
            self,
            snap,
            PlanSpec {
                operation: OperationKind::Remove,
                action: "Remove snap package",
                package_id: Some(package.package_id.clone()),
                source: package.source.clone(),
                scope: package.scope.clone(),
                args: vec!["remove".into(), package.package_id.clone()],
            },
        ))
    }

    fn plan_update(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        Ok(MaintenancePlan {
            plans: Vec::new(),
            records: vec![MaintenancePlan::record(
                self.id(),
                self.display_name(),
                OperationStatus::NotApplicable,
                "Not applicable during metadata-only update; installed snap refresh is handled by `allp upgrade`",
            )],
        })
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let Some(snap) = commands.get("snap").map(PathBuf::as_path) else {
            return Ok(MaintenancePlan {
                plans: Vec::new(),
                records: vec![MaintenancePlan::record(
                    self.id(),
                    self.display_name(),
                    OperationStatus::Unavailable,
                    "snap CLI is unavailable; installed snap refresh cannot be run",
                )],
            });
        };
        Ok(MaintenancePlan::from_plans(vec![snap_plan(
            self,
            snap,
            PlanSpec {
                operation: OperationKind::Upgrade,
                action: "Refresh installed snaps",
                package_id: None,
                source: Some("Snap Store".to_owned()),
                scope: Some("system".to_owned()),
                args: vec!["refresh".into()],
            },
        )]))
    }

    fn classify_execution_failure(
        &self,
        plan: &ExecutionPlan,
        status: &ProcessStatus,
        _command: &str,
    ) -> Option<AllpError> {
        let package = plan.package_id.as_deref().unwrap_or("selected snap");
        match classify_snap_error(&status.stderr).or_else(|| classify_snap_error(&status.stdout)) {
            Some(SnapErrorKind::PackageNotFound) => {
                Some(AllpError::PackageNotFound(package.to_owned()))
            }
            Some(SnapErrorKind::ClassicConfinementRequired) => Some(AllpError::InvalidInput(
                format!(
                    "Snap \"{package}\" requires classic confinement. Re-run the install after metadata validation so the plan includes --classic."
                ),
            )),
            Some(SnapErrorKind::ChannelUnavailable) => Some(AllpError::InvalidInput(format!(
                "Snap channel is unavailable for \"{package}\". Run `snap info {package}` and choose an available stable channel."
            ))),
            Some(SnapErrorKind::ArchitectureUnsupported) => Some(AllpError::InvalidInput(format!(
                "Snap \"{package}\" is not available for this architecture."
            ))),
            Some(SnapErrorKind::DaemonUnavailable) => Some(AllpError::InvalidInput(
                "Snap daemon is unavailable. Check snapd status before retrying.".to_owned(),
            )),
            Some(SnapErrorKind::StoreUnavailable) => Some(AllpError::InvalidInput(
                "Snap Store metadata is unavailable. Check network or Snap Store status before retrying."
                    .to_owned(),
            )),
            Some(SnapErrorKind::PermissionDenied) => Some(AllpError::InvalidInput(
                "Snap denied permission for the requested operation.".to_owned(),
            )),
            Some(SnapErrorKind::AlreadyInstalled) => Some(AllpError::InvalidInput(format!(
                "Snap \"{package}\" is already installed."
            ))),
            _ => None,
        }
    }

    fn classify_execution_success(
        &self,
        plan: &ExecutionPlan,
        status: &ProcessStatus,
        _command: &str,
    ) -> Option<Vec<BackendOperationRecord>> {
        parse_snap_refresh_status(&status.stdout, &status.stderr).map(
            |(operation_status, message)| {
                vec![BackendOperationRecord {
                    backend_id: plan.backend_id.clone(),
                    backend_name: plan.backend_name.clone(),
                    action: None,
                    command: None,
                    status: operation_status,
                    message: Some(message),
                }]
            },
        )
    }
}

fn inspect_snap_capabilities(
    backend: &SnapBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
) -> SnapCapabilities {
    let socket = snapd_socket_path();
    let rest = SnapdClient::new(&socket);
    let daemon = match rest.get("/v2/system-info") {
        Ok(_) | Err(SnapdRestError::Daemon { .. }) => CapabilityStatus::Operational,
        Err(error) => CapabilityStatus::Unavailable(error.to_string()),
    };

    let cli_status = match commands.get("snap") {
        Some(snap) => match capture_checked(
            backend,
            runner,
            NativeCommand::new(snap)
                .arg("version")
                .timeout(Duration::from_secs(2)),
        ) {
            Ok(_) => CapabilityStatus::Operational,
            Err(error) => CapabilityStatus::Unavailable(error.to_string()),
        },
        None => CapabilityStatus::Unavailable("snap CLI was not found".to_owned()),
    };

    let rest_usable = matches!(daemon, CapabilityStatus::Operational);
    let cli_usable = cli_status.is_usable();
    let discovery = if rest_usable || cli_usable {
        if rest_usable {
            CapabilityStatus::Operational
        } else {
            CapabilityStatus::Degraded("using snap CLI fallback for discovery".to_owned())
        }
    } else {
        CapabilityStatus::Unavailable(
            daemon
                .reason()
                .unwrap_or("Snap discovery transport is unavailable")
                .to_owned(),
        )
    };
    let metadata_resolution = if rest_usable || cli_usable {
        if rest_usable {
            CapabilityStatus::Operational
        } else {
            CapabilityStatus::Degraded("using snap CLI fallback for metadata resolution".to_owned())
        }
    } else {
        CapabilityStatus::Unavailable(
            daemon
                .reason()
                .unwrap_or("Snap metadata resolution transport is unavailable")
                .to_owned(),
        )
    };
    let new_installation = if rest_usable || cli_usable {
        if rest_usable {
            CapabilityStatus::Operational
        } else {
            CapabilityStatus::Degraded("using snap CLI fallback for new installation".to_owned())
        }
    } else {
        CapabilityStatus::Unavailable(
            daemon
                .reason()
                .unwrap_or("Snap installation transport is unavailable")
                .to_owned(),
        )
    };
    let installed_refresh = cli_status;

    SnapCapabilities {
        daemon,
        discovery,
        metadata_resolution,
        new_installation,
        installed_refresh,
    }
}

#[derive(Debug, Clone)]
struct SnapInfo {
    name: String,
    title: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    version: Option<String>,
    publisher: Option<SnapPublisher>,
    confinement: SnapConfinement,
    channels: Vec<SnapChannel>,
    architectures: Vec<String>,
    raw_output: String,
    validation_warning: Option<String>,
    installed: Option<InstalledSnap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapPublisher {
    name: Option<String>,
    verification: SnapPublisherVerification,
}

impl SnapPublisher {
    fn human_label(&self) -> String {
        match &self.name {
            Some(name) if !name.is_empty() => {
                format!("{name} · {}", self.verification.label())
            }
            _ => "unknown publisher".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapPublisherVerification {
    Verified,
    Unverified,
    Unknown,
}

impl SnapPublisherVerification {
    fn label(self) -> &'static str {
        match self {
            Self::Verified => "Verified",
            Self::Unverified => "Unverified",
            Self::Unknown => "Unknown verification",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapConfinement {
    Strict,
    Classic,
    Devmode,
    Unknown,
}

impl SnapConfinement {
    fn parse(value: &str) -> Self {
        let lower = value.to_ascii_lowercase();
        if lower.contains("classic") {
            Self::Classic
        } else if lower.contains("strict") {
            Self::Strict
        } else if lower.contains("devmode") {
            Self::Devmode
        } else {
            Self::Unknown
        }
    }

    fn known(self) -> Option<Self> {
        (self != Self::Unknown).then_some(self)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Strict => "Strict",
            Self::Classic => "Classic",
            Self::Devmode => "Devmode",
            Self::Unknown => "Unknown",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Classic => "classic",
            Self::Devmode => "devmode",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapChannelRisk {
    Stable,
    Candidate,
    Beta,
    Edge,
    Unknown,
}

impl SnapChannelRisk {
    fn parse(channel: &str) -> Self {
        match channel.rsplit('/').next().unwrap_or(channel) {
            "stable" => Self::Stable,
            "candidate" => Self::Candidate,
            "beta" => Self::Beta,
            "edge" => Self::Edge,
            _ => Self::Unknown,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Candidate => "candidate",
            Self::Beta => "beta",
            Self::Edge => "edge",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapChannel {
    name: String,
    risk: SnapChannelRisk,
    version: Option<String>,
    confinement: SnapConfinement,
    available: bool,
}

impl SnapChannel {
    fn track(&self) -> &str {
        self.name.split('/').next().unwrap_or(self.name.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstalledSnap {
    version: Option<String>,
    channel: Option<String>,
}

impl InstalledSnap {
    fn version_label(&self) -> Option<String> {
        version_channel_label(self.version.as_deref(), self.channel.as_deref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapErrorKind {
    PackageNotFound,
    ChannelUnavailable,
    ArchitectureUnsupported,
    ClassicConfinementRequired,
    ClassicConfinementUnsupported,
    StoreUnavailable,
    DaemonUnavailable,
    PermissionDenied,
    AlreadyInstalled,
}

trait SnapService {
    fn search(&self, query: &str) -> AllpResult<SnapWideSearch>;
    fn resolve(&self, candidate: &PackageCandidate) -> AllpResult<SnapResolution>;
    fn install(&self, request: SnapInstallRequest) -> AllpResult<SnapChangeId>;
    fn change(&self, id: &SnapChangeId) -> AllpResult<SnapChange>;
}

struct SnapCliFallbackService<'a> {
    backend: &'a SnapBackend,
    snap: &'a Path,
    runner: &'a dyn ProcessRunner,
    fallback_reason: String,
}

struct SnapdRestService<'a> {
    backend: &'a SnapBackend,
    client: SnapdClient,
}

struct SnapWideSearch {
    candidates: Vec<SnapCandidate>,
}

struct SnapResolution {
    availability: SnapAvailability,
    display_command: String,
    output: Option<CommandOutput>,
    info: Option<SnapInfo>,
    native_error: Option<String>,
    transport: SnapTransport,
    fallback_reason: Option<String>,
    rest_status: Option<u16>,
}

#[derive(Debug, Clone)]
struct ValidatedSnap {
    info: SnapInfo,
    transport: SnapTransport,
    socket_path: Option<PathBuf>,
    fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapTransport {
    Rest,
    Cli,
}

impl SnapTransport {
    fn as_str(self) -> &'static str {
        match self {
            Self::Rest => "snapd-rest",
            Self::Cli => "cli-fallback",
        }
    }
}

#[derive(Debug, Clone)]
struct SnapInstallRequest {
    name: String,
    channel: String,
    classic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapChangeId(String);

#[derive(Debug, Clone)]
struct SnapChange {
    ready: bool,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct SnapCandidate {
    canonical_name: String,
    discovery_version: Option<String>,
    discovery_status: Option<String>,
    discovery_publisher: Option<SnapPublisher>,
    discovery_summary: Option<String>,
    discovery_notes: Vec<String>,
    availability: SnapAvailability,
    resolved: Option<SnapInfo>,
    transport: SnapTransport,
    fallback_reason: Option<String>,
    socket_path: Option<PathBuf>,
}

impl SnapCandidate {
    fn into_package_candidate(self, backend: &SnapBackend, query: &str) -> PackageCandidate {
        let candidate_match = match_kind(&self.canonical_name, query);
        let mut metadata = BTreeMap::new();
        metadata.insert(
            SNAP_AVAILABILITY_KEY.to_owned(),
            self.availability.as_str().to_owned(),
        );
        metadata.insert(SNAP_DISCOVERY_QUERY_KEY.to_owned(), query.to_owned());
        metadata.insert(
            SNAP_TRANSPORT_KEY.to_owned(),
            self.transport.as_str().to_owned(),
        );
        if let Some(reason) = &self.fallback_reason {
            metadata.insert(SNAP_FALLBACK_REASON_KEY.to_owned(), reason.clone());
        }
        if let Some(path) = &self.socket_path {
            metadata.insert(SNAP_SOCKET_KEY.to_owned(), path.display().to_string());
        }
        if let Some(version) = &self.discovery_version {
            metadata.insert(SNAP_DISCOVERY_VERSION_KEY.to_owned(), version.clone());
        }
        if let Some(status) = &self.discovery_status {
            metadata.insert(SNAP_DISCOVERY_STATUS_KEY.to_owned(), status.clone());
        }
        if let Some(summary) = &self.discovery_summary {
            metadata.insert(SNAP_DISCOVERY_SUMMARY_KEY.to_owned(), summary.clone());
        }
        if !self.discovery_notes.is_empty() {
            metadata.insert(
                SNAP_DISCOVERY_NOTES_KEY.to_owned(),
                self.discovery_notes.join(", "),
            );
        }
        let source = self.discovery_publisher.as_ref().map(|publisher| {
            if let Some(name) = &publisher.name {
                metadata.insert(SNAP_DISCOVERY_PUBLISHER_KEY.to_owned(), name.clone());
                metadata.insert("snap.publisher_name".to_owned(), name.clone());
            }
            metadata.insert(
                SNAP_DISCOVERY_PUBLISHER_VERIFICATION_KEY.to_owned(),
                publisher.verification.as_str().to_owned(),
            );
            metadata.insert(
                "snap.publisher_verification".to_owned(),
                publisher.verification.as_str().to_owned(),
            );
            publisher.human_label()
        });

        if let Some(info) = &self.resolved {
            metadata.extend(snap_candidate_metadata(info, None));
        }

        PackageCandidate {
            backend_id: backend.id().to_owned(),
            backend_name: backend.display_name().to_owned(),
            category: backend.category(),
            domain: PackageDomain::Universal,
            package_id: self.canonical_name.clone(),
            display_name: self
                .resolved
                .as_ref()
                .and_then(|info| info.title.clone())
                .unwrap_or_else(|| self.canonical_name.clone()),
            version: self
                .resolved
                .as_ref()
                .and_then(|info| info.version.clone())
                .or(self.discovery_version),
            description: self
                .resolved
                .as_ref()
                .and_then(|info| info.summary.clone().or(info.description.clone()))
                .or(self.discovery_summary),
            source,
            installers: vec![backend.display_name().to_owned()],
            artifact_kind: "universal application".to_owned(),
            scope: Some("system".to_owned()),
            match_kind: candidate_match,
            identity: PackageCandidate::infer_identity(
                candidate_match,
                PackageDomain::Universal,
                "universal application",
            ),
            metadata,
        }
    }
}

impl SnapService for SnapCliFallbackService<'_> {
    fn search(&self, query: &str) -> AllpResult<SnapWideSearch> {
        let command = snap_find_command(self.snap, query);
        let output = self.runner.capture(&command)?;
        if !output.success {
            return Err(AllpError::CommandFailed {
                backend: self.backend.display_name().to_owned(),
                command: render_native_command(&command),
                code: output.code,
                stderr: output_message(&output),
            });
        }
        let mut candidates = parse_snap_find(&output.stdout);
        for candidate in &mut candidates {
            candidate.transport = SnapTransport::Cli;
            candidate.fallback_reason = Some(self.fallback_reason.clone());
        }
        Ok(SnapWideSearch { candidates })
    }

    fn resolve(&self, candidate: &PackageCandidate) -> AllpResult<SnapResolution> {
        let package_id = normalize_snap_package_id(&candidate.package_id)?;
        let command = snap_info_command(self.snap, candidate)?.timeout(Duration::from_secs(10));
        let display_command = render_native_command(&command);
        let output = match self.runner.capture(&command) {
            Ok(output) => output,
            Err(AllpError::Io(error)) => {
                return Err(snap_validation_start_error(
                    self.backend,
                    self.snap,
                    AllpError::Io(error),
                ));
            }
            Err(error) => {
                return Ok(SnapResolution {
                    availability: SnapAvailability::BackendError,
                    display_command,
                    output: None,
                    info: None,
                    native_error: Some(error.to_string()),
                    transport: SnapTransport::Cli,
                    fallback_reason: Some(self.fallback_reason.clone()),
                    rest_status: None,
                });
            }
        };

        if !output.success {
            let native_error = snap_error_text(&output);
            let availability =
                if classify_snap_error(&native_error) == Some(SnapErrorKind::PackageNotFound) {
                    SnapAvailability::Stale
                } else {
                    SnapAvailability::BackendError
                };
            return Ok(SnapResolution {
                availability,
                display_command,
                output: Some(output),
                info: None,
                native_error: Some(native_error),
                transport: SnapTransport::Cli,
                fallback_reason: Some(self.fallback_reason.clone()),
                rest_status: None,
            });
        }

        let mut info = parse_snap_info(&output.stdout, &package_id)?;
        if !output.stderr.trim().is_empty() {
            info.validation_warning = Some(output.stderr.trim().to_owned());
        }
        Ok(SnapResolution {
            availability: SnapAvailability::Available,
            display_command,
            output: Some(output),
            info: Some(info),
            native_error: None,
            transport: SnapTransport::Cli,
            fallback_reason: Some(self.fallback_reason.clone()),
            rest_status: None,
        })
    }

    fn install(&self, _request: SnapInstallRequest) -> AllpResult<SnapChangeId> {
        Err(self
            .backend
            .unsupported("Snap CLI asynchronous change creation"))
    }

    fn change(&self, _id: &SnapChangeId) -> AllpResult<SnapChange> {
        Err(self
            .backend
            .unsupported("Snap CLI asynchronous change monitoring"))
    }
}

impl SnapdRestService<'_> {
    fn search_result(&self, query: &str) -> Result<SnapWideSearch, SnapdRestError> {
        let path = format!("/v2/find?q={}&scope=wide", percent_encode(query));
        let response = self.client.get(&path)?;
        let results = response.result.as_array().ok_or_else(|| {
            SnapdRestError::UnrecognizedResponse(
                "snapd discovery result was not an array".to_owned(),
            )
        })?;
        let mut candidates = Vec::new();
        for result in results {
            candidates.push(parse_snapd_candidate(result, self.client.socket_path())?);
        }
        Ok(SnapWideSearch { candidates })
    }

    fn resolve_result(
        &self,
        candidate: &PackageCandidate,
    ) -> Result<SnapResolution, SnapdRestError> {
        let package_id = normalize_snap_package_id(&candidate.package_id)
            .map_err(|error| SnapdRestError::UnrecognizedResponse(error.to_string()))?;
        let path = format!("/v2/find?name={}", percent_encode(&package_id));
        let (_command, display_command) =
            snapd_diagnostic_command("GET", &path, self.client.socket_path());
        let response = match self.client.get(&path) {
            Ok(response) => response,
            Err(SnapdRestError::Daemon {
                status_code: 404,
                kind,
                message,
                raw_body: _,
            }) if kind.as_deref() == Some("snap-not-found") => {
                return Ok(SnapResolution {
                    availability: SnapAvailability::Stale,
                    display_command,
                    output: None,
                    info: None,
                    native_error: Some(format!(
                        "Snap Store discovery listed this snap, but exact snapd lookup returned {message} (kind: snap-not-found, value: {package_id})"
                    )),
                    transport: SnapTransport::Rest,
                    fallback_reason: None,
                    rest_status: Some(404),
                });
            }
            Err(error @ SnapdRestError::Daemon { .. }) => {
                let status = match &error {
                    SnapdRestError::Daemon { status_code, .. } => Some(*status_code),
                    _ => None,
                };
                return Ok(SnapResolution {
                    availability: SnapAvailability::BackendError,
                    display_command,
                    output: None,
                    info: None,
                    native_error: Some(error.to_string()),
                    transport: SnapTransport::Rest,
                    fallback_reason: None,
                    rest_status: status,
                });
            }
            Err(error) => return Err(error),
        };
        let results = response.result.as_array().ok_or_else(|| {
            SnapdRestError::UnrecognizedResponse("snapd exact result was not an array".to_owned())
        })?;
        let Some(result) = results.iter().find(|result| {
            result
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(&package_id))
        }) else {
            return Ok(SnapResolution {
                availability: SnapAvailability::Unavailable,
                display_command,
                output: None,
                info: None,
                native_error: Some(
                    "snapd returned no exact result for the canonical name".to_owned(),
                ),
                transport: SnapTransport::Rest,
                fallback_reason: None,
                rest_status: Some(response.status_code),
            });
        };
        let mut info = parse_snapd_info(result, &response.raw_body, &package_id)?;
        info.installed = self.inspect_installed(&package_id).ok().flatten();
        Ok(SnapResolution {
            availability: SnapAvailability::Available,
            display_command,
            output: None,
            info: Some(info),
            native_error: None,
            transport: SnapTransport::Rest,
            fallback_reason: None,
            rest_status: Some(response.status_code),
        })
    }

    fn inspect_installed(&self, package_id: &str) -> Result<Option<InstalledSnap>, SnapdRestError> {
        let path = format!("/v2/snaps/{}", percent_encode(package_id));
        match self.client.get(&path) {
            Ok(response) => Ok(Some(InstalledSnap {
                version: response
                    .result
                    .get("version")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                channel: response
                    .result
                    .get("tracking-channel")
                    .or_else(|| response.result.get("channel"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })),
            Err(SnapdRestError::Daemon {
                status_code: 404,
                kind,
                ..
            }) if matches!(kind.as_deref(), Some("snap-not-found" | "not-installed")) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn install_result(&self, request: SnapInstallRequest) -> Result<SnapChangeId, SnapdRestError> {
        let path = format!("/v2/snaps/{}", percent_encode(&request.name));
        let body = snap_install_request_body(&request);
        let response = self.client.post(&path, &body)?;
        if response.response_type != "async" {
            return Err(SnapdRestError::UnrecognizedResponse(
                "snapd install did not return an asynchronous change".to_owned(),
            ));
        }
        response
            .change
            .filter(|change| !change.trim().is_empty())
            .map(SnapChangeId)
            .ok_or_else(|| {
                SnapdRestError::UnrecognizedResponse(
                    "snapd install response did not include a change ID".to_owned(),
                )
            })
    }

    fn change_result(&self, id: &SnapChangeId) -> Result<SnapChange, SnapdRestError> {
        let path = format!("/v2/changes/{}", percent_encode(&id.0));
        let response = self.client.get(&path)?;
        Ok(parse_snap_change(&response.result))
    }
}

fn snap_install_request_body(request: &SnapInstallRequest) -> Value {
    let mut body = serde_json::Map::new();
    body.insert("action".to_owned(), Value::String("install".to_owned()));
    body.insert("channel".to_owned(), Value::String(request.channel.clone()));
    if request.classic {
        body.insert("classic".to_owned(), Value::Bool(true));
    }
    Value::Object(body)
}

fn parse_snap_change(result: &Value) -> SnapChange {
    let status = result
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("Unknown")
        .to_owned();
    let ready = result
        .get("ready")
        .and_then(Value::as_bool)
        .unwrap_or(matches!(
            status.as_str(),
            "Done" | "Error" | "Abort" | "Undone"
        ));
    let error = result
        .get("err")
        .and_then(Value::as_str)
        .filter(|error| !error.trim().is_empty())
        .map(str::to_owned);
    SnapChange {
        ready,
        status,
        error,
    }
}

impl SnapService for SnapdRestService<'_> {
    fn search(&self, query: &str) -> AllpResult<SnapWideSearch> {
        self.search_result(query)
            .map_err(|error| snapd_error(self.backend, "discovery", error))
    }

    fn resolve(&self, candidate: &PackageCandidate) -> AllpResult<SnapResolution> {
        self.resolve_result(candidate)
            .map_err(|error| snapd_error(self.backend, "exact resolution", error))
    }

    fn install(&self, request: SnapInstallRequest) -> AllpResult<SnapChangeId> {
        self.install_result(request)
            .map_err(|error| snapd_error(self.backend, "install", error))
    }

    fn change(&self, id: &SnapChangeId) -> AllpResult<SnapChange> {
        self.change_result(id)
            .map_err(|error| snapd_error(self.backend, "change monitoring", error))
    }
}

fn search_snap_candidates(
    backend: &SnapBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    query: &str,
) -> AllpResult<SnapWideSearch> {
    let rest = SnapdRestService {
        backend,
        client: SnapdClient::new(snapd_socket_path()),
    };
    match rest.search_result(query) {
        Ok(search) => Ok(search),
        Err(error) if error.allows_cli_fallback() => {
            let snap = commands.get("snap").ok_or_else(|| {
                AllpError::BackendNotDetected(format!(
                    "Snap REST discovery unavailable ({}); snap CLI fallback was not found",
                    error.fallback_reason()
                ))
            })?;
            SnapCliFallbackService {
                backend,
                snap,
                runner,
                fallback_reason: error.fallback_reason(),
            }
            .search(query)
        }
        Err(error) => Err(snapd_error(backend, "discovery", error)),
    }
}

fn validate_snap_candidate(
    backend: &SnapBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    candidate: &PackageCandidate,
) -> AllpResult<ValidatedSnap> {
    let preferred = candidate
        .metadata
        .get(SNAP_TRANSPORT_KEY)
        .map(String::as_str)
        .unwrap_or(SnapTransport::Rest.as_str());
    let resolution = if preferred == SnapTransport::Cli.as_str() {
        resolve_with_cli(
            backend,
            commands,
            runner,
            candidate,
            candidate
                .metadata
                .get(SNAP_FALLBACK_REASON_KEY)
                .cloned()
                .unwrap_or_else(|| "candidate was discovered through the CLI fallback".to_owned()),
        )?
    } else {
        let socket = candidate
            .metadata
            .get(SNAP_SOCKET_KEY)
            .map(PathBuf::from)
            .unwrap_or_else(snapd_socket_path);
        let rest = SnapdRestService {
            backend,
            client: SnapdClient::new(socket),
        };
        match rest.resolve_result(candidate) {
            Ok(resolution) => resolution,
            Err(error) if error.allows_cli_fallback() => resolve_with_cli(
                backend,
                commands,
                runner,
                candidate,
                error.fallback_reason(),
            )?,
            Err(error) => return Err(snapd_error(backend, "exact resolution", error)),
        }
    };
    match resolution.info {
        Some(info) => Ok(ValidatedSnap {
            info,
            transport: resolution.transport,
            socket_path: (resolution.transport == SnapTransport::Rest).then(|| {
                candidate
                    .metadata
                    .get(SNAP_SOCKET_KEY)
                    .map(PathBuf::from)
                    .unwrap_or_else(snapd_socket_path)
            }),
            fallback_reason: resolution.fallback_reason,
        }),
        None => Err(snap_resolution_unavailable_error(
            backend,
            candidate,
            &resolution,
        )),
    }
}

fn resolve_with_cli(
    backend: &SnapBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    candidate: &PackageCandidate,
    fallback_reason: String,
) -> AllpResult<SnapResolution> {
    let snap = commands.get("snap").ok_or_else(|| {
        AllpError::BackendNotDetected(format!(
            "Snap REST exact resolution unavailable ({fallback_reason}); snap CLI fallback was not found"
        ))
    })?;
    SnapCliFallbackService {
        backend,
        snap,
        runner,
        fallback_reason,
    }
    .resolve(candidate)
}

fn snap_find_command(snap: &Path, query: &str) -> NativeCommand {
    NativeCommand::new(snap).args(["find", query])
}

pub fn snapd_socket_path() -> PathBuf {
    std::env::var_os("ALLP_SNAPD_SOCKET")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/snapd.socket"))
}

fn snapd_error(backend: &SnapBackend, stage: &str, error: SnapdRestError) -> AllpError {
    let code = match &error {
        SnapdRestError::Daemon { status_code, .. } => Some(i32::from(*status_code)),
        _ => None,
    };
    AllpError::CommandFailed {
        backend: backend.display_name().to_owned(),
        command: format!("snapd REST {stage}"),
        code,
        stderr: error.to_string(),
    }
}

fn snapd_diagnostic_command(method: &str, path: &str, socket: &Path) -> (NativeCommand, String) {
    (
        NativeCommand::new("snapd-rest").args([method, path]),
        format!("{method} http://localhost{path} via {}", socket.display()),
    )
}

fn snap_resolution_status_command(
    commands: &CommandMap,
    candidate: &PackageCandidate,
) -> AllpResult<(NativeCommand, String)> {
    let package_id = normalize_snap_package_id(&candidate.package_id)?;
    if candidate
        .metadata
        .get(SNAP_TRANSPORT_KEY)
        .map_or(true, |transport| transport == SnapTransport::Rest.as_str())
    {
        let socket = candidate
            .metadata
            .get(SNAP_SOCKET_KEY)
            .map(PathBuf::from)
            .unwrap_or_else(snapd_socket_path);
        let path = format!("/v2/find?name={}", percent_encode(&package_id));
        return Ok(snapd_diagnostic_command("GET", &path, &socket));
    }
    let snap = commands.get("snap").ok_or_else(|| {
        AllpError::BackendNotDetected("snap CLI fallback executable was not found".to_owned())
    })?;
    let command = NativeCommand::new(snap).args(["info", package_id.as_str()]);
    let display = render_native_command(&command);
    Ok((command, display))
}

fn parse_snapd_candidate(
    result: &Value,
    socket_path: &Path,
) -> Result<SnapCandidate, SnapdRestError> {
    let canonical_name = result
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            SnapdRestError::UnrecognizedResponse(
                "snapd discovery candidate did not include a name".to_owned(),
            )
        })?
        .to_owned();
    let mut notes = Vec::new();
    if let Some(confinement) = result.get("confinement").and_then(Value::as_str) {
        notes.push(confinement.to_owned());
    }
    Ok(SnapCandidate {
        canonical_name,
        discovery_version: result
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned),
        discovery_status: result
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned),
        discovery_publisher: parse_snapd_publisher(result.get("publisher")),
        discovery_summary: result
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_owned),
        discovery_notes: notes,
        availability: SnapAvailability::Discovered,
        resolved: None,
        transport: SnapTransport::Rest,
        fallback_reason: None,
        socket_path: Some(socket_path.to_path_buf()),
    })
}

fn parse_snapd_info(
    result: &Value,
    raw_output: &str,
    fallback_name: &str,
) -> Result<SnapInfo, SnapdRestError> {
    let name = result
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(fallback_name)
        .to_owned();
    let mut channels = parse_snapd_channels(result.get("channels"));
    if channels.is_empty() {
        if let Some(channel) = parse_snapd_top_level_channel(result) {
            channels.push(channel);
        }
    }
    let confinement = result
        .get("confinement")
        .and_then(Value::as_str)
        .map(SnapConfinement::parse)
        .and_then(SnapConfinement::known)
        .or_else(|| {
            channels
                .iter()
                .find_map(|channel| channel.confinement.known())
        })
        .unwrap_or(SnapConfinement::Unknown);
    let mut architectures = parse_snapd_architectures(result.get("architectures"));
    for channel in result
        .get("channels")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|channels| channels.values())
    {
        if let Some(architecture) = channel.get("architecture").and_then(Value::as_str) {
            if !architectures.iter().any(|current| current == architecture) {
                architectures.push(architecture.to_owned());
            }
        }
    }
    Ok(SnapInfo {
        name,
        title: result
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned),
        summary: result
            .get("summary")
            .and_then(Value::as_str)
            .map(str::to_owned),
        description: result
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
        version: result
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned),
        publisher: parse_snapd_publisher(result.get("publisher")),
        confinement,
        channels,
        architectures,
        raw_output: raw_output.to_owned(),
        validation_warning: None,
        installed: None,
    })
}

fn parse_snapd_publisher(value: Option<&Value>) -> Option<SnapPublisher> {
    let value = value?;
    if let Some(name) = value.as_str() {
        return Some(normalize_snap_publisher(name));
    }
    let publisher = value.as_object()?;
    let name = publisher
        .get("display-name")
        .or_else(|| publisher.get("username"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned);
    let verification = match publisher
        .get("validation")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
    {
        "verified" | "starred" => SnapPublisherVerification::Verified,
        "unproven" | "unverified" => SnapPublisherVerification::Unverified,
        _ => SnapPublisherVerification::Unknown,
    };
    Some(SnapPublisher { name, verification })
}

fn parse_snapd_channels(value: Option<&Value>) -> Vec<SnapChannel> {
    let Some(channels) = value.and_then(Value::as_object) else {
        return Vec::new();
    };
    channels
        .iter()
        .map(|(name, details)| SnapChannel {
            name: name.clone(),
            risk: SnapChannelRisk::parse(name),
            version: details
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
            confinement: details
                .get("confinement")
                .and_then(Value::as_str)
                .map(SnapConfinement::parse)
                .unwrap_or(SnapConfinement::Unknown),
            available: !details.is_null(),
        })
        .collect()
}

fn parse_snapd_top_level_channel(result: &Value) -> Option<SnapChannel> {
    let channel = result.get("channel").and_then(Value::as_str)?.trim();
    if channel.is_empty() {
        return None;
    }
    let name = normalize_snapd_channel_name(channel);
    Some(SnapChannel {
        name: name.clone(),
        risk: SnapChannelRisk::parse(&name),
        version: result
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned),
        confinement: result
            .get("confinement")
            .and_then(Value::as_str)
            .map(SnapConfinement::parse)
            .unwrap_or(SnapConfinement::Unknown),
        available: result
            .get("status")
            .and_then(Value::as_str)
            .map_or(true, |status| status.eq_ignore_ascii_case("available")),
    })
}

fn normalize_snapd_channel_name(channel: &str) -> String {
    if channel.contains('/') {
        channel.to_owned()
    } else {
        format!("latest/{channel}")
    }
}

fn parse_snapd_architectures(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flat_map(|architectures| architectures.iter())
        .filter_map(|architecture| {
            architecture
                .as_str()
                .or_else(|| architecture.get("name").and_then(Value::as_str))
        })
        .map(str::to_ascii_lowercase)
        .collect()
}

fn snap_info_command(snap: &Path, candidate: &PackageCandidate) -> AllpResult<NativeCommand> {
    let package_id = normalize_snap_package_id(&candidate.package_id)?;
    Ok(NativeCommand::new(snap).args(["info", package_id.as_str()]))
}

fn normalize_snap_package_id(raw: &str) -> AllpResult<String> {
    let without_ansi = strip_ansi_escape_sequences(raw);
    let trimmed = without_ansi.trim();
    if trimmed.is_empty() {
        return Err(AllpError::ValidationFailed {
            backend: "Snap".to_owned(),
            message: "selected Snap package ID is empty after normalization".to_owned(),
        });
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(AllpError::ValidationFailed {
            backend: "Snap".to_owned(),
            message: "selected Snap package ID contains an embedded newline".to_owned(),
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AllpError::ValidationFailed {
            backend: "Snap".to_owned(),
            message: "selected Snap package ID contains control characters".to_owned(),
        });
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(AllpError::ValidationFailed {
            backend: "Snap".to_owned(),
            message: "selected Snap package ID contains embedded whitespace".to_owned(),
        });
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '.' | '_'))
    {
        return Err(AllpError::ValidationFailed {
            backend: "Snap".to_owned(),
            message: "selected Snap package ID contains unsupported decoration characters"
                .to_owned(),
        });
    }

    Ok(trimmed.to_owned())
}

fn strip_ansi_escape_sequences(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            if chars.next_if_eq(&'[').is_some() {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        output.push(character);
    }
    output
}

fn parse_snap_find(output: &str) -> Vec<SnapCandidate> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("Name ") || line.starts_with("No matching") {
                return None;
            }
            let columns = line.split_whitespace().collect::<Vec<_>>();
            if columns.len() < 4 {
                return None;
            }

            let notes = columns
                .get(3)
                .filter(|value| !matches!(**value, "-" | "—"))
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            Some(SnapCandidate {
                canonical_name: columns[0].to_owned(),
                discovery_version: columns.get(1).map(|value| (*value).to_owned()),
                discovery_status: None,
                discovery_publisher: columns.get(2).map(|value| normalize_snap_publisher(value)),
                discovery_summary: (columns.len() > 4).then(|| columns[4..].join(" ")),
                discovery_notes: notes,
                availability: SnapAvailability::Discovered,
                resolved: None,
                transport: SnapTransport::Cli,
                fallback_reason: None,
                socket_path: None,
            })
        })
        .collect()
}

fn parse_snap_info(output: &str, fallback_name: &str) -> AllpResult<SnapInfo> {
    let mut fields: BTreeMap<String, String> = BTreeMap::new();
    let mut channels = Vec::new();
    let mut current_key: Option<String> = None;
    let mut in_channels = false;
    let mut saw_field = false;

    for line in output.lines() {
        if line.trim().is_empty() {
            current_key = None;
            in_channels = false;
            continue;
        }

        if line.starts_with(' ') || line.starts_with('\t') {
            let trimmed = line.trim();
            if in_channels {
                if let Some(channel) = parse_snap_channel(trimmed) {
                    channels.push(channel);
                    continue;
                }
            }
            if let Some(key) = current_key
                .as_ref()
                .filter(|key| key.as_str() != "channels")
            {
                let entry = fields.entry(key.clone()).or_default();
                if entry == "|" {
                    entry.clear();
                }
                if !entry.is_empty() {
                    entry.push(' ');
                }
                entry.push_str(trimmed);
            }
            continue;
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        saw_field = true;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_owned();
        in_channels = key == "channels";
        current_key = Some(key.clone());
        if !in_channels {
            fields.insert(key, value);
        }
    }

    if !saw_field {
        return Err(AllpError::MetadataParseFailed {
            backend: "Snap".to_owned(),
            message: format!(
                "Could not find key/value metadata in `snap info` output for {fallback_name}.\nRaw stdout:\n{}",
                bounded_block(output)
            ),
        });
    }

    let name = fields
        .get("name")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| fallback_name.to_owned());
    let confinement = fields
        .get("confinement")
        .map(|value| SnapConfinement::parse(value))
        .and_then(SnapConfinement::known)
        .or_else(|| {
            channels
                .iter()
                .find_map(|channel| channel.confinement.known())
        })
        .unwrap_or(SnapConfinement::Unknown);

    Ok(SnapInfo {
        name,
        title: fields
            .get("title")
            .cloned()
            .filter(|value| !value.trim().is_empty()),
        summary: fields
            .get("summary")
            .cloned()
            .filter(|value| !value.trim().is_empty()),
        description: fields
            .get("description")
            .cloned()
            .filter(|value| !value.trim().is_empty() && value.trim() != "|"),
        version: fields
            .get("version")
            .cloned()
            .filter(|value| !value.trim().is_empty()),
        publisher: fields
            .get("publisher")
            .map(|value| normalize_snap_publisher(value)),
        confinement,
        channels,
        architectures: fields
            .get("architectures")
            .map(|value| parse_snap_architectures(value))
            .unwrap_or_default(),
        raw_output: output.to_owned(),
        validation_warning: None,
        installed: None,
    })
}

fn parse_snap_channel(line: &str) -> Option<SnapChannel> {
    let (name, value) = line.split_once(':')?;
    let name = name.trim();
    if name.is_empty() || !name.contains('/') {
        return None;
    }
    let value = value.trim();
    let available = !value.is_empty() && !matches!(value, "-" | "—");
    let first_token = value.split_whitespace().next();
    let version = first_token
        .filter(|token| !matches!(*token, "↑" | "-" | "—"))
        .filter(|token| !token.starts_with('('))
        .map(str::to_owned);

    Some(SnapChannel {
        name: name.to_owned(),
        risk: SnapChannelRisk::parse(name),
        version,
        confinement: SnapConfinement::parse(value),
        available,
    })
}

fn normalize_snap_publisher(value: &str) -> SnapPublisher {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return SnapPublisher {
            name: None,
            verification: SnapPublisherVerification::Unknown,
        };
    }

    let verified = trimmed.ends_with("**") || trimmed.contains('✓');
    let name = trimmed
        .trim_end_matches('*')
        .trim_end_matches('✓')
        .trim()
        .trim_matches(|character: char| character == '(' || character == ')')
        .to_owned();

    SnapPublisher {
        name: (!name.is_empty()).then_some(name),
        verification: if verified {
            SnapPublisherVerification::Verified
        } else {
            SnapPublisherVerification::Unverified
        },
    }
}

fn parse_snap_architectures(value: &str) -> Vec<String> {
    value
        .split(|character: char| character == ',' || character.is_whitespace())
        .map(|part| part.trim().trim_matches(['[', ']']))
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn validate_snap_architecture(
    backend: &SnapBackend,
    candidate: &PackageCandidate,
    info: &SnapInfo,
) -> AllpResult<()> {
    if info.architectures.is_empty() {
        return Ok(());
    }
    let current = current_snap_architecture();
    if info
        .architectures
        .iter()
        .any(|arch| arch == "all" || arch == &current || arch == std::env::consts::ARCH)
    {
        return Ok(());
    }

    Err(snap_unavailable_message(
        backend,
        candidate,
        SnapAvailability::Unavailable,
        format!(
            "Snap \"{}\" is not available for architecture {}. Available architectures: {}",
            info.name,
            current,
            info.architectures.join(", ")
        ),
    ))
}

fn current_snap_architecture() -> String {
    match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "armhf",
        "powerpc64" => "ppc64el",
        "s390x" => "s390x",
        other => other,
    }
    .to_owned()
}

fn select_snap_install_channel(
    backend: &SnapBackend,
    candidate: &PackageCandidate,
    info: &SnapInfo,
) -> AllpResult<Option<SnapChannel>> {
    if info.channels.is_empty() {
        return Err(snap_unavailable_message(
            backend,
            candidate,
            SnapAvailability::Unavailable,
            format!(
                "Snap \"{}\" has no installable channels in exact metadata.",
                info.name
            ),
        ));
    }

    let stable_channels = info
        .channels
        .iter()
        .filter(|channel| channel.available && channel.risk == SnapChannelRisk::Stable)
        .cloned()
        .collect::<Vec<_>>();

    if let Some(channel) = stable_channels
        .iter()
        .find(|channel| channel.name == "latest/stable")
    {
        return Ok(Some(channel.clone()));
    }

    if stable_channels.len() == 1 {
        return Ok(stable_channels.first().cloned());
    }

    if stable_channels.is_empty() {
        let channels = info
            .channels
            .iter()
            .filter(|channel| channel.available)
            .map(|channel| format!("{} ({})", channel.name, channel.risk.label()))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(AllpError::AmbiguousSelection(format!(
            "Snap \"{}\" has no stable channel available.\n\nAvailable channels: {}\n\nAllp will not silently choose candidate, beta, or edge for installation.",
            info.name,
            if channels.is_empty() { "none".to_owned() } else { channels }
        )));
    }

    let tracks = stable_channels
        .iter()
        .map(|channel| channel.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Err(AllpError::AmbiguousSelection(format!(
        "Snap \"{}\" has multiple stable tracks: {tracks}.\n\nChoose a track/channel explicitly with snap or retry after narrowing the package selection.",
        info.name
    )))
}

fn inspect_snap_installed(
    snap: &Path,
    runner: &dyn ProcessRunner,
    package_id: &str,
) -> AllpResult<Option<InstalledSnap>> {
    let output = runner.capture(
        &NativeCommand::new(snap)
            .args(["list", package_id])
            .timeout(Duration::from_secs(10)),
    )?;
    if output.success {
        return Ok(parse_snap_list_entry(&output.stdout, package_id));
    }

    let message = output_message(&output);
    let lower = message.to_ascii_lowercase();
    if lower.contains("not found")
        || lower.contains("no matching")
        || lower.contains("not installed")
    {
        return Ok(None);
    }

    Err(AllpError::ValidationFailed {
        backend: "Snap".to_owned(),
        message: format!(
            "Could not inspect installed state for Snap \"{package_id}\" before planning installation.\n{}",
            message.trim()
        ),
    })
}

fn parse_snap_list_entry(output: &str, package_id: &str) -> Option<InstalledSnap> {
    output.lines().find_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Name ") {
            return None;
        }
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if !columns
            .first()
            .is_some_and(|name| name.eq_ignore_ascii_case(package_id))
        {
            return None;
        }
        Some(InstalledSnap {
            version: columns.get(1).map(|value| (*value).to_owned()),
            channel: columns.get(3).map(|value| (*value).to_owned()),
        })
    })
}

fn resolve_snap_candidate(
    candidate: &PackageCandidate,
    validated: ValidatedSnap,
    selected_channel: Option<SnapChannel>,
) -> PackageCandidate {
    let ValidatedSnap {
        info,
        transport,
        socket_path,
        fallback_reason,
    } = validated;
    let mut resolved = candidate.clone();
    resolved.package_id = info.name.clone();
    resolved.display_name = info.title.clone().unwrap_or_else(|| info.name.clone());
    resolved.version = info.version.clone().or_else(|| {
        selected_channel
            .as_ref()
            .and_then(|channel| channel.version.clone())
    });
    resolved.description = info
        .summary
        .clone()
        .or(info.description.clone())
        .or_else(|| candidate.description.clone());
    resolved.source = info.publisher.as_ref().map(SnapPublisher::human_label);
    resolved.scope = Some("system".to_owned());
    resolved.metadata = snap_candidate_metadata(&info, selected_channel.as_ref());
    resolved
        .metadata
        .insert(SNAP_TRANSPORT_KEY.to_owned(), transport.as_str().to_owned());
    if let Some(socket_path) = socket_path {
        resolved.metadata.insert(
            SNAP_SOCKET_KEY.to_owned(),
            socket_path.display().to_string(),
        );
    }
    if let Some(reason) = fallback_reason {
        resolved
            .metadata
            .insert(SNAP_FALLBACK_REASON_KEY.to_owned(), reason);
    }
    resolved
}

fn snap_candidate_metadata(
    info: &SnapInfo,
    selected_channel: Option<&SnapChannel>,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        SNAP_AVAILABILITY_KEY.to_owned(),
        SnapAvailability::Available.as_str().to_owned(),
    );
    metadata.insert("snap.canonical_name".to_owned(), info.name.clone());
    metadata.insert(
        "snap.confinement".to_owned(),
        info.confinement.as_str().to_owned(),
    );
    metadata.insert("snap.raw_info".to_owned(), bounded_block(&info.raw_output));
    if let Some(publisher) = &info.publisher {
        if let Some(name) = &publisher.name {
            metadata.insert("snap.publisher_name".to_owned(), name.clone());
        }
        metadata.insert(
            "snap.publisher_verification".to_owned(),
            publisher.verification.as_str().to_owned(),
        );
    }
    if let Some(channel) = selected_channel {
        metadata.insert("snap.channel".to_owned(), channel.name.clone());
        metadata.insert(
            "snap.channel_risk".to_owned(),
            channel.risk.label().to_owned(),
        );
        if let Some(version) = &channel.version {
            metadata.insert("snap.channel_version".to_owned(), version.clone());
        }
    }
    let stable_available = info
        .channels
        .iter()
        .any(|channel| channel.available && channel.risk == SnapChannelRisk::Stable);
    metadata.insert(
        "snap.stable_available".to_owned(),
        stable_available.to_string(),
    );
    let tracks = info
        .channels
        .iter()
        .filter(|channel| channel.available)
        .map(SnapChannel::track)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ");
    if !tracks.is_empty() {
        metadata.insert("snap.tracks".to_owned(), tracks);
    }
    if !info.architectures.is_empty() {
        metadata.insert(
            "snap.architectures".to_owned(),
            info.architectures.join(", "),
        );
    }
    metadata
}

fn snap_plan_details(candidate: &PackageCandidate) -> Vec<(String, String)> {
    let mut details = Vec::new();
    if candidate.display_name != candidate.package_id {
        details.push(("Software".to_owned(), candidate.display_name.clone()));
    }
    if let Some(publisher) = &candidate.source {
        details.push(("Publisher".to_owned(), publisher.clone()));
    }
    if let Some(channel) = candidate.metadata.get("snap.channel") {
        details.push(("Channel".to_owned(), channel.clone()));
    }
    if let Some(confinement) = candidate.metadata.get("snap.confinement") {
        details.push((
            "Confinement".to_owned(),
            SnapConfinement::parse(confinement).label().to_owned(),
        ));
    }
    if let Some(architectures) = candidate.metadata.get("snap.architectures") {
        details.push(("Architectures".to_owned(), architectures.clone()));
    }
    details
}

fn snapd_install_plan(
    backend: &SnapBackend,
    candidate: &PackageCandidate,
) -> AllpResult<ExecutionPlan> {
    let socket = candidate
        .metadata
        .get(SNAP_SOCKET_KEY)
        .map(PathBuf::from)
        .unwrap_or_else(snapd_socket_path);
    let channel = candidate
        .metadata
        .get("snap.channel")
        .cloned()
        .ok_or_else(|| AllpError::CandidateUnavailable {
            backend: backend.display_name().to_owned(),
            message: "Snap exact metadata did not select an installable channel".to_owned(),
        })?;
    let classic = candidate
        .metadata
        .get("snap.confinement")
        .is_some_and(|value| value.eq_ignore_ascii_case("classic"));
    let executable = std::env::current_exe().map_err(AllpError::Io)?;
    let mut command = NativeCommand::new(executable).args([
        "internal-snapd-install",
        "--socket",
        socket.to_string_lossy().as_ref(),
        "--name",
        candidate.package_id.as_str(),
        "--channel",
        channel.as_str(),
    ]);
    if classic {
        command = command.arg("--classic");
    }
    let mut details = snap_plan_details(candidate);
    details.push(("Transport".to_owned(), "snapd REST API".to_owned()));
    details.push((
        "Request".to_owned(),
        format!(
            "POST http://localhost/v2/snaps/{} via {}",
            percent_encode(&candidate.package_id),
            socket.display()
        ),
    ));
    Ok(ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation: OperationKind::Install,
        action: "Install Snap application through snapd".to_owned(),
        package_id: Some(candidate.package_id.clone()),
        source: candidate.source.clone(),
        scope: candidate.scope.clone(),
        details,
        command,
        privilege: PrivilegeRequirement::RootRequired,
        requires_root: true,
        interactive: false,
    })
}

pub fn run_snapd_install(
    socket: &Path,
    name: &str,
    channel: &str,
    classic: bool,
) -> AllpResult<()> {
    let name = normalize_snap_package_id(name)?;
    if channel.trim().is_empty() || channel.chars().any(char::is_control) {
        return Err(AllpError::InvalidInput(
            "Snap install channel is invalid".to_owned(),
        ));
    }
    let backend = SnapBackend;
    let service = SnapdRestService {
        backend: &backend,
        client: SnapdClient::new(socket),
    };
    let change_id = service.install(SnapInstallRequest {
        name,
        channel: channel.to_owned(),
        classic,
    })?;
    let started = Instant::now();
    let timeout = Duration::from_secs(10 * 60);
    let mut previous_status = String::new();
    loop {
        if started.elapsed() >= timeout {
            return Err(AllpError::Timeout(format!(
                "snapd change {} did not finish within {} seconds",
                change_id.0,
                timeout.as_secs()
            )));
        }
        let change = service.change(&change_id)?;
        if change.status != previous_status {
            println!("snapd change {}: {}", change_id.0, change.status);
            previous_status = change.status.clone();
        }
        if change.ready {
            if change.status == "Done" && change.error.is_none() {
                return Ok(());
            }
            return Err(AllpError::CommandFailed {
                backend: backend.display_name().to_owned(),
                command: format!("GET /v2/changes/{}", change_id.0),
                code: None,
                stderr: change
                    .error
                    .unwrap_or_else(|| format!("snapd change ended with status {}", change.status)),
            });
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn snap_info_extra(info: &SnapInfo) -> Vec<(String, String)> {
    let mut extra = Vec::new();
    if let Some(publisher) = &info.publisher {
        extra.push((
            "Publisher verification".to_owned(),
            publisher.verification.label().to_owned(),
        ));
    }
    extra.push((
        "Confinement".to_owned(),
        info.confinement.label().to_owned(),
    ));
    if !info.channels.is_empty() {
        extra.push((
            "Channels".to_owned(),
            info.channels
                .iter()
                .filter(|channel| channel.available)
                .map(|channel| channel.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    if !info.architectures.is_empty() {
        extra.push(("Architectures".to_owned(), info.architectures.join(", ")));
    }
    extra
}

fn candidate_version_label(
    info: &SnapInfo,
    selected_channel: Option<&SnapChannel>,
) -> Option<String> {
    let version = info
        .version
        .as_deref()
        .or_else(|| selected_channel.and_then(|channel| channel.version.as_deref()));
    let channel = selected_channel.map(|channel| channel.name.as_str());
    version_channel_label(version, channel)
}

fn version_channel_label(version: Option<&str>, channel: Option<&str>) -> Option<String> {
    match (version, channel) {
        (Some(version), Some(channel)) => Some(format!("{version} ({channel})")),
        (Some(version), None) => Some(version.to_owned()),
        (None, Some(channel)) => Some(format!("channel {channel}")),
        (None, None) => None,
    }
}

fn snap_validation_start_error(backend: &SnapBackend, snap: &Path, error: AllpError) -> AllpError {
    AllpError::ValidationStartFailed {
        backend: backend.display_name().to_owned(),
        executable: snap.to_string_lossy().into_owned(),
        reason: error.to_string(),
    }
}

fn snap_resolution_unavailable_error(
    backend: &SnapBackend,
    candidate: &PackageCandidate,
    resolution: &SnapResolution,
) -> AllpError {
    let package = candidate.package_id.as_str();
    let native_error = resolution
        .native_error
        .as_deref()
        .unwrap_or("exact Snap metadata is unavailable");
    let mut message = format!(
        "The package was returned by Snap search, but exact installable\nmetadata is not available for this system.\n\nPackage: {package}\nSearch status: Found\nInstall status: {}\nNative error: {native_error}",
        match resolution.availability {
            SnapAvailability::Stale => "Stale",
            SnapAvailability::BackendError => "BackendError",
            _ => "Unavailable",
        }
    );
    message.push_str(&format!(
        "\nResolution command: {}",
        resolution.display_command
    ));
    if let Some(output) = &resolution.output {
        message.push_str(&format!(
            "\nResolution exit code: {}",
            output
                .code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unavailable".to_owned())
        ));
    }
    if let Some(status) = resolution.rest_status {
        message.push_str(&format!("\nResolution HTTP status: {status}"));
    }
    message.push_str(&format!(
        "\nResolution transport: {}",
        resolution.transport.as_str()
    ));
    if let Some(reason) = &resolution.fallback_reason {
        message.push_str(&format!("\nCLI fallback reason: {reason}"));
    }
    if resolution.availability == SnapAvailability::Stale {
        message.push_str(
            "\n\nThis looks like a stale or search-only Snap Store result. Native `snap info` and `snap install` use exact name resolution too, so Allp stops before requesting sudo.",
        );
    }

    AllpError::CandidateUnavailable {
        backend: backend.display_name().to_owned(),
        message,
    }
}

fn snap_unavailable_message(
    backend: &SnapBackend,
    candidate: &PackageCandidate,
    availability: SnapAvailability,
    native_error: String,
) -> AllpError {
    let package = candidate.package_id.as_str();
    AllpError::CandidateUnavailable {
        backend: backend.display_name().to_owned(),
        message: format!(
            "The package was returned by Snap search, but exact installable\nmetadata is not available for this system.\n\nPackage: {package}\nSearch status: Found\nInstall status: {}\nNative error: {native_error}",
            match availability {
                SnapAvailability::Stale => "Stale",
                SnapAvailability::BackendError => "BackendError",
                _ => "Unavailable",
            }
        ),
    }
}

fn snap_error_text(output: &CommandOutput) -> String {
    let message = output_message(output);
    if !message.trim().is_empty() {
        return message.trim().to_owned();
    }
    if let Some(signal) = output.signal {
        return format!("Snap terminated by signal {signal} without an error message.");
    }
    if let Some(code) = output.code {
        return format!("Snap returned exit code {code} without an error message.");
    }
    "Snap failed without an exit code or error message.".to_owned()
}

fn bounded_block(value: &str) -> String {
    const LIMIT: usize = 8_000;
    if value.trim().is_empty() {
        return "<empty>".to_owned();
    }
    if value.len() <= LIMIT {
        return value.trim_end().to_owned();
    }
    format!(
        "{}\n... <truncated {} byte(s)>",
        &value[..LIMIT],
        value.len() - LIMIT
    )
}

fn output_message(output: &CommandOutput) -> String {
    let mut message = String::new();
    if !output.stderr.trim().is_empty() {
        message.push_str(output.stderr.trim());
    }
    if !output.stdout.trim().is_empty() {
        if !message.is_empty() {
            message.push('\n');
        }
        message.push_str(output.stdout.trim());
    }
    message
}

fn print_snap_diagnostics(
    _backend: &SnapBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    candidate: &PackageCandidate,
) {
    let discovery_query = candidate
        .metadata
        .get(SNAP_DISCOVERY_QUERY_KEY)
        .map(String::as_str)
        .unwrap_or(candidate.package_id.as_str());

    println!("\nSnap diagnostics\n");
    let transport = candidate
        .metadata
        .get(SNAP_TRANSPORT_KEY)
        .map(String::as_str)
        .unwrap_or(SnapTransport::Rest.as_str());
    if transport == SnapTransport::Rest.as_str() {
        let socket = candidate
            .metadata
            .get(SNAP_SOCKET_KEY)
            .map(PathBuf::from)
            .unwrap_or_else(snapd_socket_path);
        let client = SnapdClient::new(&socket);
        let discovery_path = format!("/v2/find?q={}&scope=wide", percent_encode(discovery_query));
        let exact_path = format!("/v2/find?name={}", percent_encode(&candidate.package_id));
        println!("Transport:");
        println!("  snapd REST API");
        println!("Socket:");
        println!("  {}", socket.display());
        println!();
        print_snapd_diagnostic_request("Discovery", &client, &discovery_path);
        println!();
        let state = print_snapd_diagnostic_request("Exact resolution", &client, &exact_path);
        println!();
        println!("Candidate state:");
        println!("  {}", state.as_str());
    } else if let Some(snap) = commands.get("snap") {
        let discovery_command = snap_find_command(snap, discovery_query);
        let exact_command = match snap_info_command(snap, candidate) {
            Ok(command) => command,
            Err(error) => {
                println!("Unable to construct Snap CLI diagnostics: {error}");
                return;
            }
        };
        println!("Transport:");
        println!("  Snap CLI fallback");
        println!("Executable:");
        println!("  {}", snap.display());
        if let Some(reason) = candidate.metadata.get(SNAP_FALLBACK_REASON_KEY) {
            println!("Fallback reason:");
            println!("  {reason}");
        }
        let _ = print_snap_cli_diagnostic_request("Discovery", runner, &discovery_command);
        println!();
        println!("Exact resolution command:");
        println!("  {}", render_native_command(&exact_command));
        let state = print_snap_cli_diagnostic_request("Resolution", runner, &exact_command);
        println!();
        println!("Candidate state:");
        println!("  {}", state.as_str());
    } else {
        println!("Snap CLI fallback executable is unavailable.");
    }
    println!();
    println!("Architecture:");
    println!("  {}", current_snap_architecture());
}

fn print_snapd_diagnostic_request(
    label: &str,
    client: &SnapdClient,
    path: &str,
) -> SnapAvailability {
    println!("{label} request:");
    println!("  GET http://localhost{path}");
    match client.get(path) {
        Ok(response) => {
            println!("{label} status:");
            println!(
                "  HTTP {} / snapd {}",
                response.http_status, response.status_code
            );
            println!("{label} response:");
            println!("  {}", indent_block(&bounded_block(&response.raw_body)));
            SnapAvailability::Available
        }
        Err(SnapdRestError::Daemon {
            status_code,
            kind,
            message,
            raw_body,
        }) => {
            println!("{label} status:");
            println!("  snapd {status_code}");
            println!("Error kind:");
            println!("  {}", kind.as_deref().unwrap_or("unknown"));
            println!("Error:");
            println!("  {message}");
            println!("{label} response:");
            println!("  {}", indent_block(&bounded_block(&raw_body)));
            if status_code == 404 && kind.as_deref() == Some("snap-not-found") {
                SnapAvailability::Stale
            } else {
                SnapAvailability::BackendError
            }
        }
        Err(error) => {
            println!("{label} error:");
            println!("  {error}");
            SnapAvailability::BackendError
        }
    }
}

fn print_snap_cli_diagnostic_request(
    label: &str,
    runner: &dyn ProcessRunner,
    command: &NativeCommand,
) -> SnapAvailability {
    println!("{label} command:");
    println!("  {}", render_native_command(command));
    match runner.capture(command) {
        Ok(output) => {
            println!("{label} exit code:");
            println!(
                "  {}",
                output
                    .code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unavailable".to_owned())
            );
            if let Some(signal) = output.signal {
                println!("{label} signal:");
                println!("  {signal}");
            }
            println!("{label} stdout:");
            println!("  {}", indent_block(&bounded_block(&output.stdout)));
            println!("{label} stderr:");
            println!("  {}", indent_block(&bounded_block(&output.stderr)));
            if output.success {
                SnapAvailability::Available
            } else if classify_snap_error(&output_message(&output))
                == Some(SnapErrorKind::PackageNotFound)
            {
                SnapAvailability::Stale
            } else {
                SnapAvailability::BackendError
            }
        }
        Err(error) => {
            println!("{label} error:");
            println!("  {error}");
            SnapAvailability::BackendError
        }
    }
}

fn read_snap_action_line(eof_message: &str) -> AllpResult<String> {
    let mut input = String::new();
    input.clear();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(AllpError::Timeout(eof_message.to_owned()));
    }
    Ok(input.trim().to_owned())
}

fn indent_block(value: &str) -> String {
    value.lines().collect::<Vec<_>>().join("\n  ")
}

fn classify_snap_error(output: &str) -> Option<SnapErrorKind> {
    let lower = output.to_ascii_lowercase();
    if lower.trim().is_empty() {
        return None;
    }
    if lower.contains("permission denied") || lower.contains("access denied") {
        return Some(SnapErrorKind::PermissionDenied);
    }
    if lower.contains("already installed") {
        return Some(SnapErrorKind::AlreadyInstalled);
    }
    if lower.contains("requires classic confinement")
        || lower.contains("classic confinement") && lower.contains("required")
    {
        return Some(SnapErrorKind::ClassicConfinementRequired);
    }
    if lower.contains("classic confinement") && lower.contains("not supported") {
        return Some(SnapErrorKind::ClassicConfinementUnsupported);
    }
    if lower.contains("architecture")
        && (lower.contains("unsupported") || lower.contains("not available"))
    {
        return Some(SnapErrorKind::ArchitectureUnsupported);
    }
    if lower.contains("channel") && (lower.contains("not found") || lower.contains("unavailable")) {
        return Some(SnapErrorKind::ChannelUnavailable);
    }
    if lower.contains("snap-not-found")
        || lower.contains("not found")
        || lower.contains("no snap found")
        || lower.contains("snap") && lower.contains("not installed") && lower.contains("find")
    {
        return Some(SnapErrorKind::PackageNotFound);
    }
    if lower.contains("cannot communicate with server")
        || lower.contains("snapd")
        || lower.contains("daemon")
    {
        return Some(SnapErrorKind::DaemonUnavailable);
    }
    if lower.contains("store") && (lower.contains("unavailable") || lower.contains("timeout")) {
        return Some(SnapErrorKind::StoreUnavailable);
    }
    None
}

struct PlanSpec {
    operation: OperationKind,
    action: &'static str,
    package_id: Option<String>,
    source: Option<String>,
    scope: Option<String>,
    args: Vec<String>,
}

fn snap_plan(backend: &SnapBackend, program: &std::path::Path, spec: PlanSpec) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation: spec.operation,
        action: spec.action.to_owned(),
        package_id: spec.package_id,
        source: spec.source,
        scope: spec.scope,
        details: Vec::new(),
        command: NativeCommand::new(program).args(spec.args),
        privilege: PrivilegeRequirement::RootRequired,
        requires_root: true,
        interactive: true,
    }
}

fn parse_snap_refresh_status(stdout: &str, stderr: &str) -> Option<(OperationStatus, String)> {
    let output = format!("{stdout}\n{stderr}");
    let lower = output.to_ascii_lowercase();
    if lower.contains("all snaps up to date") {
        return Some((OperationStatus::UpToDate, "all snaps up to date".to_owned()));
    }
    if lower.lines().any(|line| {
        let line = line.trim();
        line.ends_with(" refreshed")
            || line.contains(" refreshed ")
            || line.contains(" refreshed.")
            || line.contains("updated")
    }) {
        return Some((
            OperationStatus::Updated,
            "snap refresh completed".to_owned(),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        classify_snap_error, normalize_snap_package_id, parse_snap_change, parse_snap_find,
        parse_snap_info, parse_snap_list_entry, parse_snap_refresh_status, parse_snapd_info,
        select_snap_install_channel, snap_info_command, snap_install_request_body,
        validate_snap_candidate, SnapBackend, SnapCandidate, SnapChange, SnapChangeId,
        SnapCliFallbackService, SnapConfinement, SnapErrorKind, SnapInstallRequest,
        SnapPublisherVerification, SnapService, SnapWideSearch, SNAP_AVAILABILITY_KEY,
        SNAP_DISCOVERY_NOTES_KEY, SNAP_DISCOVERY_STATUS_KEY, SNAP_FALLBACK_REASON_KEY,
        SNAP_TRANSPORT_KEY,
    };
    use crate::domain::{
        AllpError, AllpResult, BackendCategory, ExecutionPlan, MatchKind, NativeCommand,
        OperationStatus, PackageCandidate, PackageDomain,
    };
    use crate::execution::{CommandOutput, ProcessRunner, ProcessStatus};
    use std::io;
    #[cfg(unix)]
    use std::{os::unix::net::UnixListener, path::Path};

    #[test]
    fn all_snaps_up_to_date_maps_to_up_to_date() {
        let (status, message) =
            parse_snap_refresh_status("All snaps up to date.\n", "").expect("snap output parses");

        assert!(matches!(status, OperationStatus::UpToDate));
        assert_eq!(message, "all snaps up to date");
    }

    #[test]
    fn parses_decorated_verified_publisher_and_classic_fixture() {
        let info = parse_snap_info(
            include_str!("../../../tests/fixtures/snap/pycharm-info.txt"),
            "pycharm",
        )
        .expect("fixture should parse");

        assert_eq!(info.name, "pycharm");
        assert_eq!(info.title.as_deref(), Some("PyCharm"));
        assert_eq!(info.confinement, SnapConfinement::Classic);
        let publisher = info.publisher.as_ref().expect("publisher should parse");
        assert_eq!(publisher.name.as_deref(), Some("JetBrains"));
        assert_eq!(publisher.verification, SnapPublisherVerification::Verified);
        assert!(info
            .channels
            .iter()
            .any(|channel| channel.name == "latest/stable"));
    }

    #[test]
    fn parses_plain_publisher_and_strict_fixture() {
        let info = parse_snap_info(
            include_str!("../../../tests/fixtures/snap/strict-info.txt"),
            "strict-app",
        )
        .expect("fixture should parse");

        assert_eq!(info.confinement, SnapConfinement::Strict);
        let publisher = info.publisher.as_ref().expect("publisher should parse");
        assert_eq!(publisher.name.as_deref(), Some("Example"));
        assert_eq!(
            publisher.verification,
            SnapPublisherVerification::Unverified
        );
        let channel = select_snap_install_channel(&SnapBackend, &candidate("strict-app"), &info)
            .expect("stable channel should select")
            .expect("channel should exist");
        assert_eq!(channel.name, "latest/stable");
    }

    #[test]
    fn edge_only_fixture_has_no_stable_channel() {
        let info = parse_snap_info(
            include_str!("../../../tests/fixtures/snap/edge-only-info.txt"),
            "edge-only",
        )
        .expect("fixture should parse");

        let error = select_snap_install_channel(&SnapBackend, &candidate("edge-only"), &info)
            .expect_err("edge-only should be blocked");
        assert!(error.to_string().contains("no stable channel"));
    }

    #[test]
    fn parses_installed_snap_channel() {
        let installed = parse_snap_list_entry(
            include_str!("../../../tests/fixtures/snap/list-installed.txt"),
            "installed-app",
        )
        .expect("installed row should parse");

        assert_eq!(installed.version.as_deref(), Some("1.0"));
        assert_eq!(installed.channel.as_deref(), Some("latest/beta"));
        assert_eq!(
            installed.version_label().as_deref(),
            Some("1.0 (latest/beta)")
        );
    }

    #[test]
    fn classifies_snap_not_found_error() {
        assert_eq!(
            classify_snap_error(r#"error: snap "pycharm" not found"#),
            Some(SnapErrorKind::PackageNotFound)
        );
    }

    #[test]
    fn normalizes_snap_package_id_before_validation() {
        assert_eq!(
            normalize_snap_package_id("  pycharm  ").expect("trimmed ID should be valid"),
            "pycharm"
        );
        assert_eq!(
            normalize_snap_package_id("\u{1b}[31mpy.charm-pro\u{1b}[0m")
                .expect("ANSI escapes should be stripped"),
            "py.charm-pro"
        );
    }

    #[test]
    fn rejects_decorated_snap_package_ids() {
        assert!(normalize_snap_package_id("pycharm\npublisher").is_err());
        assert!(normalize_snap_package_id("pycharm\tpublisher").is_err());
        assert!(normalize_snap_package_id("pycharm\"").is_err());
        assert!(normalize_snap_package_id("pycharm publisher").is_err());
    }

    #[test]
    fn snap_info_command_uses_exact_argv() {
        let command = snap_info_command(
            std::path::Path::new("/usr/bin/snap"),
            &candidate(" pycharm "),
        )
        .expect("command should build");

        assert_eq!(command.program, std::path::PathBuf::from("/usr/bin/snap"));
        assert_eq!(
            command
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["info".to_owned(), "pycharm".to_owned()]
        );
    }

    #[test]
    fn snapd_install_body_adds_classic_only_when_required() {
        let strict = snap_install_request_body(&SnapInstallRequest {
            name: "strict-app".to_owned(),
            channel: "latest/stable".to_owned(),
            classic: false,
        });
        assert_eq!(strict["action"], "install");
        assert_eq!(strict["channel"], "latest/stable");
        assert!(strict.get("classic").is_none());

        let classic = snap_install_request_body(&SnapInstallRequest {
            name: "classic-app".to_owned(),
            channel: "latest/stable".to_owned(),
            classic: true,
        });
        assert_eq!(classic["classic"], true);
    }

    #[test]
    fn snapd_exact_metadata_preserves_install_relevant_fields() {
        let result = serde_json::json!({
            "name": "pycharm",
            "title": "PyCharm",
            "summary": "Python IDE",
            "version": "2026.1.4",
            "publisher": {
                "display-name": "JetBrains",
                "validation": "verified"
            },
            "confinement": "classic",
            "architectures": [{"name": "amd64"}],
            "channels": {
                "latest/stable": {
                    "version": "2026.1.4",
                    "architecture": "amd64",
                    "confinement": "classic"
                },
                "latest/beta": {
                    "version": "2026.2-beta",
                    "architecture": "amd64",
                    "confinement": "classic"
                }
            }
        });

        let info = parse_snapd_info(&result, "raw snapd response", "fallback")
            .expect("valid snapd metadata should parse");

        assert_eq!(info.name, "pycharm");
        assert_eq!(info.confinement, SnapConfinement::Classic);
        assert_eq!(info.architectures, vec!["amd64"]);
        assert_eq!(
            info.publisher.unwrap().verification,
            SnapPublisherVerification::Verified
        );
        assert!(info
            .channels
            .iter()
            .any(|channel| channel.name == "latest/stable" && channel.available));
    }

    #[test]
    fn snapd_top_level_stable_channel_becomes_latest_stable() {
        let result = serde_json::json!({
            "name": "pycharm",
            "title": "PyCharm",
            "summary": "Python IDE",
            "version": "2026.1.4",
            "publisher": {
                "display-name": "JetBrains",
                "validation": "verified"
            },
            "status": "available",
            "channel": "stable",
            "confinement": "classic"
        });

        let info = parse_snapd_info(&result, "raw snapd response", "fallback")
            .expect("top-level snapd discovery metadata should parse");
        let channel = select_snap_install_channel(&SnapBackend, &candidate("pycharm"), &info)
            .expect("stable top-level channel should select")
            .expect("channel should exist");

        assert_eq!(channel.name, "latest/stable");
        assert_eq!(channel.version.as_deref(), Some("2026.1.4"));
        assert_eq!(channel.confinement, SnapConfinement::Classic);
    }

    #[test]
    fn snapd_change_parsing_recognizes_terminal_success_and_failure() {
        let done = parse_snap_change(&serde_json::json!({"status":"Done","ready":true}));
        assert!(done.ready);
        assert_eq!(done.status, "Done");
        assert!(done.error.is_none());

        let failed = parse_snap_change(
            &serde_json::json!({"status":"Error","ready":true,"err":"store failed"}),
        );
        assert!(failed.ready);
        assert_eq!(failed.error.as_deref(), Some("store failed"));
    }

    #[cfg(unix)]
    #[test]
    fn snapd_discovery_and_exact_not_found_are_separate_rest_requests() {
        use super::{SnapAvailability, SnapdRestService};
        use std::{
            fs,
            io::{Read, Write},
            sync::{Arc, Mutex},
            thread,
        };

        let socket = std::env::temp_dir().join(format!(
            "allp-snap-service-{}-{:?}.sock",
            std::process::id(),
            thread::current().id()
        ));
        let _ = fs::remove_file(&socket);
        let Some(listener) = bind_test_listener(&socket, "fake snapd socket should bind") else {
            return;
        };
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server_requests = Arc::clone(&requests);
        let server = thread::spawn(move || {
            let bodies = [
                r#"{"type":"sync","status-code":200,"status":"OK","result":[{"name":"pycharm","version":"2026.1.4","confinement":"classic"}]}"#,
                r#"{"type":"error","status-code":404,"status":"Not Found","result":{"message":"snap not found","kind":"snap-not-found","value":"pycharm"}}"#,
            ];
            for (index, body) in bodies.iter().enumerate() {
                let (mut stream, _) = listener.accept().expect("client should connect");
                let mut request = [0u8; 4096];
                let count = stream.read(&mut request).expect("request should be read");
                let request = String::from_utf8_lossy(&request[..count]);
                server_requests
                    .lock()
                    .expect("requests lock")
                    .push(request.lines().next().unwrap_or_default().to_owned());
                let status = if index == 0 {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .expect("response should be written");
            }
        });

        let backend = SnapBackend;
        let service = SnapdRestService {
            backend: &backend,
            client: super::SnapdClient::with_timeout(&socket, std::time::Duration::from_secs(2)),
        };
        let discovered = service
            .search("pycharm")
            .expect("REST discovery should work");
        assert_eq!(discovered.candidates.len(), 1);
        let candidate = discovered
            .candidates
            .into_iter()
            .next()
            .expect("candidate")
            .into_package_candidate(&backend, "pycharm");
        let resolution = service
            .resolve(&candidate)
            .expect("REST 404 without available discovery status should be structured");
        assert_eq!(resolution.availability, SnapAvailability::Stale);
        assert_eq!(resolution.rest_status, Some(404));
        assert!(resolution.fallback_reason.is_none());

        server.join().expect("fake snapd should stop");
        let requests = requests.lock().expect("requests lock");
        assert_eq!(requests[0], "GET /v2/find?q=pycharm&scope=wide HTTP/1.1");
        assert_eq!(requests[1], "GET /v2/find?name=pycharm HTTP/1.1");
        fs::remove_file(socket).expect("fake socket should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn snapd_exact_not_found_is_stale_even_with_available_discovery_metadata() {
        use super::{SnapAvailability, SnapdRestService};
        use std::{
            fs,
            io::{Read, Write},
            sync::{Arc, Mutex},
            thread,
        };

        let socket = std::env::temp_dir().join(format!(
            "allp-snap-service-recovery-{}-{:?}.sock",
            std::process::id(),
            thread::current().id()
        ));
        let _ = fs::remove_file(&socket);
        let Some(listener) = bind_test_listener(&socket, "fake snapd socket should bind") else {
            return;
        };
        let requests = Arc::new(Mutex::new(Vec::new()));
        let server_requests = Arc::clone(&requests);
        let server = thread::spawn(move || {
            let body = r#"{"type":"error","status-code":404,"status":"Not Found","result":{"message":"snap not found","kind":"snap-not-found","value":"pycharm"}}"#;
            let (mut stream, _) = listener.accept().expect("client should connect");
            let mut request = [0u8; 4096];
            let count = stream.read(&mut request).expect("request should be read");
            let request = String::from_utf8_lossy(&request[..count]);
            server_requests
                .lock()
                .expect("requests lock")
                .push(request.lines().next().unwrap_or_default().to_owned());
            write!(
                stream,
                "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .expect("response should be written");
        });

        let backend = SnapBackend;
        let service = SnapdRestService {
            backend: &backend,
            client: super::SnapdClient::with_timeout(&socket, std::time::Duration::from_secs(2)),
        };
        let mut selected = candidate("pycharm");
        selected
            .metadata
            .insert(SNAP_DISCOVERY_STATUS_KEY.to_owned(), "available".to_owned());
        let resolution = service
            .resolve(&selected)
            .expect("exact 404 should be reported as stale");

        assert_eq!(resolution.availability, SnapAvailability::Stale);
        assert_eq!(resolution.rest_status, Some(404));
        assert!(resolution.info.is_none());

        server.join().expect("fake snapd should stop");
        let requests = requests.lock().expect("requests lock");
        assert_eq!(requests[0], "GET /v2/find?name=pycharm HTTP/1.1");
        assert_eq!(requests.len(), 1);
        fs::remove_file(socket).expect("fake socket should be removed");
    }

    #[cfg(unix)]
    fn bind_test_listener(socket: &Path, context: &str) -> Option<UnixListener> {
        match UnixListener::bind(socket) {
            Ok(listener) => Some(listener),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("skipping Unix socket test: {context}: {error}");
                None
            }
            Err(error) => panic!("{context}: {error}"),
        }
    }

    #[test]
    fn snapd_style_service_and_cli_fallback_produce_equivalent_discovery_candidates() {
        struct SearchRunner {
            stdout: String,
        }

        impl ProcessRunner for SearchRunner {
            fn capture(&self, command: &NativeCommand) -> AllpResult<CommandOutput> {
                assert_eq!(
                    command
                        .args
                        .iter()
                        .map(|arg| arg.to_string_lossy().into_owned())
                        .collect::<Vec<_>>(),
                    vec!["find".to_owned(), "pycharm".to_owned()]
                );
                Ok(CommandOutput {
                    success: true,
                    code: Some(0),
                    signal: None,
                    duration: std::time::Duration::from_millis(1),
                    stdout: self.stdout.clone(),
                    stderr: String::new(),
                })
            }

            fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
                unreachable!("discovery should not execute plans")
            }
        }

        struct StaticSnapdService {
            candidates: Vec<SnapCandidate>,
        }

        impl SnapService for StaticSnapdService {
            fn search(&self, _query: &str) -> AllpResult<SnapWideSearch> {
                Ok(SnapWideSearch {
                    candidates: self.candidates.clone(),
                })
            }

            fn resolve(&self, _candidate: &PackageCandidate) -> AllpResult<super::SnapResolution> {
                unreachable!("this test only compares discovery")
            }

            fn install(&self, _request: SnapInstallRequest) -> AllpResult<SnapChangeId> {
                unreachable!("this test only compares discovery")
            }

            fn change(&self, _id: &SnapChangeId) -> AllpResult<SnapChange> {
                unreachable!("this test only compares discovery")
            }
        }

        let stdout = "Name       Version   Publisher   Notes    Summary\npycharm    2026.1.4  jetbrains✓  classic  PyCharm\n";
        let runner = SearchRunner {
            stdout: stdout.to_owned(),
        };
        let cli = SnapCliFallbackService {
            backend: &SnapBackend,
            snap: std::path::Path::new("/usr/bin/snap"),
            runner: &runner,
            fallback_reason: "test fallback".to_owned(),
        };
        let snapd = StaticSnapdService {
            candidates: parse_snap_find(stdout),
        };

        let cli_candidate = cli
            .search("pycharm")
            .expect("cli search should parse")
            .candidates
            .remove(0)
            .into_package_candidate(&SnapBackend, "pycharm");
        let snapd_candidate = snapd
            .search("pycharm")
            .expect("snapd search should parse")
            .candidates
            .remove(0)
            .into_package_candidate(&SnapBackend, "pycharm");

        assert_eq!(cli_candidate.package_id, snapd_candidate.package_id);
        assert_eq!(cli_candidate.version, snapd_candidate.version);
        assert_eq!(cli_candidate.source, snapd_candidate.source);
        assert_eq!(
            cli_candidate.metadata.get(SNAP_AVAILABILITY_KEY),
            snapd_candidate.metadata.get(SNAP_AVAILABILITY_KEY)
        );
        assert_eq!(
            cli_candidate.metadata.get(SNAP_DISCOVERY_NOTES_KEY),
            snapd_candidate.metadata.get(SNAP_DISCOVERY_NOTES_KEY)
        );
    }

    #[test]
    fn spawn_failure_is_classified_as_validation_start_failure() {
        struct FailingRunner;

        impl ProcessRunner for FailingRunner {
            fn capture(
                &self,
                _command: &crate::domain::NativeCommand,
            ) -> AllpResult<CommandOutput> {
                Err(AllpError::Io(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "spawn denied",
                )))
            }

            fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
                unreachable!("Snap validation should not execute install plans")
            }
        }

        let error = validate_snap_candidate(
            &SnapBackend,
            &commands(),
            &FailingRunner,
            &candidate("pycharm"),
        )
        .expect_err("spawn failure should be classified");

        let AllpError::ValidationStartFailed {
            backend,
            executable,
            reason,
        } = error
        else {
            panic!("expected ValidationStartFailed");
        };
        assert_eq!(backend, "Snap");
        assert_eq!(executable, "/usr/bin/snap");
        assert!(reason.contains("spawn denied"));
    }

    #[test]
    fn capture_timeout_is_reported_as_validation_failure() {
        struct TimeoutRunner;

        impl ProcessRunner for TimeoutRunner {
            fn capture(
                &self,
                _command: &crate::domain::NativeCommand,
            ) -> AllpResult<CommandOutput> {
                Err(AllpError::Timeout("native command timed out".to_owned()))
            }

            fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
                unreachable!("Snap validation should not execute install plans")
            }
        }

        let error = validate_snap_candidate(
            &SnapBackend,
            &commands(),
            &TimeoutRunner,
            &candidate("pycharm"),
        )
        .expect_err("timeout should be classified");

        let AllpError::CandidateUnavailable { backend, message } = error else {
            panic!("expected CandidateUnavailable");
        };
        assert_eq!(backend, "Snap");
        assert!(message.contains("snap info pycharm"));
        assert!(message.contains("Install status: BackendError"));
        assert!(message.contains("native command timed out"));
    }

    #[test]
    fn parse_failure_after_success_is_metadata_parse_failure() {
        let error =
            parse_snap_info("this is not snap metadata", "pycharm").expect_err("output is invalid");

        let AllpError::MetadataParseFailed { backend, message } = error else {
            panic!("expected MetadataParseFailed");
        };
        assert_eq!(backend, "Snap");
        assert!(message.contains("Raw stdout:"));
        assert!(message.contains("this is not snap metadata"));
    }

    fn candidate(package_id: &str) -> PackageCandidate {
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert(
            SNAP_TRANSPORT_KEY.to_owned(),
            super::SnapTransport::Cli.as_str().to_owned(),
        );
        metadata.insert(
            SNAP_FALLBACK_REASON_KEY.to_owned(),
            "test CLI fallback".to_owned(),
        );
        PackageCandidate {
            backend_id: "snap".to_owned(),
            backend_name: "Snap".to_owned(),
            category: BackendCategory::Universal,
            domain: PackageDomain::Universal,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: None,
            description: None,
            source: None,
            installers: vec!["Snap".to_owned()],
            artifact_kind: "universal application".to_owned(),
            scope: Some("system".to_owned()),
            match_kind: MatchKind::Exact,
            identity: PackageCandidate::infer_identity(
                MatchKind::Exact,
                PackageDomain::Universal,
                "universal application",
            ),
            metadata,
        }
    }

    fn commands() -> crate::backends::CommandMap {
        let mut commands = crate::backends::CommandMap::new();
        commands.insert("snap".to_owned(), "/usr/bin/snap".into());
        commands
    }
}
