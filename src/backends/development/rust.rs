use crate::{
    backends::{
        contract::{command_path, BackendOperationCapability},
        util::{capture_checked_with_privilege, match_kind, parse_key_value_lines},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpResult, BackendCategory, Capability, DeveloperTarget, ExecutionPlan, InstalledPackage,
        MaintenancePlan, NativeCommand, OperationKind, OperationStatus, PackageCandidate,
        PackageDomain, PackageInfo, PrivilegeRequirement,
    },
    execution::ProcessRunner,
};

pub struct RustBackend;

const CAPABILITIES: &[Capability] = &[
    Capability::Search,
    Capability::Install,
    Capability::Remove,
    Capability::Upgrade,
    Capability::List,
    Capability::Info,
];

const REQUIREMENTS: &[CommandRequirement] = &[CommandRequirement {
    key: "cargo",
    alternatives: &["cargo"],
}];

const OPTIONAL: &[CommandRequirement] = &[
    CommandRequirement {
        key: "rustc",
        alternatives: &["rustc"],
    },
    CommandRequirement {
        key: "cargo-install-update",
        alternatives: &["cargo-install-update"],
    },
];

const DOMAINS: &[PackageDomain] = &[PackageDomain::Rust];

impl Backend for RustBackend {
    fn id(&self) -> &'static str {
        "rust"
    }

    fn display_name(&self) -> &'static str {
        "Rust / Cargo"
    }

    fn category(&self) -> BackendCategory {
        BackendCategory::Development
    }

    fn capabilities(&self) -> &'static [Capability] {
        CAPABILITIES
    }

    fn command_requirements(&self) -> &'static [CommandRequirement] {
        REQUIREMENTS
    }

    fn optional_command_requirements(&self) -> &'static [CommandRequirement] {
        OPTIONAL
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["cargo", "crates", "crates.io", "rustlang"]
    }

    fn package_domains(&self) -> &'static [PackageDomain] {
        DOMAINS
    }

    fn operation_capability(&self, capability: Capability) -> BackendOperationCapability {
        match capability {
            Capability::Upgrade => BackendOperationCapability::InstalledPackageUpgrade,
            _ => BackendOperationCapability::Unsupported,
        }
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        let cargo = command_path(self, commands, "cargo")?;
        let output = capture_checked_with_privilege(
            self,
            runner,
            NativeCommand::new(cargo).args(["search", query, "--limit", "20"]),
            PrivilegeRequirement::OriginalUserRequired,
        )?;
        Ok(parse_search(self, &output, query))
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let cargo = command_path(self, commands, "cargo")?;
        let output = capture_checked_with_privilege(
            self,
            runner,
            NativeCommand::new(cargo).args(["install", "--list"]),
            PrivilegeRequirement::OriginalUserRequired,
        )?;
        Ok(parse_installed(self, &output))
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        let output = self.raw_info(commands, runner, package_id)?;
        Ok(parse_info(self, package_id, &output))
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        let cargo = command_path(self, commands, "cargo")?;
        capture_checked_with_privilege(
            self,
            runner,
            NativeCommand::new(cargo).args(["info", package_id]),
            PrivilegeRequirement::OriginalUserRequired,
        )
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let cargo = command_path(self, commands, "cargo")?;
        Ok(plan(
            self,
            OperationKind::Install,
            "Compile and install Rust binary crate",
            Some(candidate.package_id.clone()),
            NativeCommand::new(cargo).args(["install", "--", candidate.package_id.as_str()]),
        ))
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let cargo = command_path(self, commands, "cargo")?;
        Ok(plan(
            self,
            OperationKind::Remove,
            "Remove Cargo-installed Rust binary crate",
            Some(package.package_id.clone()),
            NativeCommand::new(cargo).args(["uninstall", "--", package.package_id.as_str()]),
        ))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        if target.is_some_and(|target| {
            !matches!(
                target,
                DeveloperTarget::Global | DeveloperTarget::Tools | DeveloperTarget::All
            )
        }) {
            return Ok(MaintenancePlan {
                plans: Vec::new(),
                records: vec![MaintenancePlan::record(
                    self.id(),
                    self.display_name(),
                    OperationStatus::NotApplicable,
                    "Cargo host maintenance only manages globally installed binary crates",
                )],
            });
        }

        if !commands.contains_key("cargo-install-update") {
            return Ok(MaintenancePlan {
                plans: Vec::new(),
                records: vec![MaintenancePlan::record(
                    self.id(),
                    self.display_name(),
                    OperationStatus::Unavailable,
                    "upgrading installed Cargo binaries requires the optional cargo-update crate (`cargo install cargo-update`)",
                )],
            });
        }

        let cargo = command_path(self, commands, "cargo")?;
        Ok(MaintenancePlan::from_plans(vec![plan(
            self,
            OperationKind::Upgrade,
            "Upgrade Cargo-installed Rust binary crates",
            None,
            NativeCommand::new(cargo).args(["install-update", "--all"]),
        )]))
    }
}

