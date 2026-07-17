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
};

pub struct FlatpakBackend;

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
            NativeCommand::new(flatpak).args(["remotes", "--columns=name,url"]),
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
        let mut command = NativeCommand::new(flatpak).args(["install", "--user"]);
        if let Some(source) = candidate
            .source
            .as_deref()
            .filter(|source| !source.contains(' ') && !source.is_empty())
        {
            command = command.arg(source);
        }
        command = command.arg(candidate.package_id.as_str());
        Ok(ExecutionPlan {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            operation: OperationKind::Install,
            action: "Install Flatpak application".to_owned(),
            package_id: Some(candidate.package_id.clone()),
            source: candidate.source.clone(),
            scope: Some("User".to_owned()),
            details: Vec::new(),
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

fn parse_flatpak_remotes(output: &str) -> Vec<(String, String)> {
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
            Some((
                columns[0].clone(),
                columns.get(1).cloned().unwrap_or_default(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_flatpak_remotes, parse_flatpak_update_status};
    use crate::domain::OperationStatus;

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
        let remotes = parse_flatpak_remotes("Name\tURL\nflathub\thttps://flathub.org/repo/\n");

        assert_eq!(
            remotes,
            vec![("flathub".to_owned(), "https://flathub.org/repo/".to_owned())]
        );
    }
}
