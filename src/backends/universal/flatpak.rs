use crate::{
    backends::{
        contract::{command_path, BackendOperationCapability},
        util::{capture_checked, match_kind, parse_key_value_lines, split_tab_or_whitespace},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, BackendOperationRecord, Capability,
        DeveloperTarget, ExecutionPlan, InstalledPackage, MaintenancePlan, NativeCommand,
        OperationKind, OperationStatus, PackageCandidate, PackageDomain, PackageInfo,
        PrivilegeRequirement,
    },
    execution::{ProcessRunner, ProcessStatus},
    platform::PlatformContext,
    requirements::{flatpak_requirements, BackendRequirements, RequirementSet},
};
use serde::Serialize;

pub struct FlatpakBackend;

pub const FLATHUB_NAME: &str = "flathub";
pub const FLATHUB_URL: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlatpakBackendState {
    Missing,
    InstalledNoRemotes,
    InstalledUserScopeReady,
    InstalledSystemScopeReady,
    InstalledBothScopesReady,
    InstalledRefsWithoutUsableRemote,
    BackendError(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlatpakRemote {
    pub name: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub filter: Option<String>,
    pub options: Option<String>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatpakProbe {
    pub state: FlatpakBackendState,
    pub remotes: Vec<FlatpakRemote>,
    pub reason: Option<String>,
}

impl BackendRequirements for FlatpakBackend {
    fn requirements(&self, context: &PlatformContext) -> RequirementSet {
        flatpak_requirements(context)
    }
}

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
    key: "flatpak",
    alternatives: &["flatpak"],
}];

impl Backend for FlatpakBackend {
    fn id(&self) -> &'static str {
        "flatpak"
    }
    fn display_name(&self) -> &'static str {
        "Flatpak"
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
            return "Not applicable during metadata-only update; installed Flatpak updates are handled by `allp upgrade`".to_owned();
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
        let flatpak = command_path(self, commands, "flatpak")?;
        capture_checked(self, runner, NativeCommand::new(flatpak).arg("--version"))?;
        match detect_flatpak_probe(commands, runner).state {
            FlatpakBackendState::Missing => Err(AllpError::BackendNotDetected(
                "Flatpak executable was not found".to_owned(),
            )),
            FlatpakBackendState::InstalledNoRemotes
            | FlatpakBackendState::InstalledRefsWithoutUsableRemote => {
                Err(AllpError::NoConfiguredRemotes {
                    backend: self.display_name().to_owned(),
                })
            }
            FlatpakBackendState::BackendError(message) => Err(AllpError::CommandFailed {
                backend: self.display_name().to_owned(),
                command: "flatpak remotes --user/--system".to_owned(),
                code: None,
                stderr: message,
            }),
            FlatpakBackendState::InstalledUserScopeReady
            | FlatpakBackendState::InstalledSystemScopeReady
            | FlatpakBackendState::InstalledBothScopesReady => Ok(()),
        }
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        let flatpak = command_path(self, commands, "flatpak")?;
        let probe = detect_flatpak_probe(commands, runner);
        if !matches!(
            probe.state,
            FlatpakBackendState::InstalledUserScopeReady
                | FlatpakBackendState::InstalledSystemScopeReady
                | FlatpakBackendState::InstalledBothScopesReady
        ) {
            return Err(AllpError::NoConfiguredRemotes {
                backend: self.display_name().to_owned(),
            });
        }

        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(flatpak).args([
                "search",
                query,
                "--columns=application,name,description,version,branch,remotes",
            ]),
        )?;

        let mut candidates = Vec::new();
        for line in output.lines() {
            let columns = split_tab_or_whitespace(line);
            if columns.len() < 2 || columns[0].eq_ignore_ascii_case("Application") {
                continue;
            }

            let package_id = columns[0].clone();
            let display_name = columns
                .get(1)
                .cloned()
                .unwrap_or_else(|| package_id.clone());
            let remote = columns.get(5).cloned().filter(|value| !value.is_empty());
            let mut metadata = std::collections::BTreeMap::new();
            if let Some(remote) = &remote {
                metadata.insert("flatpak.remote".to_owned(), remote.clone());
            }
            if let Some(branch) = columns.get(4).filter(|value| !value.is_empty()) {
                metadata.insert("flatpak.branch".to_owned(), branch.clone());
            }
            let candidate_match = if package_id.eq_ignore_ascii_case(query)
                || display_name.eq_ignore_ascii_case(query)
            {
                crate::domain::MatchKind::Exact
            } else {
                match_kind(&package_id, query)
            };
            candidates.push(PackageCandidate {
                backend_id: self.id().to_owned(),
                backend_name: self.display_name().to_owned(),
                category: self.category(),
                domain: PackageDomain::Universal,
                package_id: package_id.clone(),
                display_name: display_name.clone(),
                description: columns.get(2).cloned().filter(|value| !value.is_empty()),
                version: columns.get(3).cloned().filter(|value| !value.is_empty()),
                source: remote.or_else(|| Some("configured Flatpak remotes".to_owned())),
                installers: vec![self.display_name().to_owned()],
                artifact_kind: "universal application".to_owned(),
                scope: None,
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

    fn plan_search_prerequisite(&self, commands: &CommandMap) -> AllpResult<Option<ExecutionPlan>> {
        plan_add_flathub(commands).map(Some)
    }

    fn verify_search_prerequisite(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<bool> {
        flathub_is_configured(commands, runner)
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let flatpak = command_path(self, commands, "flatpak")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(flatpak).args([
                "list",
                "--app",
                "--columns=application,name,version,branch,origin,installation",
            ]),
        )?;

        Ok(output
            .lines()
            .filter_map(|line| {
                let columns = split_tab_or_whitespace(line);
                if columns.len() < 2 || columns[0].eq_ignore_ascii_case("Application") {
                    return None;
                }
                Some(InstalledPackage {
                    backend_id: self.id().to_owned(),
                    backend_name: self.display_name().to_owned(),
                    category: self.category(),
                    domain: PackageDomain::Universal,
                    package_id: columns[0].clone(),
                    display_name: columns
                        .get(1)
                        .cloned()
                        .unwrap_or_else(|| columns[0].clone()),
                    version: columns.get(2).cloned().filter(|value| !value.is_empty()),
                    description: columns
                        .get(3)
                        .cloned()
                        .filter(|value| !value.is_empty())
                        .map(|branch| format!("branch: {branch}")),
                    source: columns.get(4).cloned().filter(|value| !value.is_empty()),
                    scope: columns.get(5).cloned().filter(|value| !value.is_empty()),
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
        let flatpak = command_path(self, commands, "flatpak")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(flatpak).args(["info", package_id]),
        )?;
        let fields = parse_key_value_lines(&output);
        Ok(PackageInfo {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::Universal,
            package_id: fields
                .get("Ref")
                .cloned()
                .unwrap_or_else(|| package_id.to_owned()),
            display_name: fields
                .get("Name")
                .cloned()
                .unwrap_or_else(|| package_id.to_owned()),
            version: fields.get("Version").cloned(),
            description: fields.get("Description").cloned(),
            source: fields.get("Origin").cloned(),
            scope: fields.get("Installation").cloned(),
            artifact_kind: Some("universal application".to_owned()),
            installed: None,
            extra: fields
                .into_iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "Ref" | "Name" | "Version" | "Description" | "Origin" | "Installation"
                    )
                })
                .collect(),
        })
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        let flatpak = command_path(self, commands, "flatpak")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(flatpak).args(["info", package_id]),
        )
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let flatpak = command_path(self, commands, "flatpak")?;
        let remote = candidate
            .metadata
            .get("flatpak.remote")
            .or(candidate.source.as_ref())
            .filter(|remote| !remote.contains(char::is_whitespace) && !remote.is_empty())
            .ok_or_else(|| {
                AllpError::InvalidInput(
                    "Flatpak installation requires the exact source remote from search results"
                        .to_owned(),
                )
            })?;
        let mut command = NativeCommand::new(flatpak).args(["install", "--user", remote]);
        command = command.arg(candidate.package_id.as_str());
        Ok(ExecutionPlan {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            operation: OperationKind::Install,
            action: "Install Flatpak application".to_owned(),
            package_id: Some(candidate.package_id.clone()),
            source: candidate.source.clone(),
            scope: Some("User".to_owned()),
            details: candidate
                .metadata
                .get("flatpak.branch")
                .map(|branch| vec![("Branch".to_owned(), branch.clone())])
                .unwrap_or_default(),
            command,
            privilege: PrivilegeRequirement::OriginalUserRequired,
            requires_root: false,
            interactive: true,
        })
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let flatpak = command_path(self, commands, "flatpak")?;
        let mut command = NativeCommand::new(flatpak).arg("uninstall");
        if let Some(scope) = package.scope.as_deref() {
            if scope.eq_ignore_ascii_case("user") {
                command = command.arg("--user");
            } else if scope.eq_ignore_ascii_case("system") {
                command = command.arg("--system");
            }
        }
        command = command.arg(package.package_id.as_str());
        let privilege = if package
            .scope
            .as_deref()
            .is_some_and(|scope| scope.eq_ignore_ascii_case("system"))
        {
            PrivilegeRequirement::RootRequired
        } else {
            PrivilegeRequirement::OriginalUserRequired
        };
        Ok(ExecutionPlan {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            operation: OperationKind::Remove,
            action: "Remove Flatpak application".to_owned(),
            package_id: Some(package.package_id.clone()),
            source: package.source.clone(),
            scope: package.scope.clone(),
            details: Vec::new(),
            command,
            privilege,
            requires_root: privilege == PrivilegeRequirement::RootRequired,
            interactive: true,
        })
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
                "Not applicable during metadata-only update; installed Flatpak updates are handled by `allp upgrade`",
            )],
        })
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let flatpak = command_path(self, commands, "flatpak")?;
        let probe = detect_flatpak_probe(commands, runner);
        let mut plans = Vec::new();
        if matches!(
            probe.state,
            FlatpakBackendState::InstalledUserScopeReady
                | FlatpakBackendState::InstalledBothScopesReady
        ) {
            plans.push(flatpak_plan(
                self,
                flatpak,
                OperationKind::Upgrade,
                "Update user Flatpak applications and runtimes",
                FlatpakScope::User,
            ));
        }
        if matches!(
            probe.state,
            FlatpakBackendState::InstalledSystemScopeReady
                | FlatpakBackendState::InstalledBothScopesReady
        ) {
            plans.push(flatpak_plan(
                self,
                flatpak,
                OperationKind::Upgrade,
                "Update system Flatpak applications and runtimes",
                FlatpakScope::System,
            ));
        }
        if plans.is_empty() {
            let message = match probe.state {
                FlatpakBackendState::InstalledRefsWithoutUsableRemote => {
                    "installed Flatpak refs exist, but no user or system remote is configured"
                }
                FlatpakBackendState::InstalledNoRemotes | FlatpakBackendState::Missing => {
                    "no user or system Flatpak remotes are configured"
                }
                FlatpakBackendState::BackendError(_) => {
                    return Err(AllpError::CommandFailed {
                        backend: self.display_name().to_owned(),
                        command: "flatpak remotes --user/--system".to_owned(),
                        code: None,
                        stderr: probe
                            .reason
                            .unwrap_or_else(|| "Flatpak backend probe failed".to_owned()),
                    });
                }
                FlatpakBackendState::InstalledUserScopeReady
                | FlatpakBackendState::InstalledSystemScopeReady
                | FlatpakBackendState::InstalledBothScopesReady => {
                    "no Flatpak update scope selected"
                }
            };
            return Ok(MaintenancePlan {
                plans,
                records: vec![MaintenancePlan::record(
                    self.id(),
                    self.display_name(),
                    OperationStatus::NotApplicable,
                    message,
                )],
            });
        }
        Ok(MaintenancePlan::from_plans(plans))
    }

    fn classify_execution_success(
        &self,
        plan: &ExecutionPlan,
        status: &ProcessStatus,
        _command: &str,
    ) -> Option<Vec<BackendOperationRecord>> {
        parse_flatpak_update_status(&status.stdout, &status.stderr).map(
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

fn flatpak_plan(
    backend: &FlatpakBackend,
    program: &std::path::Path,
    operation: OperationKind,
    action: &str,
    scope: FlatpakScope,
) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation,
        action: action.to_owned(),
        package_id: None,
        source: Some("configured Flatpak remotes".to_owned()),
        scope: Some(scope.label().to_owned()),
        details: Vec::new(),
        command: NativeCommand::new(program).args(["update", scope.cli_arg()]),
        privilege: scope.privilege(),
        requires_root: scope.privilege() == PrivilegeRequirement::RootRequired,
        interactive: true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlatpakScope {
    User,
    System,
}

impl FlatpakScope {
    fn cli_arg(self) -> &'static str {
        match self {
            Self::User => "--user",
            Self::System => "--system",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::System => "System",
        }
    }

    fn storage_label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
        }
    }

    fn privilege(self) -> PrivilegeRequirement {
        match self {
            Self::User => PrivilegeRequirement::OriginalUserRequired,
            Self::System => PrivilegeRequirement::RootRequired,
        }
    }
}