fn parse_search(backend: &RustBackend, output: &str, query: &str) -> Vec<PackageCandidate> {
    output
        .lines()
        .filter_map(|line| {
            let (package_id, rest) = line.split_once(" = ")?;
            let package_id = package_id.trim();
            if package_id.is_empty() {
                return None;
            }
            let (version, description) = rest
                .split_once('#')
                .map(|(version, description)| {
                    (
                        clean_quoted(version),
                        (!description.trim().is_empty()).then(|| description.trim().to_owned()),
                    )
                })
                .unwrap_or_else(|| (clean_quoted(rest), None));
            let candidate_match = match_kind(package_id, query);
            Some(PackageCandidate {
                backend_id: backend.id().to_owned(),
                backend_name: backend.display_name().to_owned(),
                category: backend.category(),
                domain: PackageDomain::Rust,
                package_id: package_id.to_owned(),
                display_name: package_id.to_owned(),
                version,
                description,
                source: Some("crates.io".to_owned()),
                installers: vec!["cargo".to_owned()],
                artifact_kind: "Rust binary crate".to_owned(),
                scope: Some("global user tool".to_owned()),
                match_kind: candidate_match,
                identity: PackageCandidate::infer_identity(
                    candidate_match,
                    PackageDomain::Rust,
                    "Rust binary crate",
                ),
                metadata: Default::default(),
            })
        })
        .collect()
}

