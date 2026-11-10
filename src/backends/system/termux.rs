use super::{apt::AptBackend, pacman::PacmanBackend};
use crate::{
    backends::{
        contract::InstallPreflight, Backend, BackendOperationCapability, CommandMap,
        CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, BackendOperationRecord, BackendSearchReport,
        Capability, DeveloperTarget, ExecutionPlan, InstalledPackage, MaintenancePlan,
        OperationKind, PackageCandidate, PackageInfo, PrivilegeRequirement,
        RuntimePrivilegeContext,
    },
    execution::{ProcessRunner, ProcessStatus},
    platform::{termux_prefix, PlatformContext},
};
use std::{env, fs};

const SCOPE: &str = "Termux app";
const ROOT_ERROR: &str =
    "Termux package management requires the app user. Exit the root shell and run Allp again.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Manager {
    Apt,
    Pacman,
    Unsupported,
}

pub struct TermuxBackend {
    manager: Manager,
}

impl TermuxBackend {
    pub fn detect() -> Self {
        let declared = env::var("TERMUX_APP_PACKAGE_MANAGER")
            .ok()
            .filter(|value| !value.is_empty())
            .or_else(|| {
                env::var("TERMUX_MAIN_PACKAGE_FORMAT")
                    .ok()
                    .filter(|value| !value.is_empty())
            });
        let manager = match declared.as_deref() {
            Some("apt" | "debian") => Manager::Apt,
            Some("pacman") => Manager::Pacman,
            Some(_) => Manager::Unsupported,
            None if termux_prefix().is_some_and(|prefix| {
                !prefix.join("bin/apt-get").is_file() && prefix.join("bin/pacman").is_file()
            }) =>
            {
                Manager::Pacman
            }
            None => Manager::Apt,
        };
        Self { manager }
    }

    fn native(&self) -> &dyn Backend {
        match self.manager {
            Manager::Pacman => &PacmanBackend,
            Manager::Apt | Manager::Unsupported => &AptBackend,
        }
    }

    fn termux_plan(&self, mut plan: ExecutionPlan) -> ExecutionPlan {
        plan.backend_id = self.id().to_owned();
        plan.backend_name = self.display_name().to_owned();
        plan.action = match plan.operation {
            OperationKind::Install => "Install package in Termux",
            OperationKind::Bootstrap => "Prepare package management in Termux",
            OperationKind::Remove => "Remove package from Termux",
            OperationKind::Update => "Refresh Termux package metadata",
            OperationKind::Upgrade => "Upgrade installed Termux packages",
        }
        .to_owned();
        plan.scope = Some(SCOPE.to_owned());
        plan.privilege = PrivilegeRequirement::OriginalUserRequired;
        plan.requires_root = false;
        if self.manager == Manager::Apt && plan.operation == OperationKind::Upgrade {
            // Termux is rolling release; its pkg wrapper runs apt full-upgrade.
            for argument in &mut plan.command.args {
                if argument == "upgrade" {
                    *argument = "full-upgrade".into();
                }
            }
            plan.command.args.retain(|arg| arg != "--with-new-pkgs");
        }
        plan
    }

    fn termux_maintenance(&self, mut maintenance: MaintenancePlan) -> MaintenancePlan {
        maintenance.plans = maintenance
            .plans
            .into_iter()
            .map(|plan| self.termux_plan(plan))
            .collect();
        maintenance
    }
}

