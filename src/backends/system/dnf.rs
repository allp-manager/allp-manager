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

pub struct DnfBackend;

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
    key: "dnf",
    alternatives: &["dnf5", "dnf"],
}];

impl Backend for DnfBackend {
    fn id(&self) -> &'static str {
        "dnf"
    }
    fn display_name(&self) -> &'static str {
        "DNF"
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
        let dnf = command_path(self, commands, "dnf")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(dnf).args(["-q", "search", query]),
        )?;
        let mut candidates = Vec::new();

        for line in output.lines() {
            let Some((left, description)) = line.split_once(" : ") else {
                continue;
            };
            let package_id = left.trim();
            if package_id.is_empty() || package_id.starts_with("Matched") {
                continue;
            }
            let base_name = package_id.split('.').next().unwrap_or(package_id);
            let candidate_match = match_kind(base_name, query);
            candidates.push(PackageCandidate {
                backend_id: self.id().to_owned(),
                backend_name: self.display_name().to_owned(),
                category: self.category(),
                domain: PackageDomain::System,
                package_id: package_id.to_owned(),
                display_name: base_name.to_owned(),
                version: None,
                description: Some(description.trim().to_owned()),
                source: Some("DNF repositories".to_owned()),
                installers: vec![self.display_name().to_owned()],
                artifact_kind: "system package".to_owned(),
                scope: Some("system".to_owned()),
                match_kind: candidate_match,
                identity: PackageCandidate::infer_identity(
                    candidate_match,
                    PackageDomain::System,
                    "system package",
                ),
            });
        }

        Ok(candidates)
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let dnf = command_path(self, commands, "dnf")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(dnf).args(["-q", "list", "--installed"]),
        )?;
        Ok(output
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with("Installed") {
                    return None;
                }
                let mut parts = line.split_whitespace();
                let package_id = parts.next()?;
                let version = parts.next().map(str::to_owned);
                let source = parts.next().map(str::to_owned);
                Some(InstalledPackage {
                    backend_id: self.id().to_owned(),
                    backend_name: self.display_name().to_owned(),
                    category: self.category(),
                    domain: PackageDomain::System,
                    package_id: package_id.to_owned(),
                    display_name: package_id
                        .split('.')
                        .next()
                        .unwrap_or(package_id)
                        .to_owned(),
                    version,
                    description: None,
                    source,
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
        let dnf = command_path(self, commands, "dnf")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(dnf).args(["-q", "info", package_id]),
        )?;
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
            description: fields
                .get("Summary")
                .cloned()
                .or_else(|| fields.get("Description").cloned()),
            source: fields
                .get("Repository")
                .cloned()
                .or_else(|| fields.get("From repo").cloned()),
            scope: Some("system".to_owned()),
            artifact_kind: Some("system package".to_owned()),
            installed: None,
            extra: fields
                .into_iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "Name" | "Version" | "Summary" | "Description" | "Repository" | "From repo"
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
        let dnf = command_path(self, commands, "dnf")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(dnf).args(["-q", "info", package_id]),
        )
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let dnf = command_path(self, commands, "dnf")?;
        Ok(make_plan(
            self,
            dnf,
            PlanSpec {
                operation: OperationKind::Install,
                action: "Install system package",
                package_id: Some(candidate.package_id.clone()),
                source: candidate.source.clone(),
                scope: candidate.scope.clone(),
                args: vec!["install".into(), "--".into(), candidate.package_id.clone()],
            },
        ))
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let dnf = command_path(self, commands, "dnf")?;
        Ok(make_plan(
            self,
            dnf,
            PlanSpec {
                operation: OperationKind::Remove,
                action: "Remove system package",
                package_id: Some(package.package_id.clone()),
                source: package.source.clone(),
                scope: package.scope.clone(),
                args: vec!["remove".into(), "--".into(), package.package_id.clone()],
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
        let dnf = command_path(self, commands, "dnf")?;
        Ok(MaintenancePlan::from_plans(vec![make_plan(
            self,
            dnf,
            PlanSpec {
                operation: OperationKind::Update,
                action: "Refresh package metadata cache",
                package_id: None,
                source: Some("DNF repositories".to_owned()),
                scope: Some("system".to_owned()),
                args: vec!["makecache".into()],
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
        let dnf = command_path(self, commands, "dnf")?;
        Ok(MaintenancePlan::from_plans(vec![make_plan(
            self,
            dnf,
            PlanSpec {
                operation: OperationKind::Upgrade,
                action: "Upgrade installed DNF packages",
                package_id: None,
                source: Some("DNF repositories".to_owned()),
                scope: Some("system".to_owned()),
                args: vec!["upgrade".into()],
            },
        )]))
    }
}

struct PlanSpec {
    operation: OperationKind,
    action: &'static str,
    package_id: Option<String>,
    source: Option<String>,
    scope: Option<String>,
    args: Vec<String>,
}

fn make_plan(backend: &DnfBackend, program: &std::path::Path, spec: PlanSpec) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation: spec.operation,
        action: spec.action.to_owned(),
        package_id: spec.package_id,
        source: spec.source,
        scope: spec.scope,
        command: NativeCommand::new(program).args(spec.args),
        privilege: PrivilegeRequirement::RootRequired,
        requires_root: true,
        interactive: true,
    }
}
