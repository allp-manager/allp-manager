//! AUR operations stay with an installed helper; Allp does not build PKGBUILDs.
use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked_with_privilege, match_kind, parse_key_value_lines},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, BackendOperationRecord, BackendSearchReport,
        Capability, DeveloperTarget, ExecutionPlan, InstalledPackage, MaintenancePlan,
        NativeCommand, OperationKind, OperationStatus, PackageCandidate, PackageDomain,
        PackageInfo, PrivilegeRequirement,
    },
    execution::ProcessRunner,
    platform::{OperatingSystem, PlatformContext},
};
use std::path::Path;

pub struct AurBackend {
    helper_available: bool,
}
impl AurBackend {
    pub fn detect() -> Self {
        Self {
            helper_available: ["yay", "paru"]
                .iter()
                .any(|helper| crate::discovery::path::find_executable(helper).is_some()),
        }
    }
}
const INVENTORY_CAPS: &[Capability] = &[Capability::List, Capability::Remove];
const CAPS: &[Capability] = &[
    Capability::Search,
    Capability::Install,
    Capability::Remove,
    Capability::List,
    Capability::Info,
    Capability::Upgrade,
];
const REQUIRED: &[CommandRequirement] = &[CommandRequirement {
    key: "pacman",
    alternatives: &["pacman"],
}];
const OPTIONAL: &[CommandRequirement] = &[
    CommandRequirement {
        key: "yay",
        alternatives: &["yay"],
    },
    CommandRequirement {
        key: "paru",
        alternatives: &["paru"],
    },
];