fn parse_flatpak_update_status(stdout: &str, stderr: &str) -> Option<(OperationStatus, String)> {
    let output = format!("{stdout}\n{stderr}");
    let lower = output.to_ascii_lowercase();
    if lower.contains("nothing to do") {
        return Some((OperationStatus::UpToDate, "nothing to do".to_owned()));
    }
    if lower.lines().any(|line| {
        let line = line.trim();
        line.starts_with("installing")
            || line.starts_with("updating")
            || line.contains(" updates complete")
            || line.contains("updated")
    }) {
        return Some((
            OperationStatus::Updated,
            "flatpak update completed".to_owned(),
        ));
    }
    None
}

pub fn detect_flatpak_state(
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
) -> FlatpakBackendState {
    detect_flatpak_probe(commands, runner).state
}

pub fn detect_flatpak_probe(commands: &CommandMap, runner: &dyn ProcessRunner) -> FlatpakProbe {
    let Some(flatpak) = commands.get("flatpak") else {
        return FlatpakProbe {
            state: FlatpakBackendState::Missing,
            remotes: Vec::new(),
            reason: Some("executable not found".to_owned()),
        };
    };

    let user = capture_flatpak_remotes(flatpak, runner, FlatpakScope::User);
    let system = capture_flatpak_remotes(flatpak, runner, FlatpakScope::System);
    let mut remotes = Vec::new();
    if let Ok(user_remotes) = &user {
        remotes.extend(user_remotes.clone());
    }
    if let Ok(system_remotes) = &system {
        remotes.extend(system_remotes.clone());
    }
    let user_ready = user.as_ref().is_ok_and(|remotes| !remotes.is_empty());
    let system_ready = system.as_ref().is_ok_and(|remotes| !remotes.is_empty());

    let state = match (user_ready, system_ready) {
        (true, true) => FlatpakBackendState::InstalledBothScopesReady,
        (true, false) => FlatpakBackendState::InstalledUserScopeReady,
        (false, true) => FlatpakBackendState::InstalledSystemScopeReady,
        (false, false) => {
            if let Err(reason) = user.as_ref().and(system.as_ref()).map(|_| ()) {
                return FlatpakProbe {
                    state: FlatpakBackendState::BackendError(reason.clone()),
                    remotes,
                    reason: Some(reason.clone()),
                };
            }
            if installed_refs_exist_without_usable_remote(flatpak, runner) {
                FlatpakBackendState::InstalledRefsWithoutUsableRemote
            } else {
                FlatpakBackendState::InstalledNoRemotes
            }
        }
    };

    FlatpakProbe {
        state,
        remotes,
        reason: None,
    }
}

