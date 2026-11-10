use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked_with_privilege, match_kind},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, BackendSearchIssue, BackendSearchIssueKind,
        BackendSearchReport, Capability, DeveloperTarget, ExecutionPlan, InstalledPackage,
        MaintenancePlan, NativeCommand, OperationKind, PackageCandidate, PackageDomain,
        PackageInfo, PrivilegeRequirement,
    },
    execution::{render_native_command, ProcessRunner},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
pub(crate) const PIP_SETUP: &str = "pip is unavailable for this Python. In your project, create an environment with `python3 -m venv .venv` (Windows: `python -m venv .venv`), or use an installed pipx or uv for CLI tools.";
const PYPI_LOOKUP: &str = r#"
import json
import sys
from urllib.error import HTTPError
from urllib.request import urlopen

try:
    with urlopen(f'https://pypi.org/pypi/{sys.argv[1]}/json', timeout=10) as response:
        limit = 16 * 1024 * 1024
        data = response.read(limit + 1)
        if len(data) > limit:
            sys.exit('PyPI project metadata exceeded 16 MiB')
        info = json.loads(data)['info']
except HTTPError as error:
    if error.code != 404:
        raise
    print('null')
else:
    print(json.dumps({key: info[key] for key in ('name', 'version', 'summary')}))
"#;

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
    ) -> AllpResult<BackendSearchReport> {
        let Some(output) =
            python_pip_capture_if_available(commands, runner, &["index", "versions", query])?
        else {
            let candidates = pypi_project(commands, runner, query)?
                .map(|project| {
                    candidate(
                        self,
                        &project.name,
                        Some(project.version),
                        project.summary,
                        crate::domain::MatchKind::Exact,
                        installer_choices(commands, false),
                    )
                })
                .into_iter()
                .collect();
            return Ok(BackendSearchReport::complete(candidates));
        };
        let installers = installer_choices(commands, true);
        let candidates = parse_pypi_candidates(self, &output, query, installers);
        if candidates.is_empty() && !output.trim().is_empty() {
            return Ok(
                BackendSearchReport::default().with_issue(BackendSearchIssue {
                    kind: BackendSearchIssueKind::UnrecognizedOutput,
                    stage: Some("pip index versions".to_owned()),
                    message: "pip returned non-empty output without a recognizable package row"
                        .to_owned(),
                }),
            );
        }
        Ok(BackendSearchReport::complete(candidates))
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        python_inventory(self, commands, runner, None)
    }

    fn list_installed_with_selector(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        selector: Option<&str>,
    ) -> AllpResult<Vec<InstalledPackage>> {
        python_inventory(self, commands, runner, selector)
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        match installed_pip_info(commands, runner, package_id)? {
            Some(output) => Ok(info_from_pip_show(self, package_id, &output)),
            None => {
                let project = require_pypi_project(commands, runner, package_id)?;
                let mut info = info_from_pip_show(self, &project.name, "");
                info.version = Some(project.version);
                info.description = project.summary;
                info.scope = None;
                Ok(info)
            }
        }
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        if let Some(output) = installed_pip_info(commands, runner, package_id)? {
            return Ok(output);
        }
        serde_json::to_string_pretty(&require_pypi_project(commands, runner, package_id)?)
            .map_err(|error| pypi_parse_error(error.to_string()))
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let installer = candidate
            .installers
            .first()
            .ok_or_else(|| AllpError::InvalidInput(PIP_SETUP.to_owned()))?;
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
            "pip" => (
                python_pip_command(commands, &["install", candidate.package_id.as_str()])?,
                "Install Python package",
            ),
            _ => {
                return Err(AllpError::InvalidInput(format!(
                    "unknown Python installer {installer}"
                )))
            }
        };
        let mut install = plan(
            self,
            OperationKind::Install,
            action,
            Some(candidate.package_id.clone()),
            command,
        );
        install.scope = Some(installer_scope(commands, installer)?);
        Ok(install)
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let installer = package.installer.as_deref().unwrap_or("pip");
        let command = match installer {
            "pip" => python_pip_command(commands, &["uninstall", "--yes", &package.package_id])?,
            "pipx" => NativeCommand::new(command_path(self, commands, "pipx")?)
                .args(["uninstall", &package.package_id]),
            "uv" => NativeCommand::new(command_path(self, commands, "uv")?).args([
                "tool",
                "uninstall",
                &package.package_id,
            ]),
            _ => {
                return Err(AllpError::InvalidInput(format!(
                    "unknown Python installer {installer}"
                )))
            }
        };
        let mut removal = plan(
            self,
            OperationKind::Remove,
            "Remove Python package or tool",
            Some(package.package_id.clone()),
            command,
        );
        removal.scope = Some(installer_scope(commands, installer)?);
        Ok(removal)
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

