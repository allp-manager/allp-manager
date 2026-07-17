use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked, match_kind},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, Capability, DeveloperTarget, ExecutionPlan,
        InstalledPackage, MaintenancePlan, NativeCommand, OperationKind, PackageCandidate,
        PackageDomain, PackageInfo, PrivilegeRequirement,
    },
    execution::{render_native_command, ProcessRunner},
};
use serde_json::Value;
use std::env;
use std::path::Path;

pub struct PythonBackend;

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
    key: "python",
    alternatives: &["python3", "python"],
}];

const OPTIONAL: &[CommandRequirement] = &[
    CommandRequirement {
        key: "pip",
        alternatives: &["pip3", "pip"],
    },
    CommandRequirement {
        key: "pipx",
        alternatives: &["pipx"],
    },
    CommandRequirement {
        key: "uv",
        alternatives: &["uv"],
    },
];
const DOMAINS: &[PackageDomain] = &[PackageDomain::Python];

impl Backend for PythonBackend {
    fn id(&self) -> &'static str {
        "python"
    }

    fn display_name(&self) -> &'static str {
        "Python"
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
        &["pypi", "pip", "pipx", "uv"]
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
        let output = python_pip_capture(commands, runner, &["index", "versions", query])?;
        let installers = installer_choices(commands);
        let mut candidates = parse_pypi_candidates(self, &output, query, installers.clone());
        if candidates.is_empty() && !output.trim().is_empty() {
            candidates.push(candidate(
                self,
                query,
                None,
                Some(output.lines().next().unwrap_or_default().trim().to_owned()),
                match_kind(query, query),
                installers,
            ));
        }
        Ok(candidates)
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let output = python_pip_capture(commands, runner, &["list", "--format=freeze"])?;
        Ok(output
            .lines()
            .filter_map(|line| {
                let (package_id, version) = line.split_once("==")?;
                Some(InstalledPackage {
                    backend_id: self.id().to_owned(),
                    backend_name: self.display_name().to_owned(),
                    category: self.category(),
                    domain: PackageDomain::Python,
                    package_id: package_id.to_owned(),
                    display_name: package_id.to_owned(),
                    version: Some(version.to_owned()),
                    description: None,
                    source: Some("PyPI / current Python environment".to_owned()),
                    scope: Some(python_scope()),
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
        let output = python_pip_capture(commands, runner, &["show", package_id])?;
        Ok(info_from_pip_show(self, package_id, &output))
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        python_pip_capture(commands, runner, &["show", package_id])
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let installer = preferred_installer(candidate, commands);
        let (command, action) = match installer.as_str() {
            "pipx" => (
                NativeCommand::new(command_path(self, commands, "pipx")?)
                    .args(["install", candidate.package_id.as_str()]),
                "Install isolated Python CLI",
            ),
            "uv" => (
                NativeCommand::new(command_path(self, commands, "uv")?).args([
                    "tool",
                    "install",
                    candidate.package_id.as_str(),
                ]),
                "Install Python tool with uv",
            ),
            _ => (
                python_pip_command(commands, &["install", candidate.package_id.as_str()])?,
                "Install Python package",
            ),
        };
        Ok(plan(
            self,
            OperationKind::Install,
            action,
            Some(candidate.package_id.clone()),
            command,
        ))
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        Ok(plan(
            self,
            OperationKind::Remove,
            "Remove Python package",
            Some(package.package_id.clone()),
            python_pip_command(commands, &["uninstall", package.package_id.as_str()])?,
        ))
    }

    fn plan_update(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        selector: Option<&str>,
        target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        python_maintenance(
            self,
            commands,
            runner,
            selector,
            target,
            OperationKind::Update,
        )
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        selector: Option<&str>,
        target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        python_maintenance(
            self,
            commands,
            runner,
            selector,
            target,
            OperationKind::Upgrade,
        )
    }
}

fn python_maintenance(
    backend: &PythonBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    selector: Option<&str>,
    target: Option<DeveloperTarget>,
    operation: OperationKind,
) -> AllpResult<MaintenancePlan> {
    let mut output = MaintenancePlan::default();
    let target = target.unwrap_or(DeveloperTarget::All);

    if python_selector_allows(selector, "pip") && target.includes(DeveloperTarget::Environment) {
        append_pip_environment_plan(backend, commands, runner, operation, &mut output)?;
    }
    if python_selector_allows(selector, "pipx") && target.includes(DeveloperTarget::Tools) {
        append_pipx_plan(backend, commands, runner, operation, &mut output)?;
    }
    if python_selector_allows(selector, "uv") && target.includes(DeveloperTarget::Tools) {
        append_uv_tool_plan(backend, commands, runner, operation, &mut output)?;
    }

    if output.plans.is_empty() && output.records.is_empty() {
        output.records.push(python_record(
            backend,
            "Python",
            crate::domain::OperationStatus::NotApplicable,
            "target is not applicable to detected Python tools",
        ));
    }

    Ok(output)
}

fn append_pip_environment_plan(
    backend: &PythonBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    operation: OperationKind,
    output: &mut MaintenancePlan,
) -> AllpResult<()> {
    let Some(venv) = env::var_os("VIRTUAL_ENV") else {
        output.records.push(python_record(
            backend,
            "pip environment",
            crate::domain::OperationStatus::Protected,
            "no active Python environment; refusing to modify system Python",
        ));
        return Ok(());
    };

    let outdated = match pip_outdated(backend, commands, runner) {
        Ok(outdated) => outdated,
        Err(AllpError::InvalidInput(message)) if message.contains("externally managed") => {
            output.records.push(python_record(
                backend,
                "pip environment",
                crate::domain::OperationStatus::Protected,
                "externally managed Python environment; use a virtual environment, pipx, or uv tool",
            ));
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    if outdated.is_empty() {
        output.records.push(python_record(
            backend,
            "pip environment",
            crate::domain::OperationStatus::UpToDate,
            "no outdated pip packages found in the active environment",
        ));
        return Ok(());
    }

    let package_ids = outdated
        .iter()
        .map(|package| package.name.as_str())
        .collect::<Vec<_>>();
    let command = python_module_pip_command(commands, &["install", "--upgrade"], &package_ids)?;
    let versions = outdated
        .iter()
        .map(|package| {
            format!(
                "{} {} -> {}",
                package.name, package.current_version, package.latest_version
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    output.plans.push(python_plan(
        backend,
        "pip environment",
        operation,
        "Upgrade selected outdated pip packages in the active environment",
        Some("PyPI".to_owned()),
        Some(format!(
            "active virtual environment · {} · pip uses the same native operation for update and upgrade · {versions}",
            Path::new(&venv).display()
        )),
        command,
    ));
    Ok(())
}

fn append_pipx_plan(
    backend: &PythonBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    operation: OperationKind,
    output: &mut MaintenancePlan,
) -> AllpResult<()> {
    let Some(pipx) = commands.get("pipx").map(std::path::PathBuf::as_path) else {
        output.records.push(python_record(
            backend,
            "pipx tools",
            crate::domain::OperationStatus::Unavailable,
            "backend not installed",
        ));
        return Ok(());
    };

    if !tool_listing_has_entries(
        backend,
        runner,
        NativeCommand::new(pipx).args(["list", "--json"]),
    )? {
        output.records.push(python_record(
            backend,
            "pipx tools",
            crate::domain::OperationStatus::UpToDate,
            "no pipx-managed tools were found",
        ));
        return Ok(());
    }

    output.plans.push(python_plan(
        backend,
        "pipx tools",
        operation,
        "Upgrade all installed pipx tools",
        Some("PyPI".to_owned()),
        Some("isolated Python tools · respects native pipx pins".to_owned()),
        NativeCommand::new(pipx).arg("upgrade-all"),
    ));
    Ok(())
}

fn append_uv_tool_plan(
    backend: &PythonBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    operation: OperationKind,
    output: &mut MaintenancePlan,
) -> AllpResult<()> {
    let Some(uv) = commands.get("uv").map(std::path::PathBuf::as_path) else {
        output.records.push(python_record(
            backend,
            "uv tools",
            crate::domain::OperationStatus::Unavailable,
            "backend not installed",
        ));
        return Ok(());
    };

    if !tool_listing_has_entries(
        backend,
        runner,
        NativeCommand::new(uv).args(["tool", "list", "--json"]),
    )? {
        output.records.push(python_record(
            backend,
            "uv tools",
            crate::domain::OperationStatus::UpToDate,
            "no uv-managed tools were found",
        ));
        return Ok(());
    }

    output.plans.push(python_plan(
        backend,
        "uv tools",
        operation,
        "Upgrade all uv-managed tools",
        Some("PyPI".to_owned()),
        Some("uv isolated tools · update and upgrade map to uv tool upgrade".to_owned()),
        NativeCommand::new(uv).args(["tool", "upgrade", "--all"]),
    ));
    Ok(())
}

#[derive(Debug, Clone)]
struct PipOutdatedPackage {
    name: String,
    current_version: String,
    latest_version: String,
}

fn pip_outdated(
    backend: &PythonBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
) -> AllpResult<Vec<PipOutdatedPackage>> {
    let command =
        python_module_pip_command(commands, &["list", "--outdated", "--format=json"], &[])?;
    let rendered = render_native_command(&command);
    let output = runner.capture(&command)?;
    if !output.success {
        if output.stderr.contains("externally-managed-environment") {
            return Err(AllpError::InvalidInput(
                "externally managed Python environment; refusing to add --break-system-packages automatically"
                    .to_owned(),
            ));
        }
        return Err(AllpError::CommandFailed {
            backend: backend.display_name().to_owned(),
            command: rendered,
            code: output.code,
            stderr: output.stderr,
        });
    }
    parse_pip_outdated_json(backend, &output.stdout)
}

fn parse_pip_outdated_json(
    backend: &PythonBackend,
    output: &str,
) -> AllpResult<Vec<PipOutdatedPackage>> {
    let value = serde_json::from_str::<Value>(output.trim()).map_err(|error| AllpError::Parse {
        backend: backend.display_name().to_owned(),
        message: format!("invalid pip outdated JSON: {error}"),
    })?;
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            Some(PipOutdatedPackage {
                name: item.get("name")?.as_str()?.to_owned(),
                current_version: item
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned(),
                latest_version: item
                    .get("latest_version")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned(),
            })
        })
        .collect())
}

fn tool_listing_has_entries(
    backend: &PythonBackend,
    runner: &dyn ProcessRunner,
    command: NativeCommand,
) -> AllpResult<bool> {
    let rendered = render_native_command(&command);
    let output = runner.capture(&command)?;
    if !output.success {
        return Err(AllpError::CommandFailed {
            backend: backend.display_name().to_owned(),
            command: rendered,
            code: output.code,
            stderr: output.stderr,
        });
    }
    Ok(json_has_entries(&output.stdout))
}

fn json_has_entries(output: &str) -> bool {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return false;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Object(map)) => !map.is_empty(),
        Ok(Value::Array(items)) => !items.is_empty(),
        _ => true,
    }
}

fn python_module_pip_command(
    commands: &CommandMap,
    prefix: &[&str],
    packages: &[&str],
) -> AllpResult<NativeCommand> {
    let python = commands
        .get("python")
        .ok_or_else(|| crate::domain::AllpError::BackendNotDetected("python".to_owned()))?;
    Ok(NativeCommand::new(python)
        .args(["-m", "pip"])
        .args(prefix.iter().copied())
        .args(packages.iter().copied()))
}

fn python_selector_allows(selector: Option<&str>, installer: &str) -> bool {
    let Some(selector) = selector else {
        return true;
    };
    let selector = selector.to_ascii_lowercase();
    selector == "python" || selector == "pypi" || selector == installer
}

fn python_record(
    backend: &PythonBackend,
    target_name: &str,
    status: crate::domain::OperationStatus,
    reason: &str,
) -> crate::domain::BackendOperationRecord {
    MaintenancePlan::record(backend.id(), target_name, status, reason)
}

fn python_plan(
    backend: &PythonBackend,
    backend_name: &str,
    operation: OperationKind,
    action: &str,
    source: Option<String>,
    scope: Option<String>,
    command: NativeCommand,
) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend_name.to_owned(),
        operation,
        action: action.to_owned(),
        package_id: None,
        source,
        scope,
        details: Vec::new(),
        command,
        privilege: PrivilegeRequirement::OriginalUserRequired,
        requires_root: false,
        interactive: true,
    }
}

fn python_pip_capture(
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    args: &[&str],
) -> AllpResult<String> {
    let command = python_pip_command(commands, args)?;
    let backend = PythonBackend;
    match capture_checked(&backend, runner, command) {
        Err(AllpError::CommandFailed { stderr, .. }) if is_missing_pip_error(&stderr) => {
            Err(AllpError::InvalidInput(
                "Python was detected, but pip is not available for this interpreter. Install pip for the active Python, use a virtual environment, or choose pipx/uv when appropriate."
                    .to_owned(),
            ))
        }
        result => result,
    }
}

fn is_missing_pip_error(stderr: &str) -> bool {
    let value = stderr.to_ascii_lowercase();
    value.contains("no module named pip") || value.contains("ensurepip")
}

fn python_pip_command(commands: &CommandMap, args: &[&str]) -> AllpResult<NativeCommand> {
    if let Some(pip) = commands.get("pip") {
        return Ok(NativeCommand::new(pip).args(args.iter().copied()));
    }
    let python = commands
        .get("python")
        .ok_or_else(|| crate::domain::AllpError::BackendNotDetected("python".to_owned()))?;
    Ok(NativeCommand::new(python)
        .args(["-m", "pip"])
        .args(args.iter().copied()))
}

fn parse_pypi_candidates(
    backend: &PythonBackend,
    output: &str,
    query: &str,
    installers: Vec<String>,
) -> Vec<PackageCandidate> {
    let mut candidates = Vec::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("Available versions") || line.starts_with("INSTALLED:") {
            continue;
        }
        let (package_id, version, description) = if let Some((left, right)) = line.split_once(" - ")
        {
            (left.trim(), None, Some(right.trim().to_owned()))
        } else if let Some((name, rest)) = line.split_once('(') {
            (
                name.trim(),
                rest.split_once(')')
                    .map(|(version, _)| version.trim().to_owned()),
                None,
            )
        } else {
            continue;
        };
        candidates.push(candidate(
            backend,
            package_id,
            version,
            description,
            match_kind(package_id, query),
            installers.clone(),
        ));
    }
    candidates
}