fn capture_flatpak_remotes(
    flatpak: &std::path::Path,
    runner: &dyn ProcessRunner,
    scope: FlatpakScope,
) -> Result<Vec<FlatpakRemote>, String> {
    let command = NativeCommand::new(flatpak).args([
        "remotes",
        scope.cli_arg(),
        "--columns=name,title,url,filter,options",
    ]);
    match runner.capture(&command) {
        Ok(output) if output.success => Ok(parse_flatpak_remotes_for_scope(&output.stdout, scope)),
        Ok(output) => Err(if output.stderr.trim().is_empty() {
            output.stdout.trim().to_owned()
        } else {
            output.stderr.trim().to_owned()
        }),
        Err(error) => Err(error.to_string()),
    }
}

fn installed_refs_exist_without_usable_remote(
    flatpak: &std::path::Path,
    runner: &dyn ProcessRunner,
) -> bool {
    let command =
        NativeCommand::new(flatpak).args(["list", "--columns=application,origin,installation"]);
    runner
        .capture(&command)
        .ok()
        .filter(|output| output.success)
        .map(|output| {
            output.stdout.lines().any(|line| {
                let columns = split_tab_or_whitespace(line);
                columns.first().is_some_and(|value| {
                    !value.is_empty() && !value.eq_ignore_ascii_case("Application")
                })
            })
        })
        .unwrap_or(false)
}

