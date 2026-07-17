use crate::{
    backends::{
        contract::command_path,
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
    NotInstalled,
    InstalledWithoutRemotes,
    InstalledWithRemotes(Vec<FlatpakRemote>),
    BackendError(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlatpakRemote {
    pub name: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub filter: Option<String>,
    pub options: Option<String>,
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

    fn probe(&self, commands: &CommandMap, runner: &dyn ProcessRunner) -> AllpResult<()> {
        let flatpak = command_path(self, commands, "flatpak")?;
        capture_checked(self, runner, NativeCommand::new(flatpak).arg("--version")).map(|_| ())
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        let flatpak = command_path(self, commands, "flatpak")?;
        let remotes = capture_checked(
            self,
            runner,
            NativeCommand::new(flatpak)
                .args(["remotes", "--columns=name,title,url,filter,options"]),
        )?;
        if parse_flatpak_remotes(&remotes).is_empty() {
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
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let flatpak = command_path(self, commands, "flatpak")?;
        Ok(MaintenancePlan::from_plans(vec![flatpak_plan(
            self,
            flatpak,
            OperationKind::Update,
            "Update installed Flatpak applications and runtimes",
            "User",
            PrivilegeRequirement::OriginalUserRequired,
        )]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let flatpak = command_path(self, commands, "flatpak")?;
        Ok(MaintenancePlan::from_plans(vec![flatpak_plan(
            self,
            flatpak,
            OperationKind::Upgrade,
            "Update installed Flatpak applications and runtimes",
            "User",
            PrivilegeRequirement::OriginalUserRequired,
        )]))
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
    scope: &str,
    privilege: PrivilegeRequirement,
) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation,
        action: action.to_owned(),
        package_id: None,
        source: Some("configured Flatpak remotes".to_owned()),
        scope: Some(scope.to_owned()),
        details: Vec::new(),
        command: NativeCommand::new(program).args(["update", "--user"]),
        privilege,
        requires_root: false,
        interactive: true,
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
    let Some(flatpak) = commands.get("flatpak") else {
        return FlatpakBackendState::NotInstalled;
    };
    let command =
        NativeCommand::new(flatpak).args(["remotes", "--columns=name,title,url,filter,options"]);
    match runner.capture(&command) {
        Ok(output) if output.success => {
            let remotes = parse_flatpak_remotes(&output.stdout);
            if remotes.is_empty() {
                FlatpakBackendState::InstalledWithoutRemotes
            } else {
                FlatpakBackendState::InstalledWithRemotes(remotes)
            }
        }
        Ok(output) => FlatpakBackendState::BackendError(if output.stderr.trim().is_empty() {
            output.stdout.trim().to_owned()
        } else {
            output.stderr.trim().to_owned()
        }),
        Err(error) => FlatpakBackendState::BackendError(error.to_string()),
    }
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
    match detect_flatpak_state(commands, runner) {
        FlatpakBackendState::InstalledWithRemotes(remotes) => Ok(remotes
            .iter()
            .any(|remote| remote.name.eq_ignore_ascii_case(FLATHUB_NAME))),
        FlatpakBackendState::InstalledWithoutRemotes | FlatpakBackendState::NotInstalled => {
            Ok(false)
        }
        FlatpakBackendState::BackendError(message) => Err(AllpError::CommandFailed {
            backend: "Flatpak".to_owned(),
            command: "flatpak remotes --columns=name,title,url,filter,options".to_owned(),
            code: None,
            stderr: message,
        }),
    }
}

fn parse_flatpak_remotes(output: &str) -> Vec<FlatpakRemote> {
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
