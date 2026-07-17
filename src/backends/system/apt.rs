use crate::{
    backends::{
        contract::{command_path, InstallPreflight},
        util::{capture_checked, match_kind, parse_key_value_lines},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, BackendOperationRecord, Capability,
        DeveloperTarget, ExecutionPlan, InstalledPackage, MaintenancePlan, NativeCommand,
        OperationKind, OperationStatus, PackageCandidate, PackageDomain, PackageInfo,
        PrivilegeRequirement,
    },
    execution::{render_execution_plan_with_context, ProcessRunner, ProcessStatus},
};

pub struct AptBackend;

const APT_LOCK_TIMEOUT_SECONDS: &str = "60";

const CAPABILITIES: &[Capability] = &[
    Capability::Search,
    Capability::Install,
    Capability::Remove,
    Capability::Update,
    Capability::Upgrade,
    Capability::List,
    Capability::Info,
];

const REQUIREMENTS: &[CommandRequirement] = &[
    CommandRequirement {
        key: "apt-get",
        alternatives: &["apt-get"],
    },
    CommandRequirement {
        key: "apt-cache",
        alternatives: &["apt-cache"],
    },
    CommandRequirement {
        key: "dpkg-query",
        alternatives: &["dpkg-query"],
    },
];

impl Backend for AptBackend {
    fn id(&self) -> &'static str {
        "apt"
    }
    fn display_name(&self) -> &'static str {
        "APT"
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
        let apt_cache = command_path(self, commands, "apt-cache")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(apt_cache).args(["search", "--names-only", query]),
        )?;

        let mut candidates = Vec::new();
        for line in output.lines() {
            let Some((package_id, description)) = line.split_once(" - ") else {
                continue;
            };
            let package_id = package_id.trim();
            if package_id.is_empty() {
                continue;
            }

            let version = if package_id.eq_ignore_ascii_case(query) {
                self.candidate_version(commands, runner, package_id)
                    .ok()
                    .flatten()
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
                description: Some(description.trim().to_owned()),
                source: Some("APT repositories".to_owned()),
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
        let dpkg_query = command_path(self, commands, "dpkg-query")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(dpkg_query)
                .arg("-W")
                .arg("-f=${binary:Package}\t${Version}\n"),
        )?;

        Ok(output
            .lines()
            .filter_map(|line| {
                let mut columns = line.splitn(2, '\t');
                let package_id = columns.next()?.trim();
                if package_id.is_empty() {
                    return None;
                }
                Some(InstalledPackage {
                    backend_id: self.id().to_owned(),
                    backend_name: self.display_name().to_owned(),
                    category: self.category(),
                    domain: PackageDomain::System,
                    package_id: package_id.to_owned(),
                    display_name: package_id.to_owned(),
                    version: columns
                        .next()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned),
                    description: None,
                    source: Some("dpkg database".to_owned()),
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
        let apt_cache = command_path(self, commands, "apt-cache")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(apt_cache).args(["show", package_id]),
        )?;
        let fields = parse_key_value_lines(&output);

        Ok(PackageInfo {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::System,
            package_id: fields
                .get("Package")
                .cloned()
                .unwrap_or_else(|| package_id.to_owned()),
            display_name: fields
                .get("Package")
                .cloned()
                .unwrap_or_else(|| package_id.to_owned()),
            version: fields.get("Version").cloned(),
            description: fields.get("Description").cloned(),
            source: fields
                .get("Source")
                .cloned()
                .or_else(|| Some("APT repositories".to_owned())),
            scope: Some("system".to_owned()),
            artifact_kind: Some("system package".to_owned()),
            installed: None,
            extra: fields
                .into_iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "Package" | "Version" | "Description" | "Source"
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
        let apt_cache = command_path(self, commands, "apt-cache")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(apt_cache).args(["show", package_id]),
        )
    }

    fn preflight_plan_install(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        candidate: &PackageCandidate,
    ) -> AllpResult<InstallPreflight> {
        match self.installed_version(commands, runner, &candidate.package_id) {
            Ok(Some(installed_version)) => Ok(InstallPreflight::AlreadyInstalled {
                package_id: candidate.package_id.clone(),
                installed_version: Some(installed_version),
                candidate_version: candidate.version.clone(),
            }),
            Ok(None) => Ok(InstallPreflight::Continue),
            Err(_) => Ok(InstallPreflight::Continue),
        }
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let apt_get = command_path(self, commands, "apt-get")?;
        Ok(ExecutionPlan {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            operation: OperationKind::Install,
            action: "Install system package".to_owned(),
            package_id: Some(candidate.package_id.clone()),
            source: candidate.source.clone(),
            scope: candidate.scope.clone(),
            command: apt_mutation_command(apt_get, "install", &[candidate.package_id.as_str()]),
            privilege: PrivilegeRequirement::RootRequired,
            requires_root: true,
            interactive: true,
        })
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let apt_get = command_path(self, commands, "apt-get")?;
        Ok(ExecutionPlan {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            operation: OperationKind::Remove,
            action: "Remove system package".to_owned(),
            package_id: Some(package.package_id.clone()),
            source: package.source.clone(),
            scope: package.scope.clone(),
            command: apt_mutation_command(apt_get, "remove", &[package.package_id.as_str()]),
            privilege: PrivilegeRequirement::RootRequired,
            requires_root: true,
            interactive: true,
        })
    }

    fn plan_update(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let apt_get = command_path(self, commands, "apt-get")?;
        Ok(MaintenancePlan::from_plans(vec![ExecutionPlan {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            operation: OperationKind::Update,
            action: "Refresh package metadata".to_owned(),
            package_id: None,
            source: Some("APT repositories".to_owned()),
            scope: Some("system".to_owned()),
            command: apt_mutation_command(apt_get, "update", &[]),
            privilege: PrivilegeRequirement::RootRequired,
            requires_root: true,
            interactive: true,
        }]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let apt_get = command_path(self, commands, "apt-get")?;
        Ok(MaintenancePlan::from_plans(vec![ExecutionPlan {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            operation: OperationKind::Upgrade,
            action: "Upgrade installed APT packages".to_owned(),
            package_id: None,
            source: Some("APT repositories".to_owned()),
            scope: Some("system".to_owned()),
            command: apt_mutation_command(apt_get, "upgrade", &[]),
            privilege: PrivilegeRequirement::RootRequired,
            requires_root: true,
            interactive: true,
        }]))
    }

    fn classify_execution_failure(
        &self,
        plan: &ExecutionPlan,
        status: &ProcessStatus,
        command: &str,
    ) -> Option<AllpError> {
        let info = parse_apt_busy(&status.stderr)?;
        Some(AllpError::BackendBusy {
            backend: self.display_name().to_owned(),
            command: if command.is_empty() {
                render_execution_plan_with_context(
                    plan,
                    &crate::domain::RuntimePrivilegeContext::NormalUser,
                )
            } else {
                command.to_owned()
            },
            code: status.code,
            lock_path: info.lock_path,
            holder_pid: info.holder_pid,
            holder_process: info.holder_process,
        })
    }

    fn classify_execution_success(
        &self,
        plan: &ExecutionPlan,
        status: &ProcessStatus,
        _command: &str,
    ) -> Option<Vec<BackendOperationRecord>> {
        let output = if status.stderr.is_empty() {
            status.stdout.clone()
        } else {
            format!("{}\n{}", status.stdout, status.stderr)
        };
        let parsed = parse_apt_upgrade_result(&output)?;
        let mut records = Vec::new();
        let changed = parsed.changed_count();
        if changed > 0 {
            records.push(BackendOperationRecord {
                backend_id: plan.backend_id.clone(),
                backend_name: plan.backend_name.clone(),
                action: None,
                command: None,
                status: OperationStatus::Updated,
                message: Some(package_count_message(changed)),
            });
        }
        if parsed.deferred_count() > 0 {
            records.push(BackendOperationRecord {
                backend_id: plan.backend_id.clone(),
                backend_name: plan.backend_name.clone(),
                action: None,
                command: None,
                status: OperationStatus::Deferred,
                message: Some(parsed.deferred_message()),
            });
        }
        if records.is_empty() && parsed.saw_summary {
            records.push(BackendOperationRecord {
                backend_id: plan.backend_id.clone(),
                backend_name: plan.backend_name.clone(),
                action: None,
                command: None,
                status: OperationStatus::UpToDate,
                message: Some("no package changes available".to_owned()),
            });
        }
        (!records.is_empty()).then_some(records)
    }
}

impl AptBackend {
    fn installed_version(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<Option<String>> {
        let dpkg_query = command_path(self, commands, "dpkg-query")?;
        let output = runner.capture(
            &NativeCommand::new(dpkg_query)
                .arg("-W")
                .arg("-f=${Status}\t${Version}\n")
                .arg(package_id),
        )?;
        if !output.success {
            return Ok(None);
        }
        Ok(output.stdout.lines().find_map(|line| {
            let (status, version) = line.split_once('\t')?;
            status
                .contains("install ok installed")
                .then(|| version.trim().to_owned())
        }))
    }

    fn candidate_version(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<Option<String>> {
        let apt_cache = command_path(self, commands, "apt-cache")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(apt_cache).args(["policy", package_id]),
        )?;
        Ok(output.lines().find_map(|line| {
            line.trim()
                .strip_prefix("Candidate:")
                .map(str::trim)
                .filter(|value| *value != "(none)")
                .map(str::to_owned)
        }))
    }
}

fn apt_mutation_command(
    apt_get: &std::path::Path,
    operation: &str,
    package_ids: &[&str],
) -> NativeCommand {
    let mut command = NativeCommand::new(apt_get)
        .arg("-o")
        .arg(format!("DPkg::Lock::Timeout={APT_LOCK_TIMEOUT_SECONDS}"))
        .arg(operation);
    if !package_ids.is_empty() {
        command = command.arg("--").args(package_ids.iter().copied());
    }
    command
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AptBusyInfo {
    lock_path: Option<String>,
    holder_pid: Option<u32>,
    holder_process: Option<String>,
}

fn parse_apt_busy(stderr: &str) -> Option<AptBusyInfo> {
    let lower = stderr.to_ascii_lowercase();
    if ![
        "could not get lock",
        "unable to acquire the dpkg frontend lock",
        "it is held by process",
        "could not open lock file",
        "waiting for cache lock",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return None;
    }

    Some(AptBusyInfo {
        lock_path: parse_lock_path(stderr),
        holder_pid: parse_holder_pid(stderr),
        holder_process: parse_holder_process(stderr),
    })
}

fn parse_lock_path(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let Some(position) = line.find("/var/") else {
            continue;
        };
        let tail = &line[position..];
        let path = tail
            .split(|character: char| character.is_whitespace() || matches!(character, '.' | ','))
            .next()
            .unwrap_or(tail)
            .trim_matches(|character| matches!(character, '\'' | '"' | ':'));
        if !path.is_empty() {
            return Some(path.to_owned());
        }
    }
    None
}

fn parse_holder_pid(stderr: &str) -> Option<u32> {
    let marker = "It is held by process ";
    let start = stderr.find(marker)? + marker.len();
    stderr[start..]
        .split_whitespace()
        .next()?
        .trim_matches(|character: char| !character.is_ascii_digit())
        .parse()
        .ok()
}

fn parse_holder_process(stderr: &str) -> Option<String> {
    let marker = "It is held by process ";
    let start = stderr.find(marker)? + marker.len();
    let rest = &stderr[start..];
    let open = rest.find('(')?;
    let close = rest[open + 1..].find(')')? + open + 1;
    let process = rest[open + 1..close].trim();
    (!process.is_empty()).then(|| process.to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AptDeferredReason {
    PhasedUpdate,
    Deferred,
}

impl AptDeferredReason {
    fn label(self) -> &'static str {
        match self {
            Self::PhasedUpdate => "phased updates",
            Self::Deferred => "deferred packages",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::PhasedUpdate => "deferred due to phasing",
            Self::Deferred => "deferred by APT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AptUpgradeResult {
    upgraded: usize,
    newly_installed: usize,
    removed: usize,
    not_upgraded: usize,
    deferred_packages: Vec<String>,
    deferred_reason: Option<AptDeferredReason>,
    saw_summary: bool,
}

impl AptUpgradeResult {
    fn changed_count(&self) -> usize {
        self.upgraded + self.newly_installed + self.removed
    }

    fn deferred_count(&self) -> usize {
        if !self.deferred_packages.is_empty() {
            self.deferred_packages.len()
        } else if self.deferred_reason.is_some() {
            self.not_upgraded
        } else {
            0
        }
    }

    fn deferred_message(&self) -> String {
        let count = self.deferred_count();
        let label = self
            .deferred_reason
            .unwrap_or(AptDeferredReason::Deferred)
            .label();
        let mut message = format!("{count} {label}");
        if let Some(reason) = self.deferred_reason {
            message.push_str(&format!(" · {}", reason.detail()));
        }
        if !self.deferred_packages.is_empty() {
            message.push_str(": ");
            message.push_str(&self.deferred_packages.join(", "));
        }
        message
    }
}

fn parse_apt_upgrade_result(output: &str) -> Option<AptUpgradeResult> {
    let mut result = AptUpgradeResult {
        upgraded: 0,
        newly_installed: 0,
        removed: 0,
        not_upgraded: 0,
        deferred_packages: Vec::new(),
        deferred_reason: None,
        saw_summary: false,
    };
    let mut collect_deferred = false;

    for raw_line in output.lines() {
        let line = raw_line.trim();
        let lower = line.to_ascii_lowercase();
        if let Some(count) = count_before_marker(line, " upgraded") {
            result.upgraded = count;
            result.saw_summary = true;
        }
        if let Some(count) = count_before_marker(line, " newly installed") {
            result.newly_installed = count;
            result.saw_summary = true;
        }
        if let Some(count) = count_before_marker(line, " to remove") {
            result.removed = count;
            result.saw_summary = true;
        }
        if let Some(count) = count_before_marker(line, " not upgraded") {
            result.not_upgraded = count;
            result.saw_summary = true;
        }

        if lower.contains("deferred due to phasing") {
            result.deferred_reason = Some(AptDeferredReason::PhasedUpdate);
            collect_deferred = true;
            continue;
        }
        if lower.contains("following upgrades have been deferred") {
            result.deferred_reason = Some(AptDeferredReason::Deferred);
            collect_deferred = true;
            continue;
        }

        if collect_deferred {
            if line.is_empty() {
                collect_deferred = false;
                continue;
            }
            if looks_like_package_name(line) {
                result.deferred_packages.push(line.to_owned());
            } else if !raw_line.starts_with(char::is_whitespace) {
                collect_deferred = false;
            }
        }
    }

    result.saw_summary.then_some(result)
}

fn count_before_marker(line: &str, marker: &str) -> Option<usize> {
    let position = line.find(marker)?;
    line[..position]
        .split_whitespace()
        .last()?
        .trim_matches(|character: char| !character.is_ascii_digit())
        .parse()
        .ok()
}

fn looks_like_package_name(line: &str) -> bool {
    !line.is_empty()
        && line.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.' | ':')
        })
}

fn package_count_message(count: usize) -> String {
    if count == 1 {
        "1 package".to_owned()
    } else {
        format!("{count} packages")
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_apt_busy, parse_apt_upgrade_result, AptDeferredReason};

    #[test]
    fn parses_apt_lock_holder_details() {
        let stderr = "E: Could not get lock /var/lib/dpkg/lock-frontend. It is held by process 7515 (packagekitd)\nE: Unable to acquire the dpkg frontend lock";

        let info = parse_apt_busy(stderr).expect("lock stderr should parse as busy");

        assert_eq!(
            info.lock_path.as_deref(),
            Some("/var/lib/dpkg/lock-frontend")
        );
        assert_eq!(info.holder_pid, Some(7515));
        assert_eq!(info.holder_process.as_deref(), Some("packagekitd"));
    }

    #[test]
    fn parses_updated_and_phased_apt_results() {
        let output = r#"Reading package lists...
Building dependency tree...
Reading state information...
Calculating upgrade...
The following upgrades have been deferred due to phasing:
  python3-software-properties
  software-properties-common
  software-properties-gtk
The following packages will be upgraded:
  curl
1 upgraded, 0 newly installed, 0 to remove and 3 not upgraded.
"#;

        let parsed = parse_apt_upgrade_result(output).expect("APT summary should parse");

        assert_eq!(parsed.changed_count(), 1);
        assert_eq!(parsed.deferred_count(), 3);
        assert_eq!(
            parsed.deferred_reason,
            Some(AptDeferredReason::PhasedUpdate)
        );
        assert_eq!(
            parsed.deferred_packages,
            vec![
                "python3-software-properties",
                "software-properties-common",
                "software-properties-gtk"
            ]
        );
    }
}