pub fn plan_add_flathub(commands: &CommandMap) -> AllpResult<ExecutionPlan> {
    let backend = FlatpakBackend;
    let flatpak = command_path(&backend, commands, "flatpak")?;
    Ok(ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation: OperationKind::Bootstrap,
        action: "Add the Flathub Flatpak remote".to_owned(),
        package_id: Some(FLATHUB_NAME.to_owned()),
        source: Some(FLATHUB_URL.to_owned()),
        scope: Some("User".to_owned()),
        details: vec![
            ("Mutation".to_owned(), "Add remote".to_owned()),
            ("Remote".to_owned(), FLATHUB_NAME.to_owned()),
            ("URL".to_owned(), FLATHUB_URL.to_owned()),
        ],
        command: NativeCommand::new(flatpak).args([
            "remote-add",
            "--user",
            "--if-not-exists",
            FLATHUB_NAME,
            FLATHUB_URL,
        ]),
        privilege: PrivilegeRequirement::OriginalUserRequired,
        requires_root: false,
        interactive: false,
    })
}

pub fn flathub_is_configured(
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
) -> AllpResult<bool> {
    let probe = detect_flatpak_probe(commands, runner);
    match probe.state {
        FlatpakBackendState::InstalledUserScopeReady
        | FlatpakBackendState::InstalledSystemScopeReady
        | FlatpakBackendState::InstalledBothScopesReady => Ok(probe
            .remotes
            .iter()
            .any(|remote| remote.name.eq_ignore_ascii_case(FLATHUB_NAME))),
        FlatpakBackendState::InstalledNoRemotes
        | FlatpakBackendState::InstalledRefsWithoutUsableRemote
        | FlatpakBackendState::Missing => Ok(false),
        FlatpakBackendState::BackendError(message) => Err(AllpError::CommandFailed {
            backend: "Flatpak".to_owned(),
            command: "flatpak remotes --user/--system --columns=name,title,url,filter,options"
                .to_owned(),
            code: None,
            stderr: message,
        }),
    }
}

