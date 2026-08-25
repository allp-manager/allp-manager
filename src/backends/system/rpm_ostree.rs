use crate::{
    backends::{
        contract::{command_path, BackendOperationCapability},
        util::{capture_checked, match_kind},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, Capability, DeveloperTarget, ExecutionPlan,
        InstalledPackage, MaintenancePlan, NativeCommand, OperationKind, PackageCandidate,
        PackageDomain, PackageInfo, PrivilegeRequirement,
    },
    execution::ProcessRunner,
};
use serde::Deserialize;

pub struct RpmOstreeBackend;

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
    key: "rpm-ostree",
    alternatives: &["rpm-ostree"],
}];

impl Backend for RpmOstreeBackend {
    fn id(&self) -> &'static str {
        "rpm-ostree"
    }

    fn display_name(&self) -> &'static str {
        "rpm-ostree (Bazzite / Atomic)"
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

    fn aliases(&self) -> &'static [&'static str] {
        &["rpmostree", "bazzite", "fedora-atomic", "atomic"]
    }

    fn operation_capability(&self, capability: Capability) -> BackendOperationCapability {
        match capability {
            Capability::Update => BackendOperationCapability::MetadataRefresh,
            Capability::Upgrade => BackendOperationCapability::InstalledPackageUpgrade,
            _ => BackendOperationCapability::Unsupported,
        }
    }

    fn probe(&self, commands: &CommandMap, runner: &dyn ProcessRunner) -> AllpResult<()> {
        let rpm_ostree = command_path(self, commands, "rpm-ostree")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(rpm_ostree).args(["status", "--json"]),
        )?;
        parse_layered_packages(self, &output).map(|_| ())
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        let rpm_ostree = command_path(self, commands, "rpm-ostree")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(rpm_ostree).args(["search", query]),
        )?;
        Ok(parse_search(self, &output, query))
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let rpm_ostree = command_path(self, commands, "rpm-ostree")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(rpm_ostree).args(["status", "--json"]),
        )?;
        parse_layered_packages(self, &output)
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        let output = self.raw_info(commands, runner, package_id)?;
        let candidates = parse_search(self, &output, package_id);
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.package_id.eq_ignore_ascii_case(package_id))
            .or_else(|| candidates.first());
        Ok(PackageInfo {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::System,
            package_id: candidate
                .map(|candidate| candidate.package_id.clone())
                .unwrap_or_else(|| package_id.to_owned()),
            display_name: candidate
                .map(|candidate| candidate.display_name.clone())
                .unwrap_or_else(|| package_id.to_owned()),
            version: candidate.and_then(|candidate| candidate.version.clone()),
            description: candidate.and_then(|candidate| candidate.description.clone()),
            source: Some("rpm-md repositories for the current atomic image".to_owned()),
            scope: Some("layered host package; staged deployment".to_owned()),
            artifact_kind: Some("rpm-ostree layered package".to_owned()),
            installed: None,
            extra: vec![("Native search".to_owned(), output.trim().to_owned())],
        })
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        let rpm_ostree = command_path(self, commands, "rpm-ostree")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(rpm_ostree).args(["search", package_id]),
        )
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let rpm_ostree = command_path(self, commands, "rpm-ostree")?;
        Ok(layering_plan(
            self,
            OperationKind::Install,
            "Layer package into the Bazzite/Atomic host image",
            Some(candidate.package_id.clone()),
            NativeCommand::new(rpm_ostree).args(["install", "--", candidate.package_id.as_str()]),
        ))
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let rpm_ostree = command_path(self, commands, "rpm-ostree")?;
        Ok(layering_plan(
            self,
            OperationKind::Remove,
            "Remove layered package from the Bazzite/Atomic host image",
            Some(package.package_id.clone()),
            NativeCommand::new(rpm_ostree).args(["uninstall", "--", package.package_id.as_str()]),
        ))
    }

    fn plan_update(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let rpm_ostree = command_path(self, commands, "rpm-ostree")?;
        Ok(MaintenancePlan::from_plans(vec![system_plan(
            self,
            OperationKind::Update,
            "Refresh rpm-md metadata for layered host packages",
            NativeCommand::new(rpm_ostree).arg("refresh-md"),
        )]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let rpm_ostree = command_path(self, commands, "rpm-ostree")?;
        let mut plan = system_plan(
            self,
            OperationKind::Upgrade,
            "Stage the next Bazzite/Atomic system image",
            NativeCommand::new(rpm_ostree).arg("upgrade"),
        );
        plan.details.push((
            "Activation".to_owned(),
            "The new deployment takes effect after reboot".to_owned(),
        ));
        Ok(MaintenancePlan::from_plans(vec![plan]))
    }
}

fn parse_search(backend: &RpmOstreeBackend, output: &str, query: &str) -> Vec<PackageCandidate> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with('=')
                || line.starts_with("Importing ")
                || line.starts_with("Enabled rpm-md")
                || line.starts_with("Updating rpm-md")
            {
                return None;
            }
            let (raw_id, description) = line.split_once(" : ")?;
            let package_id = raw_id.trim();
            if package_id.is_empty() || package_id.contains(char::is_whitespace) {
                return None;
            }
            let display_name = strip_rpm_architecture(package_id);
            let candidate_match = match_kind(&display_name, query);
            Some(PackageCandidate {
                backend_id: backend.id().to_owned(),
                backend_name: backend.display_name().to_owned(),
                category: backend.category(),
                domain: PackageDomain::System,
                package_id: package_id.to_owned(),
                display_name,
                version: None,
                description: (!description.trim().is_empty())
                    .then(|| description.trim().to_owned()),
                source: Some("rpm-md repositories for the current atomic image".to_owned()),
                installers: vec!["rpm-ostree".to_owned()],
                artifact_kind: "rpm-ostree layered package".to_owned(),
                scope: Some("layered host package; staged deployment".to_owned()),
                match_kind: candidate_match,
                identity: PackageCandidate::infer_identity(
                    candidate_match,
                    PackageDomain::System,
                    "rpm-ostree layered package",
                ),
                metadata: [(
                    "layering_warning".to_owned(),
                    "Bazzite recommends Homebrew, Flatpak, or containers before host package layering"
                        .to_owned(),
                )]
                .into_iter()
                .collect(),
            })
        })
        .collect()
}

