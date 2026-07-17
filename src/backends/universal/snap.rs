use crate::execution::render_native_command;
use crate::{
    backends::{
        contract::{
            command_path, InstallPreflight, InstallPreflightRecovery, InstallPreflightStatus,
            InstallPreflightWarning,
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
};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::Path,
    time::Duration,
};

pub struct SnapBackend;

const CAPABILITIES: &[Capability] = &[
    Capability::Search,
    Capability::Install,
    Capability::Remove,
    Capability::Update,
    Capability::Upgrade,
    Capability::List,
    Capability::Info,
];
const REQUIREMENTS: &[CommandRequirement] = &[CommandRequirement {
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

    fn probe(&self, commands: &CommandMap, runner: &dyn ProcessRunner) -> AllpResult<()> {
        let snap = command_path(self, commands, "snap")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(snap)
                .arg("version")
                .timeout(Duration::from_secs(2)),
        )
        .map(|_| ())
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        let snap = command_path(self, commands, "snap")?;
        let output = capture_checked(self, runner, NativeCommand::new(snap).args(["find", query]))?;
        let mut candidates = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("Name ") || line.starts_with("No matching") {
                continue;
            }
            let columns: Vec<&str> = line.split_whitespace().collect();
            if columns.len() < 4 {
                continue;
            }
            let package_id = columns[0];
            let version = columns.get(1).map(|value| (*value).to_owned());
            let publisher = columns.get(2).map(|value| normalize_snap_publisher(value));
            let mut metadata = BTreeMap::new();
            let source = publisher.as_ref().map(|publisher| {
                metadata.insert(
                    "snap.publisher_name".to_owned(),
                    publisher.name.clone().unwrap_or_default(),
                );
                metadata.insert(
                    "snap.publisher_verification".to_owned(),
                    publisher.verification.as_str().to_owned(),
                );
                publisher.human_label()
            });
            let description = (columns.len() > 4).then(|| columns[4..].join(" "));

            let candidate_match = match_kind(package_id, query);
            candidates.push(PackageCandidate {
                backend_id: self.id().to_owned(),
                backend_name: self.display_name().to_owned(),
                category: self.category(),
                domain: PackageDomain::Universal,
                package_id: package_id.to_owned(),
                display_name: package_id.to_owned(),
                version,
                description,
                source,
                installers: vec![self.display_name().to_owned()],
                artifact_kind: "universal application".to_owned(),
                scope: Some("system".to_owned()),
                match_kind: candidate_match,
                identity: PackageCandidate::infer_identity(
                    candidate_match,
                    PackageDomain::Universal,
                    "universal application",
                ),
                metadata,
            });
        }

        Ok(candidates)
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
        let snap = command_path(self, commands, "snap")?;
        let info = validate_snap_candidate(self, snap, runner, candidate)?;
        validate_snap_architecture(&info)?;
        let selected_channel = select_snap_install_channel(&info)?;

        if let Some(installed) = inspect_snap_installed(snap, runner, &info.name)? {
            let candidate_version = candidate_version_label(&info, selected_channel.as_ref());
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
            candidate: Box::new(resolve_snap_candidate(candidate, info, selected_channel)),
            warnings,
        })
    }

    fn install_preflight_status(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<Option<InstallPreflightStatus>> {
        let snap = command_path(self, commands, "snap")?;
        Ok(Some(InstallPreflightStatus {
            stage: "Validating Snap package...".to_owned(),
            command: snap_info_command(snap, candidate)?,
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
                | AllpError::ValidationStartFailed { .. }
                | AllpError::MetadataParseFailed { .. }
        ) {
            return Err(error);
        }
        if no_interactive {
            return Err(error);
        }

        let snap = command_path(self, commands, "snap")?;
        let diagnostics_command = snap_info_command(snap, candidate)?;
        println!("✖ {error}");

        loop {
            println!();
            println!("[1] Search again");
            println!("[2] Show Snap diagnostics");
            println!("[0] Cancel");
            print!("Choose an action [1-2, 0 to cancel]: ");
            io::stdout().flush()?;

            let input =
                read_snap_action_line("input closed before a Snap validation action was selected")?;

            match input.as_str() {
                "1" => return Ok(InstallPreflightRecovery::RetrySearch),
                "2" => {
                    print_snap_diagnostics(self, snap, runner, &diagnostics_command);
                    loop {
                        println!();
                        println!("[1] Retry validation");
                        println!("[2] Search again");
                        println!("[0] Cancel");
                        print!("Choose an action [1-2, 0 to cancel]: ");
                        io::stdout().flush()?;
                        match read_snap_action_line(
                            "input closed before a Snap validation action was selected",
                        )?
                        .as_str()
                        {
                            "1" => return Ok(InstallPreflightRecovery::RetryValidation),
                            "2" => return Ok(InstallPreflightRecovery::RetrySearch),
                            "0" => return Ok(InstallPreflightRecovery::Cancelled),
                            _ => eprintln!("Please enter 1, 2, or 0."),
                        }
                    }
                }
                "0" => return Ok(InstallPreflightRecovery::Cancelled),
                _ => eprintln!("Please enter 1, 2, or 0."),
            }
        }
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
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
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let snap = command_path(self, commands, "snap")?;
        Ok(MaintenancePlan::from_plans(vec![snap_plan(
            self,
            snap,
            PlanSpec {
                operation: OperationKind::Update,
                action: "Refresh installed snaps",
                package_id: None,
                source: Some("Snap Store".to_owned()),
                scope: Some("system".to_owned()),
                args: vec!["refresh".into()],
            },
        )]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let snap = command_path(self, commands, "snap")?;
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

fn validate_snap_candidate(
    backend: &SnapBackend,
    snap: &Path,
    runner: &dyn ProcessRunner,
    candidate: &PackageCandidate,
) -> AllpResult<SnapInfo> {
    let package_id = normalize_snap_package_id(&candidate.package_id)?;
    let command = snap_info_command(snap, candidate)?.timeout(Duration::from_secs(10));
    let output = match runner.capture(&command) {
        Ok(output) => output,
        Err(AllpError::Io(error)) => {
            return Err(snap_validation_start_error(
                backend,
                snap,
                AllpError::Io(error),
            ));
        }
        Err(error) => return Err(snap_validation_capture_error(backend, &command, error)),
    };
    if !output.success {
        return Err(snap_validation_failed_error(backend, &command, &output));
    }
    let mut info = parse_snap_info(&output.stdout, &package_id)?;
    if !output.stderr.trim().is_empty() {
        info.validation_warning = Some(output.stderr.trim().to_owned());
    }
    Ok(info)
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

fn validate_snap_architecture(info: &SnapInfo) -> AllpResult<()> {
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

    Err(AllpError::ValidationFailed {
        backend: "Snap".to_owned(),
        message: format!(
            "Snap \"{}\" is not available for architecture {}. Available architectures: {}",
            info.name,
            current,
            info.architectures.join(", ")
        ),
    })
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

fn select_snap_install_channel(info: &SnapInfo) -> AllpResult<Option<SnapChannel>> {
    if info.channels.is_empty() {
        return Ok(None);
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
    info: SnapInfo,
    selected_channel: Option<SnapChannel>,
) -> PackageCandidate {
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
}

fn snap_candidate_metadata(
    info: &SnapInfo,
    selected_channel: Option<&SnapChannel>,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
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

fn snap_validation_capture_error(
    backend: &SnapBackend,
    command: &NativeCommand,
    error: AllpError,
) -> AllpError {
    AllpError::ValidationFailed {
        backend: backend.display_name().to_owned(),
        message: format!(
            "  Command: {}\n  Exit code: unavailable\n  Error: {error}",
            render_native_command(command)
        ),
    }
}

fn snap_validation_failed_error(
    backend: &SnapBackend,
    command: &NativeCommand,
    output: &CommandOutput,
) -> AllpError {
    let mut message = format!(
        "  Command: {}\n  Exit code: {}\n",
        render_native_command(command),
        output
            .code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unavailable".to_owned())
    );
    if let Some(signal) = output.signal {
        message.push_str(&format!("  Signal: {signal}\n"));
    }
    message.push_str(&format!("  Duration: {}ms\n", output.duration.as_millis()));
    message.push_str(&format!("  Error: {}", snap_error_text(output)));

    AllpError::ValidationFailed {
        backend: backend.display_name().to_owned(),
        message,
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
    snap: &Path,
    runner: &dyn ProcessRunner,
    command: &NativeCommand,
) {
    println!("\nSnap diagnostics\n");
    println!("Executable:");
    println!("  {}", snap.to_string_lossy());
    println!();
    println!("Command:");
    println!("  {}", render_native_command(command));

    match runner.capture(command) {
        Ok(output) => {
            println!();
            println!("Exit code:");
            println!(
                "  {}",
                output
                    .code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unavailable".to_owned())
            );
            if let Some(signal) = output.signal {
                println!();
                println!("Signal:");
                println!("  {signal}");
            }
            println!();
            println!("stdout:");
            println!("  {}", indent_block(&bounded_block(&output.stdout)));
            println!();
            println!("stderr:");
            println!("  {}", indent_block(&bounded_block(&output.stderr)));
        }
        Err(error) => {
            println!();
            println!("Exit code:");
            println!("  <not started>");
            println!();
            println!("stdout:");
            println!("  <empty>");
            println!();
            println!("stderr:");
            println!("  Unable to start Snap diagnostics: {error}");
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
    if lower.contains("cannot communicate with server")
        || lower.contains("snapd")
        || lower.contains("daemon")
    {
        return Some(SnapErrorKind::DaemonUnavailable);
    }
    if lower.contains("store") && (lower.contains("unavailable") || lower.contains("timeout")) {
        return Some(SnapErrorKind::StoreUnavailable);
    }
    if lower.contains("not found")
        || lower.contains("no snap found")
        || lower.contains("snap") && lower.contains("not installed") && lower.contains("find")
    {
        return Some(SnapErrorKind::PackageNotFound);
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
        classify_snap_error, normalize_snap_package_id, parse_snap_info, parse_snap_list_entry,
        parse_snap_refresh_status, select_snap_install_channel, snap_info_command,
        validate_snap_candidate, SnapBackend, SnapConfinement, SnapErrorKind,
        SnapPublisherVerification,
    };
    use crate::domain::{
        AllpError, AllpResult, BackendCategory, ExecutionPlan, MatchKind, OperationStatus,
        PackageCandidate, PackageDomain,
    };
    use crate::execution::{CommandOutput, ProcessRunner, ProcessStatus};
    use std::io;

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
        let channel = select_snap_install_channel(&info)
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

        let error = select_snap_install_channel(&info).expect_err("edge-only should be blocked");
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
            std::path::Path::new("/usr/bin/snap"),
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
            std::path::Path::new("/usr/bin/snap"),
            &TimeoutRunner,
            &candidate("pycharm"),
        )
        .expect_err("timeout should be classified");

        let AllpError::ValidationFailed { backend, message } = error else {
            panic!("expected ValidationFailed");
        };
        assert_eq!(backend, "Snap");
        assert!(message.contains("snap info pycharm"));
        assert!(message.contains("Exit code: unavailable"));
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
            metadata: Default::default(),
        }
    }
}