#[cfg(test)]
fn parse_flatpak_remotes(output: &str) -> Vec<FlatpakRemote> {
    parse_flatpak_remotes_with_scope(output, None)
}

fn parse_flatpak_remotes_for_scope(output: &str, scope: FlatpakScope) -> Vec<FlatpakRemote> {
    parse_flatpak_remotes_with_scope(output, Some(scope.storage_label()))
}

fn parse_flatpak_remotes_with_scope(output: &str, scope: Option<&str>) -> Vec<FlatpakRemote> {
    output
        .lines()
        .filter_map(|line| {
            let columns = split_tab_or_whitespace(line);
            if columns.is_empty()
                || columns[0].eq_ignore_ascii_case("Name")
                || columns[0].trim().is_empty()
            {
                return None;
            }
            Some(FlatpakRemote {
                name: columns[0].clone(),
                title: columns.get(1).cloned().filter(|value| !value.is_empty()),
                url: columns.get(2).cloned().filter(|value| !value.is_empty()),
                filter: columns.get(3).cloned().filter(|value| !value.is_empty()),
                options: columns.get(4).cloned().filter(|value| !value.is_empty()),
                scope: scope.map(str::to_owned),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_flatpak_remotes, parse_flatpak_update_status, FlatpakBackend};
    use crate::domain::OperationStatus;
    use crate::{
        backends::{Backend, CommandMap},
        domain::{BackendCategory, MatchKind, PackageCandidate, PackageDomain},
    };
    use std::{collections::BTreeMap, path::PathBuf};

    #[test]
    fn nothing_to_do_maps_to_up_to_date() {
        let (status, message) =
            parse_flatpak_update_status("Looking for updates...\nNothing to do.\n", "")
                .expect("flatpak output parses");

        assert!(matches!(status, OperationStatus::UpToDate));
        assert_eq!(message, "nothing to do");
    }

    #[test]
    fn parses_configured_remotes() {
        let remotes = parse_flatpak_remotes(
            "Name\tTitle\tURL\tFilter\tOptions\nflathub\tFlathub\thttps://flathub.org/repo/\t\tuser\n",
        );

        assert_eq!(remotes[0].name, "flathub");
        assert_eq!(remotes[0].title.as_deref(), Some("Flathub"));
        assert_eq!(remotes[0].url.as_deref(), Some("https://flathub.org/repo/"));
    }

    #[test]
    fn install_plan_uses_exact_remote_and_application_id() {
        let mut commands = CommandMap::new();
        commands.insert("flatpak".to_owned(), PathBuf::from("/usr/bin/flatpak"));
        let mut metadata = BTreeMap::new();
        metadata.insert("flatpak.remote".to_owned(), "flathub".to_owned());
        let candidate = PackageCandidate {
            backend_id: "flatpak".to_owned(),
            backend_name: "Flatpak".to_owned(),
            category: BackendCategory::Universal,
            domain: PackageDomain::Universal,
            package_id: "org.telegram.desktop".to_owned(),
            display_name: "Telegram Desktop".to_owned(),
            description: None,
            version: None,
            source: Some("flathub".to_owned()),
            installers: vec!["Flatpak".to_owned()],
            artifact_kind: "universal application".to_owned(),
            scope: None,
            match_kind: MatchKind::Exact,
            identity: PackageCandidate::infer_identity(
                MatchKind::Exact,
                PackageDomain::Universal,
                "universal application",
            ),
            metadata,
        };

        let plan = FlatpakBackend
            .plan_install(&commands, &candidate)
            .expect("exact Flatpak result should produce a plan");

        assert_eq!(plan.command.program, PathBuf::from("/usr/bin/flatpak"));
        assert_eq!(
            plan.command
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["install", "--user", "flathub", "org.telegram.desktop"]
        );
    }
}