fn strip_rpm_architecture(package_id: &str) -> String {
    const ARCHES: &[&str] = &[
        "x86_64", "aarch64", "i686", "i586", "armv7hl", "ppc64le", "s390x", "noarch",
    ];
    package_id
        .rsplit_once('.')
        .filter(|(_, architecture)| ARCHES.contains(architecture))
        .map(|(name, _)| name)
        .unwrap_or(package_id)
        .to_owned()
}

#[derive(Debug, Deserialize)]
struct StatusDocument {
    #[serde(default)]
    deployments: Vec<Deployment>,
}

#[derive(Debug, Deserialize)]
struct Deployment {
    #[serde(default)]
    booted: bool,
    #[serde(default, rename = "requested-packages")]
    requested_packages: Vec<String>,
    #[serde(default, rename = "requested-local-packages")]
    requested_local_packages: Vec<String>,
}

fn parse_layered_packages(
    backend: &RpmOstreeBackend,
    output: &str,
) -> AllpResult<Vec<InstalledPackage>> {
    let status: StatusDocument =
        serde_json::from_str(output).map_err(|error| AllpError::MetadataParseFailed {
            backend: backend.display_name().to_owned(),
            message: format!("invalid `rpm-ostree status --json` output: {error}"),
        })?;
    let Some(deployment) = status
        .deployments
        .iter()
        .find(|deployment| deployment.booted)
        .or_else(|| status.deployments.first())
    else {
        return Ok(Vec::new());
    };

    let mut packages = deployment
        .requested_packages
        .iter()
        .map(|package_id| installed_package(backend, package_id, "rpm-md layered package"))
        .collect::<Vec<_>>();
    packages.extend(
        deployment
            .requested_local_packages
            .iter()
            .map(|package_id| installed_package(backend, package_id, "local RPM layered package")),
    );
    packages.sort_by(|left, right| left.package_id.cmp(&right.package_id));
    packages.dedup_by(|left, right| left.package_id == right.package_id);
    Ok(packages)
}

fn installed_package(
    backend: &RpmOstreeBackend,
    package_id: &str,
    description: &str,
) -> InstalledPackage {
    InstalledPackage {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        category: backend.category(),
        domain: PackageDomain::System,
        package_id: package_id.to_owned(),
        display_name: strip_rpm_architecture(package_id),
        version: None,
        description: Some(description.to_owned()),
        source: Some("current rpm-ostree deployment request".to_owned()),
        scope: Some("layered host package; staged deployment".to_owned()),
    }
}

