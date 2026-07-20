use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked, match_kind, parse_key_value_lines},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpResult, BackendCategory, Capability, DeveloperTarget, ExecutionPlan, InstalledPackage,
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
    ) -> AllpResult<Vec<PackageCandidate>> {
        let pacman = command_path(self, commands, "pacman")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(pacman).args(["-Ss", query]),
        )?;
        let mut lines = output.lines().peekable();
        let mut candidates = Vec::new();

        while let Some(header) = lines.next() {
            if header.starts_with(' ') || !header.contains('/') {
                continue;
            }
            let mut parts = header.split_whitespace();
            let Some(repo_and_name) = parts.next() else {
                continue;
            };
            let version = parts.next().map(str::to_owned);
            let Some((repository, package_id)) = repo_and_name.split_once('/') else {
                continue;
            };
            let has_description = lines.peek().is_some_and(|line| line.starts_with(' '));
            let description = if has_description {
                lines.next().map(|line| line.trim().to_owned())
            } else {
                None
            };

            let candidate_match = match_kind(package_id, query);
            candidates.push(PackageCandidate {
                backend_id: self.id().to_owned(),
                backend_name: self.display_name().to_owned(),
                category: self.category(),
                domain: PackageDomain::System,
                package_id: package_id.to_owned(),
                display_name: package_id.to_owned(),
                version,
                description,
                source: Some(repository.to_owned()),
                installers: vec![self.display_name().to_owned()],
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

        Ok(candidates)
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