fn python_inventory(
    backend: &PythonBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    selector: Option<&str>,
) -> AllpResult<Vec<InstalledPackage>> {
    let selector = selector.map(str::to_ascii_lowercase);
    let selected = selector
        .as_deref()
        .filter(|value| matches!(*value, "pip" | "pipx" | "uv"));
    let mut packages = Vec::new();
    if selected.is_none_or(|installer| installer == "pip") {
        packages.extend(pip_inventory(
            backend,
            commands,
            runner,
            selected == Some("pip"),
        )?);
    }
    for installer in ["pipx", "uv"] {
        if selected.is_some_and(|selected| selected != installer) {
            continue;
        }
        let Some(program) = commands.get(installer) else {
            if selected == Some(installer) {
                return Err(AllpError::BackendNotDetected(installer.to_owned()));
            }
            continue;
        };
        let command = if installer == "pipx" {
            NativeCommand::new(program).args(["list", "--json"])
        } else {
            NativeCommand::new(program).args(["tool", "list"])
        };
        let output = crate::backends::util::capture_checked_with_privilege(
            backend,
            runner,
            command,
            PrivilegeRequirement::OriginalUserRequired,
        )?;
        if installer == "pipx" {
            let report =
                serde_json::from_str::<Value>(&output).map_err(|error| AllpError::Parse {
                    backend: backend.display_name().to_owned(),
                    message: format!("invalid pipx inventory: {error}"),
                })?;
            let tools = report
                .get("venvs")
                .and_then(Value::as_object)
                .ok_or_else(|| AllpError::Parse {
                    backend: backend.display_name().to_owned(),
                    message: "pipx list did not return a venv inventory".to_owned(),
                })?;
            for (name, tool) in tools {
                let version = tool
                    .pointer("/metadata/main_package/package_version")
                    .and_then(Value::as_str);
                packages.push(python_tool(backend, installer, name, version));
            }
        } else {
            for line in output
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('-'))
            {
                let mut columns = line.split_whitespace();
                let name = columns.next().unwrap_or_default();
                let version = columns
                    .next()
                    .and_then(|value| value.strip_prefix('v'))
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| AllpError::Parse {
                        backend: backend.display_name().to_owned(),
                        message: "uv tool list did not return a recognizable tool inventory"
                            .to_owned(),
                    })?;
                packages.push(python_tool(backend, installer, name, Some(version)));
            }
        }
    }
    Ok(packages)
}

fn python_tool(
    backend: &PythonBackend,
    installer: &str,
    name: &str,
    version: Option<&str>,
) -> InstalledPackage {
    InstalledPackage {
        installer: Some(installer.to_owned()),
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        category: backend.category(),
        domain: PackageDomain::Python,
        package_id: name.to_owned(),
        display_name: name.to_owned(),
        version: version.map(str::to_owned),
        description: None,
        source: Some("PyPI".to_owned()),
        scope: Some(format!("isolated user tool via {installer}")),
    }
}