impl Backend for TermuxBackend {
    fn id(&self) -> &'static str {
        "termux"
    }

    fn display_name(&self) -> &'static str {
        "Termux"
    }

    fn category(&self) -> BackendCategory {
        BackendCategory::System
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["pkg"]
    }

    fn capabilities(&self) -> &'static [Capability] {
        self.native().capabilities()
    }

    fn command_requirements(&self) -> &'static [CommandRequirement] {
        self.native().command_requirements()
    }

    fn platform_incompatibility(&self, platform: &PlatformContext) -> Option<String> {
        if !platform.is_termux() {
            Some("Termux package management is available inside the Android Termux app".to_owned())
        } else if platform.is_root {
            Some(ROOT_ERROR.to_owned())
        } else if self.manager == Manager::Unsupported {
            Some("Termux declares a package manager other than APT or Pacman".to_owned())
        } else {
            None
        }
    }

    fn probe(&self, commands: &CommandMap, _runner: &dyn ProcessRunner) -> AllpResult<()> {
        let prefix = termux_prefix().ok_or_else(|| {
            AllpError::InvalidInput("Termux package prefix was not found".to_owned())
        })?;
        let bin = fs::canonicalize(prefix.join("bin"))?;
        for command in commands.values() {
            if !fs::canonicalize(command)?.starts_with(&bin) {
                return Err(AllpError::InvalidInput(format!(
                    "Termux package command {} is outside the app prefix {}",
                    command.display(),
                    bin.display()
                )));
            }
        }
        Ok(())
    }

    fn operation_capability(&self, capability: Capability) -> BackendOperationCapability {
        if self.manager == Manager::Pacman && capability == Capability::Upgrade {
            BackendOperationCapability::CombinedRefreshAndUpgrade
        } else {
            self.native().operation_capability(capability)
        }
    }

    fn requires_metadata_refresh_before_upgrade(&self) -> bool {
        self.native().requires_metadata_refresh_before_upgrade()
    }

    fn authorize_noninteractive(&self, plan: &mut ExecutionPlan) {
        self.native().authorize_noninteractive(plan);
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<BackendSearchReport> {
        let mut report = self.native().search(commands, runner, query)?;
        for candidate in &mut report.candidates {
            candidate.backend_id = self.id().to_owned();
            candidate.backend_name = self.display_name().to_owned();
            candidate.installers = vec![self.display_name().to_owned()];
            candidate.scope = Some(SCOPE.to_owned());
        }
        Ok(report)
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let mut packages = self.native().list_installed(commands, runner)?;
        for package in &mut packages {
            package.backend_id = self.id().to_owned();
            package.backend_name = self.display_name().to_owned();
            package.scope = Some(SCOPE.to_owned());
        }
        Ok(packages)
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        let mut info = self.native().info(commands, runner, package_id)?;
        info.backend_id = self.id().to_owned();
        info.backend_name = self.display_name().to_owned();
        info.scope = Some(SCOPE.to_owned());
        Ok(info)
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        self.native().raw_info(commands, runner, package_id)
    }

    fn preflight_plan_install(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        candidate: &PackageCandidate,
    ) -> AllpResult<InstallPreflight> {
        self.native()
            .preflight_plan_install(commands, runner, candidate)
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        self.native()
            .plan_install(commands, candidate)
            .map(|plan| self.termux_plan(plan))
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        self.native()
            .plan_remove(commands, package)
            .map(|plan| self.termux_plan(plan))
    }

    fn plan_update(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        selector: Option<&str>,
        target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        self.native()
            .plan_update(commands, runner, selector, target)
            .map(|plan| self.termux_maintenance(plan))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        selector: Option<&str>,
        target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        self.native()
            .plan_upgrade(commands, runner, selector, target)
            .map(|plan| self.termux_maintenance(plan))
    }

    fn validate_before_execution(
        &self,
        _plan: &ExecutionPlan,
        _runner: &dyn ProcessRunner,
        context: &RuntimePrivilegeContext,
    ) -> AllpResult<()> {
        if context.is_root() {
            return Err(AllpError::InvalidInput(ROOT_ERROR.to_owned()));
        }
        Ok(())
    }

    fn classify_execution_failure(
        &self,
        plan: &ExecutionPlan,
        status: &ProcessStatus,
        command: &str,
    ) -> Option<AllpError> {
        self.native()
            .classify_execution_failure(plan, status, command)
    }

    fn classify_execution_success(
        &self,
        plan: &ExecutionPlan,
        status: &ProcessStatus,
        command: &str,
    ) -> Option<Vec<BackendOperationRecord>> {
        self.native()
            .classify_execution_success(plan, status, command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::NativeCommand,
        execution::{render_execution_plan_with_context, CommandOutput},
        platform::{Distribution, OperatingSystem},
    };
    use std::path::PathBuf;

    struct TermuxRunner;

    impl ProcessRunner for TermuxRunner {
        fn capture(&self, command: &NativeCommand) -> AllpResult<CommandOutput> {
            let output = match command.args.first().and_then(|arg| arg.to_str()) {
                Some("search") => "tree - display directories as trees\n",
                Some("policy") => "tree:\n  Candidate: 2.2.1\n",
                Some("-W") => "installed\ttree\t2.2.1\n",
                Some("show") => "Package: tree\nVersion: 2.2.1\nDescription: list files in trees\n",
                _ => panic!("unexpected native command {command:?}"),
            };
            Ok(CommandOutput {
                success: true,
                code: Some(0),
                signal: None,
                duration: std::time::Duration::ZERO,
                stdout: output.to_owned(),
                stderr: String::new(),
            })
        }

        fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
            panic!("planning tests must not execute package commands")
        }
    }

    fn commands() -> CommandMap {
        ["apt-get", "apt-cache", "dpkg-query", "pacman"]
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    PathBuf::from(format!("/termux/bin/{name}")),
                )
            })
            .collect()
    }

    #[test]
    fn termux_is_available_only_to_the_android_app_user() {
        let backend = TermuxBackend {
            manager: Manager::Apt,
        };
        let mut platform = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        platform.os = OperatingSystem::Android;
        platform.distribution = None;
        assert!(backend.platform_incompatibility(&platform).is_some());
        platform.distribution = Some(Distribution {
            id: "termux".to_owned(),
            name: "Termux".to_owned(),
            version: None,
        });
        assert!(backend.platform_incompatibility(&platform).is_none());
        assert!(AptBackend.platform_incompatibility(&platform).is_some());
        assert!(PacmanBackend.platform_incompatibility(&platform).is_some());
        platform.is_root = true;
        assert_eq!(
            backend.platform_incompatibility(&platform).as_deref(),
            Some(ROOT_ERROR)
        );
    }

    #[test]
    fn termux_apt_reads_and_plans_without_root() {
        let backend = TermuxBackend {
            manager: Manager::Apt,
        };
        let commands = commands();
        let results = backend.search(&commands, &TermuxRunner, "tree").unwrap();
        assert_eq!(results.candidates.len(), 1);
        assert_eq!(results.candidates[0].backend_id, "termux");
        assert_eq!(results.candidates[0].version.as_deref(), Some("2.2.1"));
        let inventory = backend.list_installed(&commands, &TermuxRunner).unwrap();
        assert_eq!(inventory[0].scope.as_deref(), Some(SCOPE));
        let info = backend.info(&commands, &TermuxRunner, "tree").unwrap();
        assert_eq!(info.backend_id, "termux");
        let install = backend
            .plan_install(&commands, &results.candidates[0])
            .unwrap();
        let remove = backend.plan_remove(&commands, &inventory[0]).unwrap();
        let refresh = backend
            .plan_update(&commands, &TermuxRunner, None, None)
            .unwrap()
            .plans
            .remove(0);
        let upgrade = backend
            .plan_upgrade(&commands, &TermuxRunner, None, None)
            .unwrap()
            .plans
            .remove(0);
        for (plan, expected) in [
            (install, "install -- tree"),
            (remove, "remove -- tree"),
            (refresh, "update -o APT::Update::Error-Mode=any"),
            (upgrade, "full-upgrade"),
        ] {
            assert_eq!(plan.privilege, PrivilegeRequirement::OriginalUserRequired);
            assert!(!plan.requires_root);
            let render =
                render_execution_plan_with_context(&plan, &RuntimePrivilegeContext::NormalUser);
            assert!(render.starts_with("/termux/bin/apt-get "));
            assert!(render.ends_with(expected), "{render}");
            assert!(backend
                .validate_before_execution(
                    &plan,
                    &TermuxRunner,
                    &RuntimePrivilegeContext::RootDirect
                )
                .is_err());
        }
        assert!(backend.requires_metadata_refresh_before_upgrade());
    }

    #[test]
    fn termux_pacman_sync_and_upgrade_is_one_operation() {
        let backend = TermuxBackend {
            manager: Manager::Pacman,
        };
        let commands = commands();
        assert!(!backend.has_capability(Capability::Update));
        assert_eq!(
            backend.operation_capability(Capability::Upgrade),
            BackendOperationCapability::CombinedRefreshAndUpgrade
        );
        let plan = backend
            .plan_upgrade(&commands, &TermuxRunner, None, None)
            .unwrap()
            .plans
            .remove(0);
        assert_eq!(
            render_execution_plan_with_context(&plan, &RuntimePrivilegeContext::NormalUser),
            "/termux/bin/pacman -Syu"
        );
        assert!(!plan.requires_root);
    }
}
