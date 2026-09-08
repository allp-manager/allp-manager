use crate::{
    backends::{
        contract::command_path,
        util::{capture_allowing_no_matches, capture_checked, match_kind, parse_key_value_lines},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpResult, BackendCategory, BackendSearchIssue, BackendSearchIssueKind,
        BackendSearchReport, Capability, DeveloperTarget, ExecutionPlan, InstalledPackage,
        MaintenancePlan, NativeCommand, OperationKind, PackageCandidate, PackageDomain,
        PackageInfo, PrivilegeRequirement,
    },
    execution::ProcessRunner,
};

pub struct PacmanBackend;

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
    key: "pacman",
    alternatives: &["pacman"],
}];

impl Backend for PacmanBackend {
    fn id(&self) -> &'static str {
        "pacman"
    }
    fn display_name(&self) -> &'static str {
        "Pacman"
    }
    fn category(&self) -> BackendCategory {
        BackendCategory::System
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
    ) -> AllpResult<BackendSearchReport> {
        let pacman = command_path(self, commands, "pacman")?;
        let output = capture_allowing_no_matches(
            self,
            runner,
            NativeCommand::new(pacman).args(["-Ss", query]),
            |output| {
                let stderr = output.stderr.trim().to_ascii_lowercase();
                output.code == Some(1)
                    && output.stdout.trim().is_empty()
                    && (stderr.is_empty() || stderr.contains("no packages match"))
            },
        )?;
        Ok(output
            .as_deref()
            .map(|output| parse_pacman_search(self, output, query))
            .unwrap_or_default())
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let pacman = command_path(self, commands, "pacman")?;
        let output = capture_checked(self, runner, NativeCommand::new(pacman).arg("-Q"))?;
        Ok(output
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let package_id = parts.next()?;
                Some(InstalledPackage {
                    backend_id: self.id().to_owned(),
                    backend_name: self.display_name().to_owned(),
                    category: self.category(),
                    domain: PackageDomain::System,
                    package_id: package_id.to_owned(),
                    display_name: package_id.to_owned(),
                    version: parts.next().map(str::to_owned),
                    description: None,
                    source: Some("Pacman local database".to_owned()),
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
        let pacman = command_path(self, commands, "pacman")?;
        let remote = NativeCommand::new(pacman).args(["-Si", package_id]);
        let output = match capture_checked(self, runner, remote) {
            Ok(output) => output,
            Err(_) => capture_checked(
                self,
                runner,
                NativeCommand::new(pacman).args(["-Qi", package_id]),
            )?,
        };
        let fields = parse_key_value_lines(&output);
        Ok(PackageInfo {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::System,
            package_id: fields
                .get("Name")
                .cloned()
                .unwrap_or_else(|| package_id.to_owned()),
            display_name: fields
                .get("Name")
                .cloned()
                .unwrap_or_else(|| package_id.to_owned()),
            version: fields.get("Version").cloned(),
            description: fields.get("Description").cloned(),
            source: fields.get("Repository").cloned(),
            scope: Some("system".to_owned()),
            artifact_kind: Some("system package".to_owned()),
            installed: None,
            extra: fields
                .into_iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "Name" | "Version" | "Description" | "Repository"
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
        let pacman = command_path(self, commands, "pacman")?;
        let remote = NativeCommand::new(pacman).args(["-Si", package_id]);
        match capture_checked(self, runner, remote) {
            Ok(output) => Ok(output),
            Err(_) => capture_checked(
                self,
                runner,
                NativeCommand::new(pacman).args(["-Qi", package_id]),
            ),
        }
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let pacman = command_path(self, commands, "pacman")?;
        Ok(plan(
            self,
            pacman,
            PlanSpec {
                operation: OperationKind::Install,
                action: "Install system package",
                package_id: Some(candidate.package_id.clone()),
                source: candidate.source.clone(),
                scope: candidate.scope.clone(),
                args: ["-S", "--", candidate.package_id.as_str()],
            },
        ))
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let pacman = command_path(self, commands, "pacman")?;
        Ok(plan(
            self,
            pacman,
            PlanSpec {
                operation: OperationKind::Remove,
                action: "Remove system package",
                package_id: Some(package.package_id.clone()),
                source: package.source.clone(),
                scope: package.scope.clone(),
                args: ["-R", "--", package.package_id.as_str()],
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
        let pacman = command_path(self, commands, "pacman")?;
        let mut plan = plan(
            self,
            pacman,
            PlanSpec {
                operation: OperationKind::Update,
                action: "Synchronize package databases",
                package_id: None,
                source: Some("Pacman repositories".to_owned()),
                scope: Some("system".to_owned()),
                args: ["-Sy"],
            },
        );
        plan.details.push((
            "Policy".to_owned(),
            "Pacman -Sy refreshes sync databases only; run a full upgrade before installing packages to avoid partial upgrades".to_owned(),
        ));
        Ok(MaintenancePlan::from_plans(vec![plan]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let pacman = command_path(self, commands, "pacman")?;
        Ok(MaintenancePlan::from_plans(vec![plan(
            self,
            pacman,
            PlanSpec {
                operation: OperationKind::Upgrade,
                action: "Synchronize repositories and upgrade installed packages",
                package_id: None,
                source: Some("Pacman repositories".to_owned()),
                scope: Some("system".to_owned()),
                args: ["-Syu"],
            },
        )]))
    }
}

fn parse_pacman_search(backend: &PacmanBackend, output: &str, query: &str) -> BackendSearchReport {
    let mut lines = output.lines().peekable();
    let mut report = BackendSearchReport::default();
    let mut rejected_lines = 0usize;

    while let Some(header) = lines.next() {
        if header.trim().is_empty() {
            continue;
        }
        if header.starts_with(' ') {
            rejected_lines += 1;
            continue;
        }
        let mut parts = header.split_whitespace();
        let Some(repo_and_name) = parts.next() else {
            rejected_lines += 1;
            continue;
        };
        let Some((repository, package_id)) = repo_and_name.split_once('/') else {
            rejected_lines += 1;
            continue;
        };
        if repository.is_empty() || package_id.is_empty() {
            rejected_lines += 1;
            continue;
        }
        let version = parts.next().map(str::to_owned);
        let description = if lines.peek().is_some_and(|line| line.starts_with(' ')) {
            lines.next().map(|line| line.trim().to_owned())
        } else {
            None
        };

        let candidate_match = match_kind(package_id, query);
        report.candidates.push(PackageCandidate {
            backend_id: backend.id().to_owned(),
            backend_name: backend.display_name().to_owned(),
            category: backend.category(),
            domain: PackageDomain::System,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version,
            description,
            source: Some(repository.to_owned()),
            installers: vec![backend.display_name().to_owned()],
            artifact_kind: "system package".to_owned(),
            scope: Some("system".to_owned()),
            match_kind: candidate_match,
            identity: PackageCandidate::infer_identity(
                candidate_match,
                PackageDomain::System,
                "system package",
            ),
            metadata: Default::default(),
        });
    }

    if rejected_lines > 0 {
        report.issues.push(BackendSearchIssue {
            kind: BackendSearchIssueKind::UnrecognizedOutput,
            stage: Some("pacman -Ss".to_owned()),
            message: format!(
                "Pacman returned {rejected_lines} non-empty line(s) in an unrecognized format"
            ),
        });
    }
    report
}

struct PlanSpec<T> {
    operation: OperationKind,
    action: &'static str,
    package_id: Option<String>,
    source: Option<String>,
    scope: Option<String>,
    args: T,
}

fn plan<const N: usize>(
    backend: &PacmanBackend,
    program: &std::path::Path,
    spec: PlanSpec<[&str; N]>,
) -> ExecutionPlan {
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

#[cfg(test)]
mod search_tests {
    use super::{parse_pacman_search, PacmanBackend};
    use crate::domain::BackendSearchIssueKind;

    #[test]
    fn pacman_search_fixtures_distinguish_matches_no_matches_and_unknown_output() {
        let valid = parse_pacman_search(
            &PacmanBackend,
            include_str!("../../../tests/fixtures/pacman/search-valid.txt"),
            "firefox",
        );
        assert_eq!(valid.candidates.len(), 2);
        assert!(valid.issues.is_empty());

        let empty = parse_pacman_search(
            &PacmanBackend,
            include_str!("../../../tests/fixtures/pacman/search-no-match.txt"),
            "missing",
        );
        assert!(empty.candidates.is_empty());
        assert!(empty.issues.is_empty());

        let unknown = parse_pacman_search(
            &PacmanBackend,
            include_str!("../../../tests/fixtures/pacman/search-unrecognized.txt"),
            "firefox",
        );
        assert!(unknown.candidates.is_empty());
        assert_eq!(
            unknown.issues[0].kind,
            BackendSearchIssueKind::UnrecognizedOutput
        );
    }
}