fn layering_plan(
    backend: &RpmOstreeBackend,
    operation: OperationKind,
    action: &str,
    package_id: Option<String>,
    command: NativeCommand,
) -> ExecutionPlan {
    let mut plan = system_plan(backend, operation, action, command);
    plan.package_id = package_id;
    plan.details.push((
        "Bazzite policy".to_owned(),
        "Host layering is a last resort; prefer Homebrew for CLI tools, Flatpak for apps, or a container"
            .to_owned(),
    ));
    plan.details.push((
        "Activation".to_owned(),
        "The staged package change normally takes effect after reboot".to_owned(),
    ));
    plan
}

fn system_plan(
    backend: &RpmOstreeBackend,
    operation: OperationKind,
    action: &str,
    command: NativeCommand,
) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation,
        action: action.to_owned(),
        package_id: None,
        source: Some("Bazzite/Atomic host image and rpm-md repositories".to_owned()),
        scope: Some("transactional host deployment".to_owned()),
        details: Vec::new(),
        command,
        privilege: PrivilegeRequirement::RootRequired,
        requires_root: true,
        interactive: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct NoopRunner;

    impl ProcessRunner for NoopRunner {
        fn capture(&self, _command: &NativeCommand) -> AllpResult<crate::execution::CommandOutput> {
            unreachable!("plan construction must not execute a command")
        }

        fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<crate::execution::ProcessStatus> {
            unreachable!("plan construction must not execute a command")
        }
    }

    #[test]
    fn parses_bazzite_search_output() {
        let packages = parse_search(
            &RpmOstreeBackend,
            "================ Name Matched ================\nhtop.x86_64 : Interactive process viewer\n",
            "htop",
        );

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package_id, "htop.x86_64");
        assert_eq!(packages[0].display_name, "htop");
    }

    #[test]
    fn lists_only_requested_layered_packages_from_booted_deployment() {
        let packages = parse_layered_packages(
            &RpmOstreeBackend,
            r#"{
                "deployments": [
                    {"booted": false, "requested-packages": ["pending-only"]},
                    {"booted": true, "requested-packages": ["htop", "fish"],
                     "requested-local-packages": ["vendor-tool-1.0-1.x86_64"]}
                ]
            }"#,
        )
        .expect("rpm-ostree status JSON should parse");

        assert_eq!(packages.len(), 3);
        assert!(packages.iter().any(|package| package.package_id == "htop"));
        assert!(!packages
            .iter()
            .any(|package| package.package_id == "pending-only"));
    }

    #[test]
    fn package_layering_plan_is_transactional_and_root_scoped() {
        let mut commands = CommandMap::new();
        commands.insert(
            "rpm-ostree".to_owned(),
            PathBuf::from("/usr/bin/rpm-ostree"),
        );
        let candidate = parse_search(
            &RpmOstreeBackend,
            "htop.x86_64 : Interactive process viewer\n",
            "htop",
        )
        .remove(0);
        let plan = RpmOstreeBackend
            .plan_install(&commands, &candidate)
            .expect("layering plan should be created");

        assert_eq!(plan.command.args, ["install", "--", "htop.x86_64"]);
        assert_eq!(plan.privilege, PrivilegeRequirement::RootRequired);
        assert!(plan
            .details
            .iter()
            .any(|(key, value)| key == "Bazzite policy" && value.contains("last resort")));
    }

    #[test]
    fn bazzite_maintenance_separates_metadata_refresh_from_image_upgrade() {
        let mut commands = CommandMap::new();
        commands.insert(
            "rpm-ostree".to_owned(),
            PathBuf::from("/usr/bin/rpm-ostree"),
        );

        let update = RpmOstreeBackend
            .plan_update(&commands, &NoopRunner, Some("bazzite"), None)
            .expect("metadata refresh plan")
            .plans
            .remove(0);
        let upgrade = RpmOstreeBackend
            .plan_upgrade(&commands, &NoopRunner, Some("bazzite"), None)
            .expect("image upgrade plan")
            .plans
            .remove(0);

        assert_eq!(update.command.args, ["refresh-md"]);
        assert_eq!(upgrade.command.args, ["upgrade"]);
        assert!(upgrade
            .details
            .iter()
            .any(|(key, value)| key == "Activation" && value.contains("reboot")));
    }
}
