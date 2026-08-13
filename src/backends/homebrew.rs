use crate::{
    backends::{
        contract::{command_path, BackendOperationCapability},
        util::{capture_checked_with_privilege, match_kind},
        Backend, CommandMap, CommandRequirement,
    },
    discovery::revalidate_homebrew_executable,
    domain::{
        AllpError, AllpResult, BackendCategory, BackendOperationRecord, Capability,
        DeveloperTarget, ExecutionPlan, InstalledPackage, MaintenancePlan, NativeCommand,
        OperationKind, OperationStatus, PackageCandidate, PackageDomain, PackageInfo,
        PrivilegeRequirement, RuntimePrivilegeContext,
    },
    execution::{CommandOutput, ProcessRunner},
};
use serde::Deserialize;

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
    alternatives: &["brew"],
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

    fn operation_capability(&self, capability: Capability) -> BackendOperationCapability {
        match capability {
            Capability::Update => BackendOperationCapability::MetadataRefresh,
            Capability::Upgrade => BackendOperationCapability::InstalledPackageUpgrade,
            _ => BackendOperationCapability::Unsupported,
        }
    }

    fn requires_metadata_refresh_before_upgrade(&self) -> bool {
        true
    }

    fn plan_upgrade_after_metadata_refresh(&self) -> bool {
        true
    }

    fn authorize_noninteractive(&self, plan: &mut ExecutionPlan) {
        if plan.operation == OperationKind::Upgrade
            && plan.interactive
            && !plan
                .command
                .args
                .iter()
                .any(|argument| argument == "--no-ask")
        {
            plan.command.args.push("--no-ask".into());
            plan.interactive = false;
        }
    }

    fn validate_before_execution(
        &self,
        plan: &ExecutionPlan,
        runner: &dyn ProcessRunner,
        context: &RuntimePrivilegeContext,
    ) -> AllpResult<()> {
        if !matches!(
            plan.operation,
            OperationKind::Install
                | OperationKind::Remove
                | OperationKind::Update
                | OperationKind::Upgrade
        ) {
            return Ok(());
        }
        revalidate_homebrew_executable(&plan.command.program, context, runner)
            .map(|_| ())
            .map_err(|problem| {
                AllpError::InvalidInput(format!(
                    "Homebrew changed or became unusable after planning; refusing to execute {}: {}",
                    plan.command.program.display(),
                    problem.message
                ))
            })
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
            runner.capture_with_privilege(
                &homebrew_command(brew).args(["search", "--formula", query]),
                PrivilegeRequirement::OriginalUserRequired,
            ),
            query,
            "Homebrew formulae",
            "Homebrew formula",
            &mut candidates,
        );
        append_search(
            self,
            runner.capture_with_privilege(
                &homebrew_command(brew).args(["search", "--cask", query]),
                PrivilegeRequirement::OriginalUserRequired,
            ),
            query,
            "Homebrew casks",
            "Homebrew cask",
            &mut candidates,
        );

        if candidates.is_empty() {
            let output = capture_checked_with_privilege(
                self,
                runner,
                homebrew_command(brew).args(["search", query]),
                PrivilegeRequirement::OriginalUserRequired,
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
            &capture_checked_with_privilege(
                self,
                runner,
                homebrew_command(brew).args(["list", "--formula", "--versions"]),
                PrivilegeRequirement::OriginalUserRequired,
            )
            .unwrap_or_default(),
            "Homebrew formulae",
            "formula",
            &mut packages,
        );
        append_installed(
            self,
            &capture_checked_with_privilege(
                self,
                runner,
                homebrew_command(brew).args(["list", "--cask", "--versions"]),
                PrivilegeRequirement::OriginalUserRequired,
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
        let output = capture_checked_with_privilege(
            self,
            runner,
            homebrew_command(brew).args(["info", package_id]),
            PrivilegeRequirement::OriginalUserRequired,
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
        capture_checked_with_privilege(
            self,
            runner,
            homebrew_command(brew).args(["info", package_id]),
            PrivilegeRequirement::OriginalUserRequired,
        )
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let brew = command_path(self, commands, "brew")?;
        let mut command = homebrew_command(brew).arg("install");
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
        let mut command = homebrew_command(brew).arg("uninstall");
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
        runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let brew = command_path(self, commands, "brew")?;
        let operation = if supports_update_if_needed(brew, runner) {
            "update-if-needed"
        } else {
            "update"
        };
        Ok(MaintenancePlan::from_plans(vec![plan(
            self,
            OperationKind::Update,
            "Refresh Homebrew formula and cask metadata",
            None,
            Some("Homebrew".to_owned()),
            homebrew_command(brew).arg(operation),
        )]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let brew = command_path(self, commands, "brew")?;
        let command = no_auto_update(homebrew_command(brew).args(["outdated", "--json=v2"]));
        let output =
            runner.capture_with_privilege(&command, PrivilegeRequirement::OriginalUserRequired)?;
        if !output.success {
            return Err(AllpError::CommandFailed {
                backend: self.display_name().to_owned(),
                command: "brew outdated --json=v2".to_owned(),
                code: output.code,
                stderr: output.stderr,
            });
        }
        let outdated = parse_outdated(&output.stdout)?;
        if outdated.is_empty() {
            return Ok(MaintenancePlan {
                plans: Vec::new(),
                records: vec![MaintenancePlan::record(
                    self.id(),
                    self.display_name(),
                    OperationStatus::UpToDate,
                    "no outdated formulae or casks",
                )],
            });
        }
        let mut upgrade = plan(
            self,
            OperationKind::Upgrade,
            "Upgrade installed Homebrew packages",
            None,
            Some("Homebrew".to_owned()),
            no_auto_update(homebrew_command(brew).arg("upgrade")),
        );
        upgrade.details.push((
            "Outdated formulae".to_owned(),
            outdated.formula_names().join(", "),
        ));
        upgrade.details.push((
            "Outdated casks".to_owned(),
            outdated.cask_names().join(", "),
        ));
        upgrade
            .details
            .push(("Outdated total".to_owned(), outdated.total().to_string()));
        Ok(MaintenancePlan::from_plans(vec![upgrade]))
    }

    fn classify_execution_failure(
        &self,
        _plan: &ExecutionPlan,
        status: &crate::execution::ProcessStatus,
        command: &str,
    ) -> Option<AllpError> {
        is_busy_output(&status.stderr).then(|| AllpError::BackendBusy {
            backend: self.display_name().to_owned(),
            command: command.to_owned(),
            code: status.code,
            lock_path: None,
            holder_pid: None,
            holder_process: Some("another Homebrew update".to_owned()),
        })
    }

    fn classify_execution_success(
        &self,
        plan: &ExecutionPlan,
        status: &crate::execution::ProcessStatus,
        _command: &str,
    ) -> Option<Vec<BackendOperationRecord>> {
        if plan.operation == OperationKind::Update {
            let changed = !(status.stdout.trim().is_empty() && status.stderr.trim().is_empty())
                && !status.stdout.contains("Already up-to-date");
            return Some(vec![BackendOperationRecord {
                backend_id: plan.backend_id.clone(),
                backend_name: plan.backend_name.clone(),
                action: None,
                command: None,
                status: if changed {
                    OperationStatus::Updated
                } else {
                    OperationStatus::UpToDate
                },
                message: Some(if changed {
                    "Homebrew metadata refreshed".to_owned()
                } else {
                    "Homebrew metadata already current".to_owned()
                }),
                privilege_status: None,
            }]);
        }
        if plan.operation == OperationKind::Upgrade {
            let count = plan
                .details
                .iter()
                .find(|(key, _)| key == "Outdated total")
                .and_then(|(_, value)| value.parse::<usize>().ok())?;
            return Some(vec![BackendOperationRecord {
                backend_id: plan.backend_id.clone(),
                backend_name: plan.backend_name.clone(),
                action: None,
                command: None,
                status: OperationStatus::Updated,
                message: Some(format!(
                    "native upgrade completed for {count} outdated package(s); post-upgrade state requires verification"
                )),
                privilege_status: None,
            }]);
        }
        None
    }

    fn post_execution_verification(
        &self,
        plan: &ExecutionPlan,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Option<BackendOperationRecord>> {
        if plan.operation == OperationKind::Upgrade {
            verify_upgrade(plan, runner).map(Some)
        } else {
            Ok(None)
        }
    }
}

fn no_auto_update(command: NativeCommand) -> NativeCommand {
    command.env("HOMEBREW_NO_AUTO_UPDATE", "1")
}

fn homebrew_command(brew: &std::path::Path) -> NativeCommand {
    NativeCommand::new(brew)
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_UPDATE_REPORT_NEW", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
}

fn supports_update_if_needed(brew: &std::path::Path, runner: &dyn ProcessRunner) -> bool {
    runner
        .capture_with_privilege(
            &homebrew_command(brew).args(["help", "update-if-needed"]),
            PrivilegeRequirement::OriginalUserRequired,
        )
        .is_ok_and(|output| output.success)
}

fn is_busy_output(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("another `brew update` process is already running")
        || lower.contains("another brew update process is already running")
}

#[derive(Debug, Deserialize)]
struct HomebrewOutdated {
    #[serde(default)]
    formulae: Vec<HomebrewOutdatedFormula>,
    #[serde(default)]
    casks: Vec<HomebrewOutdatedCask>,
}

#[derive(Debug, Deserialize)]
struct HomebrewOutdatedFormula {
    name: String,
    #[serde(default)]
    pinned: bool,
}

#[derive(Debug, Deserialize)]
struct HomebrewOutdatedCask {
    name: String,
}

impl HomebrewOutdated {
    fn is_empty(&self) -> bool {
        self.formulae.is_empty() && self.casks.is_empty()
    }

    fn total(&self) -> usize {
        self.formulae.len() + self.casks.len()
    }

    fn formula_names(&self) -> Vec<String> {
        self.formulae
            .iter()
            .map(|formula| {
                if formula.pinned {
                    format!("{} (pinned)", formula.name)
                } else {
                    formula.name.clone()
                }
            })
            .collect()
    }

    fn cask_names(&self) -> Vec<String> {
        self.casks.iter().map(|cask| cask.name.clone()).collect()
    }
}

fn parse_outdated(output: &str) -> AllpResult<HomebrewOutdated> {
    serde_json::from_str(output).map_err(|error| AllpError::MetadataParseFailed {
        backend: "Homebrew".to_owned(),
        message: format!("invalid `brew outdated --json=v2` output: {error}"),
    })
}

fn verify_upgrade(
    plan: &ExecutionPlan,
    runner: &dyn ProcessRunner,
) -> AllpResult<BackendOperationRecord> {
    let before = plan
        .details
        .iter()
        .find(|(key, _)| key == "Outdated total")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .ok_or_else(|| AllpError::Parse {
            backend: "Homebrew".to_owned(),
            message: "upgrade plan did not preserve the pre-upgrade outdated count".to_owned(),
        })?;
    let command =
        no_auto_update(homebrew_command(&plan.command.program).args(["outdated", "--json=v2"]));
    let output =
        runner.capture_with_privilege(&command, PrivilegeRequirement::OriginalUserRequired)?;
    if !output.success {
        return Err(AllpError::CommandFailed {
            backend: "Homebrew".to_owned(),
            command: "brew outdated --json=v2".to_owned(),
            code: output.code,
            stderr: output.stderr,
        });
    }
    let remaining = parse_outdated(&output.stdout)?.total();
    let updated = before.saturating_sub(remaining);
    Ok(BackendOperationRecord {
        backend_id: plan.backend_id.clone(),
        backend_name: plan.backend_name.clone(),
        action: None,
        command: None,
        status: if remaining == 0 {
            OperationStatus::Updated
        } else {
            OperationStatus::Deferred
        },
        message: Some(format!(
            "before: {before} outdated · updated: {updated} · remaining: {remaining}"
        )),
        privilege_status: None,
    })
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

#[cfg(test)]
mod tests {
    use super::{homebrew_command, is_busy_output, parse_outdated};

    #[test]
    fn every_homebrew_command_suppresses_process_local_analytics_and_hints() {
        let command = homebrew_command(std::path::Path::new("/opt/homebrew/bin/brew"));
        let environment: std::collections::HashMap<_, _> = command
            .env
            .iter()
            .map(|(key, value)| (key, value))
            .collect();

        assert_eq!(
            environment.get(&std::ffi::OsString::from("HOMEBREW_NO_ANALYTICS")),
            Some(&&std::ffi::OsString::from("1"))
        );
        assert_eq!(
            environment.get(&std::ffi::OsString::from("HOMEBREW_NO_UPDATE_REPORT_NEW")),
            Some(&&std::ffi::OsString::from("1"))
        );
        assert_eq!(
            command
                .env
                .iter()
                .filter(|(key, _)| key == &std::ffi::OsString::from("HOMEBREW_NO_UPDATE_REPORT_NEW"))
                .count(),
            1,
            "each Homebrew environment key must be rendered once"
        );
        assert_eq!(
            environment.get(&std::ffi::OsString::from("HOMEBREW_NO_ENV_HINTS")),
            Some(&&std::ffi::OsString::from("1"))
        );
    }

    #[test]
    fn parses_formulae_casks_and_pinned_state() {
        let parsed = parse_outdated(
            r#"{"formulae":[{"name":"openssl@3","pinned":true},{"name":"git"}],"casks":[{"name":"firefox"}]}"#,
        )
        .expect("Homebrew JSON v2 should parse");

        assert_eq!(parsed.total(), 3);
        assert_eq!(parsed.formula_names(), vec!["openssl@3 (pinned)", "git"]);
        assert_eq!(parsed.cask_names(), vec!["firefox"]);
    }

    #[test]
    fn empty_outdated_json_is_evidence_for_up_to_date() {
        let parsed = parse_outdated(r#"{"formulae":[],"casks":[]}"#)
            .expect("empty Homebrew JSON v2 should parse");
        assert!(parsed.is_empty());
    }

    #[test]
    fn recognizes_homebrew_update_concurrency_message() {
        assert!(is_busy_output(
            "Error: Another `brew update` process is already running."
        ));
    }
}
