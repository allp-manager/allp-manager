use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked, match_kind, parse_key_value_lines},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpResult, BackendCategory, BackendOperationRecord, Capability, DeveloperTarget,
        ExecutionPlan, InstalledPackage, MaintenancePlan, NativeCommand, OperationKind,
        OperationStatus, PackageCandidate, PackageDomain, PackageInfo, PrivilegeRequirement,
    },
    execution::{ProcessRunner, ProcessStatus},
};
use std::time::Duration;

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
            let publisher = columns.get(2).map(|value| (*value).to_owned());
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
                source: publisher,
                installers: vec![self.display_name().to_owned()],
                artifact_kind: "universal application".to_owned(),
                scope: Some("system".to_owned()),
                match_kind: candidate_match,
                identity: PackageCandidate::infer_identity(
                    candidate_match,
                    PackageDomain::Universal,
                    "universal application",
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
        let fields = parse_key_value_lines(&output);
        Ok(PackageInfo {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::Universal,
            package_id: fields
                .get("name")
                .cloned()
                .unwrap_or_else(|| package_id.to_owned()),
            display_name: fields
                .get("name")
                .cloned()
                .unwrap_or_else(|| package_id.to_owned()),
            version: fields.get("version").cloned(),
            description: fields
                .get("summary")
                .cloned()
                .or_else(|| fields.get("description").cloned()),
            source: fields.get("publisher").cloned(),
            scope: Some("system".to_owned()),
            artifact_kind: Some("universal application".to_owned()),
            installed: None,
            extra: fields
                .into_iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "name" | "version" | "summary" | "description" | "publisher"
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
        let snap = command_path(self, commands, "snap")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(snap).args(["info", package_id]),
        )
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let snap = command_path(self, commands, "snap")?;
        Ok(snap_plan(
            self,
            snap,
            PlanSpec {
                operation: OperationKind::Install,
                action: "Install snap package",
                package_id: Some(candidate.package_id.clone()),
                source: candidate.source.clone(),
                scope: candidate.scope.clone(),
                args: vec!["install".into(), candidate.package_id.clone()],
            },
        ))
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
    use super::parse_snap_refresh_status;
    use crate::domain::OperationStatus;

    #[test]
    fn all_snaps_up_to_date_maps_to_up_to_date() {
        let (status, message) =
            parse_snap_refresh_status("All snaps up to date.\n", "").expect("snap output parses");

        assert!(matches!(status, OperationStatus::UpToDate));
        assert_eq!(message, "all snaps up to date");
    }
}
