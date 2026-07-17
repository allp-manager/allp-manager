use crate::{
    domain::{
        AllpError, AllpResult, BackendCategory, BackendOperationRecord, Capability,
        DeveloperTarget, ExecutionPlan, InstalledPackage, MaintenancePlan, NativeCommand,
        PackageCandidate, PackageDomain, PackageInfo, RuntimePrivilegeContext,
    },
    execution::ProcessRunner,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub type CommandMap = BTreeMap<String, PathBuf>;

#[derive(Debug, Clone)]
pub enum InstallPreflight {
    Continue,
    UseCandidate {
        candidate: Box<PackageCandidate>,
        warnings: Vec<InstallPreflightWarning>,
    },
    AlreadyInstalled {
        package_id: String,
        installed_version: Option<String>,
        candidate_version: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct InstallPreflightWarning {
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct InstallPreflightStatus {
    pub stage: String,
    pub command: NativeCommand,
    pub display_command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallPreflightRecovery {
    RetryValidation,
    RetrySearch,
    TryAlternativeInstallers,
    Cancelled,
}

const SYSTEM_DOMAINS: &[PackageDomain] = &[PackageDomain::System];
const UNIVERSAL_DOMAINS: &[PackageDomain] = &[PackageDomain::Universal];
const DEVELOPMENT_DOMAINS: &[PackageDomain] = &[
    PackageDomain::Homebrew,
    PackageDomain::Python,
    PackageDomain::Node,
];

#[derive(Debug, Clone, Copy)]
pub struct CommandRequirement {
    pub key: &'static str,
    pub alternatives: &'static [&'static str],
}

pub trait Backend: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn category(&self) -> BackendCategory;
    fn capabilities(&self) -> &'static [Capability];
    fn command_requirements(&self) -> &'static [CommandRequirement];
    fn optional_command_requirements(&self) -> &'static [CommandRequirement] {
        &[]
    }
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }
    fn package_domains(&self) -> &'static [PackageDomain] {
        match self.category() {
            BackendCategory::System => SYSTEM_DOMAINS,
            BackendCategory::Universal => UNIVERSAL_DOMAINS,
            BackendCategory::Development => DEVELOPMENT_DOMAINS,
        }
    }

    fn has_capability(&self, capability: Capability) -> bool {
        self.capabilities().contains(&capability)
    }

    fn probe(&self, _commands: &CommandMap, _runner: &dyn ProcessRunner) -> AllpResult<()> {
        Ok(())
    }

    fn search(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        Err(self.unsupported("search"))
    }

    fn plan_search_prerequisite(
        &self,
        _commands: &CommandMap,
    ) -> AllpResult<Option<ExecutionPlan>> {
        Ok(None)
    }

    fn verify_search_prerequisite(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
    ) -> AllpResult<bool> {
        Ok(false)
    }

    fn list_installed(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        Err(self.unsupported("list"))
    }

    fn info(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _package_id: &str,
    ) -> AllpResult<PackageInfo> {
        Err(self.unsupported("info"))
    }

    fn raw_info(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _package_id: &str,
    ) -> AllpResult<String> {
        Err(self.unsupported("raw info"))
    }

    fn preflight_plan_install(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _candidate: &PackageCandidate,
    ) -> AllpResult<InstallPreflight> {
        Ok(InstallPreflight::Continue)
    }

    fn install_preflight_status(
        &self,
        _commands: &CommandMap,
        _candidate: &PackageCandidate,
    ) -> AllpResult<Option<InstallPreflightStatus>> {
        Ok(None)
    }

    fn recover_install_preflight_failure(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _candidate: &PackageCandidate,
        error: AllpError,
        _no_interactive: bool,
    ) -> AllpResult<InstallPreflightRecovery> {
        Err(error)
    }

    fn plan_install(
        &self,
        _commands: &CommandMap,
        _candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        Err(self.unsupported("install"))
    }

    fn preflight_install(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _candidate: &PackageCandidate,
        _context: &RuntimePrivilegeContext,
    ) -> AllpResult<()> {
        Ok(())
    }

    fn classify_execution_failure(
        &self,
        _plan: &ExecutionPlan,
        _status: &crate::execution::ProcessStatus,
        _command: &str,
    ) -> Option<AllpError> {
        None
    }

    fn classify_execution_success(
        &self,
        _plan: &ExecutionPlan,
        _status: &crate::execution::ProcessStatus,
        _command: &str,
    ) -> Option<Vec<BackendOperationRecord>> {
        None
    }

    fn plan_remove(
        &self,
        _commands: &CommandMap,
        _package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        Err(self.unsupported("remove"))
    }

    fn plan_update(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        Err(self.unsupported("update"))
    }

    fn plan_upgrade(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        Err(self.unsupported("upgrade"))
    }

    fn unsupported(&self, operation: &str) -> AllpError {
        AllpError::UnsupportedOperation {
            backend: self.display_name().to_owned(),
            operation: operation.to_owned(),
        }
    }
}

pub fn backend_matches_filter(backend: &dyn Backend, filter: &str) -> bool {
    backend.id().eq_ignore_ascii_case(filter)
        || backend
            .aliases()
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(filter))
}

pub fn command_path<'a>(
    backend: &dyn Backend,
    commands: &'a CommandMap,
    key: &str,
) -> AllpResult<&'a Path> {
    commands
        .get(key)
        .map(PathBuf::as_path)
        .ok_or_else(|| AllpError::BackendNotDetected(format!("{} ({key})", backend.id())))
}