impl Backend for AurBackend {
    fn id(&self) -> &'static str {
        "aur"
    }
    fn display_name(&self) -> &'static str {
        "AUR"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["aur", "yay", "paru"]
    }
    fn category(&self) -> BackendCategory {
        BackendCategory::System
    }
    fn capabilities(&self) -> &'static [Capability] {
        if self.helper_available {
            CAPS
        } else {
            INVENTORY_CAPS
        }
    }
    fn command_requirements(&self) -> &'static [CommandRequirement] {
        REQUIRED
    }
    fn optional_command_requirements(&self) -> &'static [CommandRequirement] {
        OPTIONAL
    }
    fn platform_incompatibility(&self, platform: &PlatformContext) -> Option<String> {
        (platform.os != OperatingSystem::Linux)
            .then(|| "AUR requires Arch Linux tooling and a normal build user".to_owned())
    }
    fn maintenance_dependencies(&self, capability: Capability) -> &'static [&'static str] {
        if capability == Capability::Upgrade {
            &["pacman"]
        } else {
            &[]
        }
    }
    fn maintenance_requires_terminal(&self) -> bool {
        true
    }
    fn operation_not_applicable_message(
        &self,
        capability: Capability,
        _: crate::backends::BackendOperationCapability,
    ) -> String {
        if capability == Capability::Update {
            "AUR helpers query the AUR during search/upgrade; use allp upgrade to upgrade packages"
                .to_owned()
        } else if !self.helper_available {
            "AUR inventory is available, but upgrades require yay or paru; install a helper manually".to_owned()
        } else {
            format!("{} is not supported", capability.label())
        }
    }
    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<BackendSearchReport> {
        let (_, helper) = helper(commands, None)?;
        let command = NativeCommand::new(helper).args(["-Ssa", "--color", "never", "--", query]);
        let output =
            runner.capture_with_privilege(&command, PrivilegeRequirement::OriginalUserRequired)?;
        if !output.success {
            if output.code == Some(1)
                && output.stdout.trim().is_empty()
                && output.stderr.trim().is_empty()
            {
                return Ok(BackendSearchReport::default());
            }
            return Err(AllpError::CommandFailed {
                backend: self.display_name().to_owned(),
                command: crate::execution::render_native_command(&command),
                code: output.code,
                stderr: output.stderr,
            });
        }
        let mut report = super::pacman::parse_pacman_search(self, &output.stdout, query);
        let original_count = report.candidates.len();
        report
            .candidates
            .retain(|candidate| candidate.source.as_deref() == Some("aur"));
        if report.candidates.len() != original_count {
            report.issues.push(crate::domain::BackendSearchIssue { kind: crate::domain::BackendSearchIssueKind::UnrecognizedOutput,
                stage: Some("AUR-only search".to_owned()), message: "Helper returned repository packages during an AUR-only search; those results were rejected".to_owned() });
        }
        for candidate in &mut report.candidates {
            candidate.installers = ["yay", "paru"]
                .into_iter()
                .filter(|key| commands.contains_key(*key))
                .map(str::to_owned)
                .collect();
            candidate.artifact_kind = "AUR build recipe".to_owned();
            candidate.scope = Some("system (built as user)".to_owned());
            candidate.identity = PackageCandidate::infer_identity(
                match_kind(&candidate.package_id, query),
                PackageDomain::System,
                "AUR build recipe",
            );
        }
        Ok(report)
    }
    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        foreign_packages(commands, runner)
    }
    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package: &str,
    ) -> AllpResult<PackageInfo> {
        let output = self.raw_info(commands, runner, package)?;
        let fields = parse_key_value_lines(&output);
        if fields.get("Name").map(String::as_str) != Some(package) {
            return Err(AllpError::Parse {
                backend: "AUR".to_owned(),
                message: "helper returned unrecognized package information".to_owned(),
            });
        }
        Ok(PackageInfo {
            backend_id: "aur".to_owned(),
            backend_name: "AUR".to_owned(),
            category: self.category(),
            domain: PackageDomain::System,
            package_id: package.to_owned(),
            display_name: package.to_owned(),
            version: fields.get("Version").cloned(),
            description: fields.get("Description").cloned(),
            source: Some("AUR (community build recipes)".to_owned()),
            scope: Some("system (built as user)".to_owned()),
            artifact_kind: Some("AUR build recipe".to_owned()),
            installed: None,
            extra: fields
                .into_iter()
                .filter(|(key, _)| !matches!(key.as_str(), "Name" | "Version" | "Description"))
                .collect(),
        })
    }
    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package: &str,
    ) -> AllpResult<String> {
        let (_, helper) = helper(commands, None)?;
        capture_checked_with_privilege(
            self,
            runner,
            NativeCommand::new(helper).args(["-Sia", "--color", "never", "--", package]),
            PrivilegeRequirement::OriginalUserRequired,
        )
    }
    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        crate::operations::validate_package_id(&candidate.package_id)?;
        let requested = candidate.installers.first().map(String::as_str);
        let (key, helper) = helper(commands, requested)?;
        let mut plan = plan(
            helper,
            OperationKind::Install,
            "Build and install AUR package",
            Some(&candidate.package_id),
            &["-Sa", "--", &candidate.package_id],
        );
        plan.details.push(("Installer".to_owned(), key.to_owned()));
        Ok(plan)
    }
    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        crate::operations::validate_package_id(&package.package_id)?;
        let pacman = command_path(self, commands, "pacman")?;
        let mut plan = plan(
            pacman,
            OperationKind::Remove,
            "Remove foreign package through Pacman",
            Some(&package.package_id),
            &["-R", "--", &package.package_id],
        );
        plan.privilege = PrivilegeRequirement::RootRequired;
        plan.requires_root = true;
        Ok(plan)
    }
    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        selector: Option<&str>,
        _: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let requested = selector.filter(|s| matches!(*s, "yay" | "paru"));
        let (key, helper) = helper(commands, requested)?;
        let foreign = foreign_packages(commands, runner)?;
        if foreign.is_empty() {
            return Ok(MaintenancePlan {
                plans: Vec::new(),
                records: vec![BackendOperationRecord {
                    backend_id: "aur".to_owned(),
                    backend_name: "AUR".to_owned(),
                    action: None,
                    command: None,
                    status: OperationStatus::UpToDate,
                    message: Some("No foreign packages are installed".to_owned()),
                    privilege_status: None,
                }],
            });
        }
        let mut plan = plan(
            helper,
            OperationKind::Upgrade,
            "Upgrade AUR packages only",
            None,
            &["-Sua"],
        );
        plan.details.push(("Installer".to_owned(), key.to_owned()));
        plan.details
            .push(("Foreign packages".to_owned(), foreign.len().to_string()));
        Ok(MaintenancePlan::from_plans(vec![plan]))
    }
}