fn clean_quoted(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('"').trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn parse_installed(backend: &RustBackend, output: &str) -> Vec<InstalledPackage> {
    output
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .filter_map(|line| {
            let header = line.trim().strip_suffix(':')?;
            let mut parts = header.split_whitespace();
            let package_id = parts.next()?;
            let version = parts
                .next()
                .and_then(|value| value.strip_prefix('v'))
                .map(str::to_owned);
            Some(InstalledPackage {
                backend_id: backend.id().to_owned(),
                backend_name: backend.display_name().to_owned(),
                category: backend.category(),
                domain: PackageDomain::Rust,
                package_id: package_id.to_owned(),
                display_name: package_id.to_owned(),
                version,
                description: Some("Cargo-installed binary crate".to_owned()),
                source: Some("Cargo install registry or source".to_owned()),
                scope: Some("global user tool".to_owned()),
            })
        })
        .collect()
}

fn parse_info(backend: &RustBackend, package_id: &str, output: &str) -> PackageInfo {
    let fields = parse_key_value_lines(output);
    let first_line = output.lines().map(str::trim).find(|line| !line.is_empty());
    let description = first_line.and_then(|line| {
        line.split_once('#')
            .map(|(_, description)| description.trim().to_owned())
            .filter(|description| !description.is_empty())
    });
    PackageInfo {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        category: backend.category(),
        domain: PackageDomain::Rust,
        package_id: package_id.to_owned(),
        display_name: package_id.to_owned(),
        version: fields.get("version").cloned(),
        description,
        source: fields
            .get("repository")
            .cloned()
            .or_else(|| fields.get("crates.io").cloned())
            .or_else(|| Some("crates.io".to_owned())),
        scope: Some("global user tool".to_owned()),
        artifact_kind: Some("Rust binary crate".to_owned()),
        installed: None,
        extra: fields
            .into_iter()
            .filter(|(key, _)| !matches!(key.as_str(), "version" | "repository" | "crates.io"))
            .collect(),
    }
}

fn plan(
    backend: &RustBackend,
    operation: OperationKind,
    action: &str,
    package_id: Option<String>,
    command: NativeCommand,
) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation,
        action: action.to_owned(),
        package_id,
        source: Some("crates.io / Cargo install sources".to_owned()),
        scope: Some("global user tool".to_owned()),
        details: vec![(
            "Safety".to_owned(),
            "Cargo compiles source locally; crate build scripts execute as the selected user"
                .to_owned(),
        )],
        command,
        privilege: PrivilegeRequirement::OriginalUserRequired,
        requires_root: false,
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
    fn parses_crates_io_search_rows() {
        let packages = parse_search(
            &RustBackend,
            "ripgrep = \"14.1.1\" # Line-oriented search tool\ncargo-edit = \"0.13.7\" # Cargo helpers\n",
            "ripgrep",
        );

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package_id, "ripgrep");
        assert_eq!(packages[0].version.as_deref(), Some("14.1.1"));
        assert_eq!(packages[0].domain, PackageDomain::Rust);
    }

    #[test]
    fn parses_cargo_installed_binary_crates_without_binary_rows() {
        let packages = parse_installed(
            &RustBackend,
            "ripgrep v14.1.1:\n    rg\ncargo-edit v0.13.7:\n    cargo-add\n    cargo-rm\n",
        );

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package_id, "ripgrep");
        assert_eq!(packages[0].version.as_deref(), Some("14.1.1"));
    }

    #[test]
    fn install_plan_keeps_argument_boundary_and_user_scope() {
        let mut commands = CommandMap::new();
        commands.insert(
            "cargo".to_owned(),
            PathBuf::from("/home/alice/.cargo/bin/cargo"),
        );
        let candidate = parse_search(&RustBackend, "demo = \"1.0.0\" # Demo\n", "demo").remove(0);
        let plan = RustBackend
            .plan_install(&commands, &candidate)
            .expect("Cargo install plan should be created");

        assert_eq!(plan.command.args, ["install", "--", "demo"]);
        assert_eq!(plan.privilege, PrivilegeRequirement::OriginalUserRequired);
    }

    #[test]
    fn global_upgrade_requires_and_uses_cargo_update_without_project_commands() {
        let mut commands = CommandMap::new();
        commands.insert("cargo".to_owned(), PathBuf::from("/usr/bin/cargo"));
        commands.insert(
            "cargo-install-update".to_owned(),
            PathBuf::from("/home/alice/.cargo/bin/cargo-install-update"),
        );

        let maintenance = RustBackend
            .plan_upgrade(
                &commands,
                &NoopRunner,
                Some("cargo"),
                Some(DeveloperTarget::Global),
            )
            .expect("Cargo upgrade plan should be created");
        let plan = maintenance.plans.first().expect("upgrade plan");

        assert_eq!(plan.command.args, ["install-update", "--all"]);
        assert!(!plan.command.args.iter().any(|argument| argument == "add"));
        assert_eq!(plan.privilege, PrivilegeRequirement::OriginalUserRequired);
    }

    #[test]
    fn project_upgrade_is_explicitly_not_applicable() {
        let mut commands = CommandMap::new();
        commands.insert("cargo".to_owned(), PathBuf::from("/usr/bin/cargo"));

        let maintenance = RustBackend
            .plan_upgrade(
                &commands,
                &NoopRunner,
                Some("cargo"),
                Some(DeveloperTarget::Project),
            )
            .expect("project target should be reported without mutation");

        assert!(maintenance.plans.is_empty());
        assert_eq!(maintenance.records.len(), 1);
        assert!(matches!(
            maintenance.records[0].status,
            OperationStatus::NotApplicable
        ));
    }
}
