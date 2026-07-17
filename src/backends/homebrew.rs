use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked, match_kind},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpResult, BackendCategory, Capability, DeveloperTarget, ExecutionPlan, InstalledPackage,
        MaintenancePlan, NativeCommand, OperationKind, PackageCandidate, PackageDomain,
        PackageInfo, PrivilegeRequirement,
    },
    execution::{CommandOutput, ProcessRunner},
};

pub struct HomebrewBackend;

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
    key: "brew",
    alternatives: &[
        "brew",
        "/opt/homebrew/bin/brew",
        "/usr/local/bin/brew",
        "/home/linuxbrew/.linuxbrew/bin/brew",
    ],
}];
const DOMAINS: &[PackageDomain] = &[PackageDomain::Homebrew];

impl Backend for HomebrewBackend {
    fn id(&self) -> &'static str {
        "brew"
    }

    fn display_name(&self) -> &'static str {
        "Homebrew"
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

    fn aliases(&self) -> &'static [&'static str] {
        &["homebrew", "linuxbrew"]
    }

    fn package_domains(&self) -> &'static [PackageDomain] {
        DOMAINS
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        let brew = command_path(self, commands, "brew")?;
        let mut candidates = Vec::new();
        append_search(
            self,
            runner.capture(&NativeCommand::new(brew).args(["search", "--formula", query])),
            query,
            "Homebrew formulae",
            "Homebrew formula",
            &mut candidates,
        );
        append_search(
            self,
            runner.capture(&NativeCommand::new(brew).args(["search", "--cask", query])),
            query,
            "Homebrew casks",
            "Homebrew cask",
            &mut candidates,
        );

        if candidates.is_empty() {
            let output = capture_checked(
                self,
                runner,
                NativeCommand::new(brew).args(["search", query]),
            )?;
            append_lines(
                self,
                &output,
                query,
                "Homebrew",
                "Homebrew formula",
                &mut candidates,
            );
        }

        Ok(candidates)
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let brew = command_path(self, commands, "brew")?;
        let mut packages = Vec::new();
        append_installed(
            self,
            &capture_checked(
                self,
                runner,
                NativeCommand::new(brew).args(["list", "--formula", "--versions"]),
            )
            .unwrap_or_default(),
            "Homebrew formulae",
            "formula",
            &mut packages,
        );
        append_installed(
            self,
            &capture_checked(
                self,
                runner,
                NativeCommand::new(brew).args(["list", "--cask", "--versions"]),
            )
            .unwrap_or_default(),
            "Homebrew casks",
            "cask",
            &mut packages,
        );
        Ok(packages)
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        let brew = command_path(self, commands, "brew")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(brew).args(["info", package_id]),
        )?;
        Ok(PackageInfo {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::Homebrew,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: first_version(&output),
            description: first_nonempty_line(&output),
            source: Some("Homebrew".to_owned()),
            scope: Some("current user".to_owned()),
            artifact_kind: Some("Homebrew formula or cask".to_owned()),
            installed: None,
            extra: vec![("Native info".to_owned(), output.trim().to_owned())],
        })
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        let brew = command_path(self, commands, "brew")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(brew).args(["info", package_id]),
        )
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let brew = command_path(self, commands, "brew")?;
        let mut command = NativeCommand::new(brew).arg("install");
        if candidate
            .artifact_kind
            .eq_ignore_ascii_case("Homebrew cask")
        {
            command = command.arg("--cask");
        }
        command = command.arg(candidate.package_id.as_str());
        Ok(plan(
            self,
            OperationKind::Install,
            "Install Homebrew package",
            Some(candidate.package_id.clone()),
            candidate.source.clone(),
            command,
        ))
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let brew = command_path(self, commands, "brew")?;
        let mut command = NativeCommand::new(brew).arg("uninstall");
        if package
            .source
            .as_deref()
            .is_some_and(|source| source.eq_ignore_ascii_case("Homebrew casks"))
        {
            command = command.arg("--cask");
        }
        command = command.arg(package.package_id.as_str());
        Ok(plan(
            self,
            OperationKind::Remove,
            "Remove Homebrew package",
            Some(package.package_id.clone()),
            package.source.clone(),
            command,
        ))
    }

    fn plan_update(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let brew = command_path(self, commands, "brew")?;
        Ok(MaintenancePlan::from_plans(vec![plan(
            self,
            OperationKind::Update,
            "Refresh Homebrew formula and cask metadata",
            None,
            Some("Homebrew".to_owned()),
            NativeCommand::new(brew).arg("update"),
        )]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let brew = command_path(self, commands, "brew")?;
        Ok(MaintenancePlan::from_plans(vec![plan(
            self,
            OperationKind::Upgrade,
            "Upgrade installed Homebrew packages",
            None,
            Some("Homebrew".to_owned()),
            NativeCommand::new(brew).arg("upgrade"),
        )]))
    }
}

fn append_search(
    backend: &HomebrewBackend,
    result: AllpResult<CommandOutput>,
    query: &str,
    source: &str,
    artifact_kind: &str,
    candidates: &mut Vec<PackageCandidate>,
) {
    if let Ok(output) = result {
        if output.success {
            append_lines(
                backend,
                &output.stdout,
                query,
                source,
                artifact_kind,
                candidates,
            );
        }
    }
}

fn append_lines(
    backend: &HomebrewBackend,
    output: &str,
    query: &str,
    source: &str,
    artifact_kind: &str,
    candidates: &mut Vec<PackageCandidate>,
) {
    for line in output.lines() {
        let package_id = line.trim();
        if package_id.is_empty() || package_id.starts_with("==>") {
            continue;
        }
        let candidate_match = match_kind(package_id, query);
        candidates.push(PackageCandidate {
            backend_id: backend.id().to_owned(),
            backend_name: backend.display_name().to_owned(),
            category: backend.category(),
            domain: PackageDomain::Homebrew,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: None,
            description: None,
            source: Some(source.to_owned()),
            installers: vec!["brew".to_owned()],
            artifact_kind: artifact_kind.to_owned(),
            scope: Some("current user".to_owned()),
            match_kind: candidate_match,
            identity: PackageCandidate::infer_identity(
                candidate_match,
                PackageDomain::Homebrew,
                artifact_kind,
            ),
            metadata: Default::default(),
        });
    }
}

fn append_installed(
    backend: &HomebrewBackend,
    output: &str,
    source: &str,
    artifact: &str,
    packages: &mut Vec<InstalledPackage>,
) {
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let Some(package_id) = parts.next() else {
            continue;
        };
        packages.push(InstalledPackage {
            backend_id: backend.id().to_owned(),
            backend_name: backend.display_name().to_owned(),
            category: backend.category(),
            domain: PackageDomain::Homebrew,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: parts.next().map(str::to_owned),
            description: Some(artifact.to_owned()),
            source: Some(source.to_owned()),
            scope: Some("current user".to_owned()),
        });
    }
}

fn plan(
    backend: &HomebrewBackend,
    operation: OperationKind,
    action: &str,
    package_id: Option<String>,
    source: Option<String>,
    command: NativeCommand,
) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation,
        action: action.to_owned(),
        package_id,
        source,
        scope: Some("current user".to_owned()),
        details: Vec::new(),
        command,
        privilege: PrivilegeRequirement::OriginalUserRequired,
        requires_root: false,
        interactive: true,
    }
}

fn first_nonempty_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("==>"))
        .map(str::to_owned)
}

fn first_version(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let _name = parts.next()?;
        parts.next().map(str::to_owned)
    })
}