fn helper<'a>(
    commands: &'a CommandMap,
    requested: Option<&str>,
) -> AllpResult<(&'a str, &'a Path)> {
    if let Some(requested) = requested {
        if !matches!(requested, "aur" | "AUR") {
            return commands
                .get_key_value(requested)
                .map(|(key, path)| (key.as_str(), path.as_path()))
                .ok_or_else(|| {
                    AllpError::BackendNotDetected(format!(
                        "AUR helper '{requested}' is not installed; install yay or paru manually"
                    ))
                });
        }
    }
    for key in ["yay", "paru"] {
        if let Some(path) = commands.get(key) {
            return Ok((key, path));
        }
    }
    Err(AllpError::BackendNotDetected(
        "AUR requires an installed yay or paru helper".to_owned(),
    ))
}

fn plan(
    helper: &Path,
    operation: OperationKind,
    action: &str,
    package: Option<&str>,
    args: &[&str],
) -> ExecutionPlan {
    ExecutionPlan { backend_id: "aur".to_owned(), backend_name: "AUR".to_owned(), operation, action: action.to_owned(), package_id: package.map(str::to_owned), source: Some("AUR (community build recipes)".to_owned()), scope: Some("system (built as user)".to_owned()),
        details: vec![("Review".to_owned(), "Helper PKGBUILD review and native prompts remain enabled; --yes only confirms Allp's plan.".to_owned())],
        command: NativeCommand::new(helper).args(args.iter().copied()), privilege: PrivilegeRequirement::OriginalUserRequired, requires_root: false, interactive: true }
}

/// Foreign means absent from configured sync databases; it may be AUR or a
/// locally built package. Preserve that uncertainty in inventory and reports.
pub(super) fn foreign_packages(
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
) -> AllpResult<Vec<InstalledPackage>> {
    let pacman = command_path(
        &AurBackend {
            helper_available: false,
        },
        commands,
        "pacman",
    )?;
    let command = NativeCommand::new(pacman).arg("-Qm");
    let output = runner.capture(&command)?;
    if !output.success {
        if output.code == Some(1)
            && output.stdout.trim().is_empty()
            && output.stderr.trim().is_empty()
        {
            return Ok(Vec::new());
        }
        return Err(AllpError::CommandFailed {
            backend: "AUR inventory".to_owned(),
            command: crate::execution::render_native_command(&command),
            code: output.code,
            stderr: output.stderr,
        });
    }
    let mut packages = Vec::new();
    for line in output.stdout.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 2 || crate::operations::validate_package_id(fields[0]).is_err() {
            return Err(AllpError::Parse {
                backend: "AUR inventory".to_owned(),
                message: "pacman -Qm returned an invalid installed-package inventory".to_owned(),
            });
        }
        packages.push(InstalledPackage {
            installer: None,
            backend_id: "aur".to_owned(),
            backend_name: "AUR".to_owned(),
            category: BackendCategory::System,
            domain: PackageDomain::System,
            package_id: fields[0].to_owned(),
            display_name: fields[0].to_owned(),
            version: Some(fields[1].to_owned()),
            description: Some(
                "Foreign package: may be AUR or locally built; verify its source before restoring"
                    .to_owned(),
            ),
            source: Some("Pacman foreign database".to_owned()),
            scope: Some("system".to_owned()),
        });
    }
    Ok(packages)
}