fn pip_inventory(
    backend: &PythonBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    pip_required: bool,
) -> AllpResult<Vec<InstalledPackage>> {
    let mut args = vec!["list", "--format=freeze"];
    if python_environment(commands)?.is_some() {
        args.push("--local");
    }
    let Some(output) = python_pip_capture_if_available(commands, runner, &args)? else {
        if pip_required {
            return Err(AllpError::InvalidInput(PIP_SETUP.to_owned()));
        }
        // A Python runtime without pip has no pip-managed inventory for
        // this backend. It must not make an otherwise complete system
        // package profile impossible to save.
        return Ok(Vec::new());
    };
    let scope = installer_scope(commands, "pip")?;
    Ok(output
        .lines()
        .filter_map(|line| {
            let (package_id, version) = line.split_once("==")?;
            Some(InstalledPackage {
                installer: Some("pip".to_owned()),
                backend_id: backend.id().to_owned(),
                backend_name: backend.display_name().to_owned(),
                category: backend.category(),
                domain: PackageDomain::Python,
                package_id: package_id.to_owned(),
                display_name: package_id.to_owned(),
                version: Some(version.to_owned()),
                description: None,
                source: Some("PyPI / current Python environment".to_owned()),
                scope: Some(scope.clone()),
            })
        })
        .collect())
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
    let (venv, python) = match python_environment(commands) {
        Ok(Some(environment)) => environment,
        Ok(None) => {
            output.records.push(python_record(
                backend,
                "pip environment",
                crate::domain::OperationStatus::Protected,
                "no active Python environment or project .venv; activate an environment or create one with python3 -m venv .venv (Windows: python -m venv .venv)",
            ));
            return Ok(());
        }
        Err(AllpError::InvalidInput(message)) => {
            output.records.push(python_record(
                backend,
                "pip environment",
                crate::domain::OperationStatus::Protected,
                &message,
            ));
            return Ok(());
        }
        Err(error) => return Err(error),
    };

    let outdated = match pip_outdated(backend, &python, runner) {
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
        Err(AllpError::CommandFailed { stderr, .. }) if is_missing_pip_error(&stderr) => {
            output.records.push(python_record(
                backend,
                "pip environment",
                crate::domain::OperationStatus::Unavailable,
                &format!("pip is missing in {}; run its Python with `-m ensurepip --upgrade` or recreate the environment with pip", venv.display()),
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
            "no outdated pip packages found in the selected environment",
        ));
        return Ok(());
    }

    let package_ids = outdated
        .iter()
        .map(|package| package.name.as_str())
        .collect::<Vec<_>>();
    let command = NativeCommand::new(&python)
        .args(["-m", "pip", "install", "--upgrade"])
        .args(package_ids);
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
        "Upgrade selected outdated pip packages in the selected environment",
        Some("PyPI".to_owned()),
        Some(format!(
            "virtual environment · {} · pip uses the same native operation for update and upgrade · {versions}",
            venv.display()
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

    let tools = capture_checked_with_privilege(
        backend,
        runner,
        NativeCommand::new(pipx).args(["list", "--json"]),
        PrivilegeRequirement::OriginalUserRequired,
    )?;
    let tools = serde_json::from_str::<Value>(&tools)
        .ok()
        .and_then(|value| {
            value
                .get("venvs")?
                .as_object()
                .map(|venvs| !venvs.is_empty())
        })
        .ok_or_else(|| AllpError::Parse {
            backend: backend.display_name().to_owned(),
            message: "pipx list did not return a valid venv inventory".to_owned(),
        })?;
    if !tools {
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

    let tools = capture_checked_with_privilege(
        backend,
        runner,
        NativeCommand::new(uv).args(["tool", "list"]),
        PrivilegeRequirement::OriginalUserRequired,
    )?;
    if tools.trim().is_empty() {
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

#[derive(Deserialize)]
struct PipOutdatedPackage {
    name: String,
    #[serde(rename = "version")]
    current_version: String,
    latest_version: String,
}

fn pip_outdated(
    backend: &PythonBackend,
    python: &Path,
    runner: &dyn ProcessRunner,
) -> AllpResult<Vec<PipOutdatedPackage>> {
    let command = NativeCommand::new(python).args([
        "-m",
        "pip",
        "list",
        "--outdated",
        "--format=json",
        "--local",
    ]);
    let rendered = render_native_command(&command);
    let output =
        runner.capture_with_privilege(&command, PrivilegeRequirement::OriginalUserRequired)?;
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
    let items: Vec<PipOutdatedPackage> =
        serde_json::from_str(output).map_err(|error| AllpError::Parse {
            backend: backend.display_name().to_owned(),
            message: format!("invalid pip outdated JSON: {error}"),
        })?;
    if items.iter().any(|item| {
        normalized_pypi_name(&item.name).is_none()
            || item.current_version.is_empty()
            || item.latest_version.is_empty()
    }) {
        return Err(AllpError::Parse {
            backend: backend.display_name().to_owned(),
            message: "pip outdated returned an invalid package row".to_owned(),
        });
    }
    Ok(items)
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

fn installed_pip_info(
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    package_id: &str,
) -> AllpResult<Option<String>> {
    match python_pip_capture_if_available(commands, runner, &["show", package_id]) {
        Err(AllpError::CommandFailed { stderr, .. })
            if stderr.contains("Package(s) not found:") =>
        {
            Ok(None)
        }
        result => result,
    }
}

fn python_pip_capture_if_available(
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    args: &[&str],
) -> AllpResult<Option<String>> {
    let command = python_pip_command(commands, args)?;
    let backend = PythonBackend;
    match capture_checked_with_privilege(
        &backend,
        runner,
        command,
        PrivilegeRequirement::OriginalUserRequired,
    ) {
        Err(AllpError::CommandFailed { stderr, .. }) if is_missing_pip_error(&stderr) => Ok(None),
        result => result.map(Some),
    }
}

fn is_missing_pip_error(stderr: &str) -> bool {
    let value = stderr.to_ascii_lowercase();
    value.contains("no module named pip") || value.contains("ensurepip")
}

fn python_pip_command(commands: &CommandMap, args: &[&str]) -> AllpResult<NativeCommand> {
    if let Some((_, python)) = python_environment(commands)? {
        return Ok(NativeCommand::new(python)
            .args(["-m", "pip"])
            .args(args.iter().copied()));
    }
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

fn installer_scope(commands: &CommandMap, installer: &str) -> AllpResult<String> {
    if installer != "pip" {
        return Ok(format!("isolated user tool via {installer}"));
    }
    Ok(match python_environment(commands)? {
        Some((root, _)) => format!("virtual environment · {}", root.display()),
        None => python_scope(),
    })
}

fn python_environment(commands: &CommandMap) -> AllpResult<Option<(PathBuf, PathBuf)>> {
    if let Some(active) = env::var_os("VIRTUAL_ENV") {
        return resolve_python_environment(PathBuf::from(active)).map(Some);
    }
    if let Some(root) = commands
        .get("python")
        .and_then(|path| path.parent()?.parent())
    {
        if root.join("pyvenv.cfg").is_file() {
            return resolve_python_environment(root.to_owned()).map(Some);
        }
    }
    let current = env::current_dir()?;
    for directory in current.ancestors() {
        let project = ["pyproject.toml", "setup.py", "setup.cfg"]
            .iter()
            .any(|marker| directory.join(marker).is_file());
        if directory == current || project {
            let local = directory.join(".venv");
            if local.join("pyvenv.cfg").is_file() {
                return resolve_python_environment(local).map(Some);
            }
        }
        if project || directory.join(".git").exists() {
            break;
        }
    }
    Ok(None)
}

fn resolve_python_environment(root: PathBuf) -> AllpResult<(PathBuf, PathBuf)> {
    let interpreters = if cfg!(windows) {
        ["Scripts/python.exe", "Scripts/python3.exe"]
    } else {
        ["bin/python3", "bin/python"]
    };
    if root.is_absolute() && root.join("pyvenv.cfg").is_file() {
        for interpreter in interpreters {
            let python = root.join(interpreter);
            if python.is_file() {
                return Ok((root, python));
            }
        }
    }
    Err(AllpError::InvalidInput(format!(
        "{} does not point to a valid Python virtual environment; activate the correct environment or recreate it",
        root.display()
    )))
}

#[derive(Deserialize, Serialize)]
struct PyPiProject {
    name: String,
    version: String,
    summary: Option<String>,
}

fn pypi_project(
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    package_id: &str,
) -> AllpResult<Option<PyPiProject>> {
    let Some(name) = normalized_pypi_name(package_id) else {
        return Ok(None);
    };
    let command = NativeCommand::new(command_path(&PythonBackend, commands, "python")?)
        .args(["-I", "-c", PYPI_LOOKUP, &name])
        .timeout(Duration::from_secs(15));
    let output = capture_checked_with_privilege(
        &PythonBackend,
        runner,
        command,
        PrivilegeRequirement::OriginalUserRequired,
    )?;
    let project: Option<PyPiProject> =
        serde_json::from_str(&output).map_err(|error| pypi_parse_error(error.to_string()))?;
    if let Some(project) = &project {
        if normalized_pypi_name(&project.name).as_deref() != Some(&name)
            || project.version.is_empty()
        {
            return Err(pypi_parse_error(
                "project metadata did not match the requested package",
            ));
        }
    }
    Ok(project)
}

fn require_pypi_project(
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    package_id: &str,
) -> AllpResult<PyPiProject> {
    pypi_project(commands, runner, package_id)?
        .ok_or_else(|| AllpError::PackageNotFound(package_id.to_owned()))
}

fn pypi_parse_error(message: impl Into<String>) -> AllpError {
    AllpError::Parse {
        backend: "PyPI".to_owned(),
        message: message.into(),
    }
}

fn normalized_pypi_name(name: &str) -> Option<String> {
    let mut normalized = String::new();
    for byte in name.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' => {
                normalized.push(byte.to_ascii_lowercase() as char)
            }
            b'-' | b'_' | b'.' if !normalized.is_empty() => {
                if !normalized.ends_with('-') {
                    normalized.push('-');
                }
            }
            _ => return None,
        }
    }
    (!normalized.is_empty() && !normalized.ends_with('-')).then_some(normalized)
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

fn installer_choices(commands: &CommandMap, pip_available: bool) -> Vec<String> {
    let mut choices = Vec::new();
    if pip_available {
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
        scope: None,
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

#[cfg(test)]
mod search_contract_tests {
    use super::{parse_pypi_candidates, PythonBackend};

    #[test]
    fn unknown_pip_text_does_not_parse_as_a_package() {
        let candidates = parse_pypi_candidates(
            &PythonBackend,
            "future machine-readable format enabled",
            "demo",
            vec!["pip".to_owned()],
        );

        assert!(candidates.is_empty());
    }
}