fn candidate(
    backend: &PythonBackend,
    package_id: &str,
    version: Option<String>,
    description: Option<String>,
    match_kind: crate::domain::MatchKind,
    installers: Vec<String>,
) -> PackageCandidate {
    PackageCandidate {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        category: backend.category(),
        domain: PackageDomain::Python,
        package_id: package_id.to_owned(),
        display_name: package_id.to_owned(),
        version,
        description,
        source: Some("PyPI".to_owned()),
        installers,
        artifact_kind: "Python package".to_owned(),
        scope: Some(python_scope()),
        match_kind,
        identity: PackageCandidate::infer_identity(
            match_kind,
            PackageDomain::Python,
            "Python package",
        ),
        metadata: Default::default(),
    }
}

fn installer_choices(commands: &CommandMap) -> Vec<String> {
    let mut choices = Vec::new();
    if commands.contains_key("pip") || commands.contains_key("python") {
        choices.push("pip".to_owned());
    }
    if commands.contains_key("pipx") {
        choices.push("pipx".to_owned());
    }
    if commands.contains_key("uv") {
        choices.push("uv".to_owned());
    }
    choices
}

fn preferred_installer(candidate: &PackageCandidate, commands: &CommandMap) -> String {
    candidate
        .installers
        .iter()
        .find(|installer| commands.contains_key(installer.as_str()) || installer.as_str() == "pip")
        .cloned()
        .unwrap_or_else(|| "pip".to_owned())
}

fn plan(
    backend: &PythonBackend,
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
        source: Some("PyPI".to_owned()),
        scope: Some(python_scope()),
        details: Vec::new(),
        command,
        privilege: PrivilegeRequirement::OriginalUserRequired,
        requires_root: false,
        interactive: true,
    }
}

fn info_from_pip_show(backend: &PythonBackend, package_id: &str, output: &str) -> PackageInfo {
    let fields = crate::backends::util::parse_key_value_lines(output);
    PackageInfo {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        category: backend.category(),
        domain: PackageDomain::Python,
        package_id: fields
            .get("Name")
            .cloned()
            .unwrap_or_else(|| package_id.to_owned()),
        display_name: fields
            .get("Name")
            .cloned()
            .unwrap_or_else(|| package_id.to_owned()),
        version: fields.get("Version").cloned(),
        description: fields.get("Summary").cloned(),
        source: Some("PyPI".to_owned()),
        scope: Some(python_scope()),
        artifact_kind: Some("Python package".to_owned()),
        installed: None,
        extra: fields.into_iter().collect(),
    }
}

fn python_scope() -> String {
    if env::var_os("VIRTUAL_ENV").is_some() {
        "active virtual environment".to_owned()
    } else {
        "current Python environment".to_owned()
    }
}
