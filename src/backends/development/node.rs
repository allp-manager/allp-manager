use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked, match_kind},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, Capability, DeveloperTarget, ExecutionPlan,
        InstalledPackage, MaintenancePlan, NativeCommand, OperationKind, OperationStatus,
        PackageCandidate, PackageDomain, PackageInfo, PrivilegeRequirement,
        RuntimePrivilegeContext,
    },
    execution::{render_native_command, ProcessRunner},
};
use serde_json::Value;
use std::{
    env, fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub struct NodeBackend;

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
    key: "npm",
    alternatives: &["npm"],
}];

const OPTIONAL: &[CommandRequirement] = &[
    CommandRequirement {
        key: "node",
        alternatives: &["node", "nodejs"],
    },
    CommandRequirement {
        key: "pnpm",
        alternatives: &["pnpm"],
    },
    CommandRequirement {
        key: "yarn",
        alternatives: &["yarn", "yarnpkg"],
    },
    CommandRequirement {
        key: "corepack",
        alternatives: &["corepack"],
    },
];
const DOMAINS: &[PackageDomain] = &[PackageDomain::Node];

impl Backend for NodeBackend {
    fn id(&self) -> &'static str {
        "node"
    }

    fn display_name(&self) -> &'static str {
        "Node.js"
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
        &["npm", "pnpm", "yarn", "nodejs", "corepack"]
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
        let npm = command_path(self, commands, "npm")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(npm).args(["search", query, "--json", "--searchlimit=20"]),
        )?;
        let installers = installer_choices(commands);
        let mut candidates = parse_npm_search(self, &output, query, &installers);
        if candidates.is_empty() {
            candidates.push(candidate(
                self,
                query,
                None,
                None,
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
        let npm = command_path(self, commands, "npm")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(npm).args(["list", "--global", "--depth=0", "--json"]),
        )?;
        Ok(parse_npm_list(self, &output))
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        let npm = command_path(self, commands, "npm")?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(npm).args(["view", package_id, "--json"]),
        )?;
        Ok(info_from_npm_view(self, package_id, &output))
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        let npm = command_path(self, commands, "npm")?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(npm).args(["view", package_id, "--json"]),
        )
    }

    fn preflight_install(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        candidate: &PackageCandidate,
        context: &RuntimePrivilegeContext,
    ) -> AllpResult<()> {
        if context.is_root() {
            return Ok(());
        }
        if preferred_installer(candidate, commands) == "npm" {
            preflight_npm_global_prefix(self, commands, runner)?;
        }
        Ok(())
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let installer = preferred_installer(candidate, commands);
        let command = match installer.as_str() {
            "pnpm" => NativeCommand::new(command_path(self, commands, "pnpm")?).args([
                "add",
                "--global",
                candidate.package_id.as_str(),
            ]),
            "yarn" => NativeCommand::new(command_path(self, commands, "yarn")?).args([
                "global",
                "add",
                candidate.package_id.as_str(),
            ]),
            _ => NativeCommand::new(command_path(self, commands, "npm")?).args([
                "install",
                "--global",
                candidate.package_id.as_str(),
            ]),
        };
        Ok(plan(
            self,
            OperationKind::Install,
            "Install Node package as global user tool",
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
            "Remove global Node package",
            Some(package.package_id.clone()),
            NativeCommand::new(command_path(self, commands, "npm")?).args([
                "uninstall",
                "--global",
                package.package_id.as_str(),
            ]),
        ))
    }

    fn plan_update(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        selector: Option<&str>,
        target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        node_maintenance(
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
        node_maintenance(
            self,
            commands,
            runner,
            selector,
            target,
            OperationKind::Upgrade,
        )
    }
}

fn preflight_npm_global_prefix(
    backend: &NodeBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
) -> AllpResult<()> {
    let npm = command_path(backend, commands, "npm")?;
    let command = NativeCommand::new(npm).args(["config", "get", "prefix"]);
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
    let prefix = output.stdout.trim();
    if prefix.is_empty() {
        return Err(AllpError::InvalidInput(
            "npm global prefix could not be determined; refusing to install a global npm package"
                .to_owned(),
        ));
    }
    let prefix = PathBuf::from(prefix);
    if !directory_writable_by_current_user(&prefix) {
        return Err(AllpError::InvalidInput(format!(
            "npm global prefix is not writable by the current user: {}\nAllp will not retry npm global installs with sudo. Fix npm prefix ownership or configure a writable user prefix.",
            prefix.display()
        )));
    }
    Ok(())
}

fn directory_writable_by_current_user(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_dir() {
        return false;
    }
    let mode = metadata.mode();
    let (uid, gid) = current_effective_ids();
    if uid == Some(0) {
        return true;
    }
    if uid.is_some_and(|uid| metadata.uid() == uid) {
        return mode & 0o200 != 0;
    }
    if gid.is_some_and(|gid| metadata.gid() == gid) {
        return mode & 0o020 != 0;
    }
    mode & 0o002 != 0
}

fn current_effective_ids() -> (Option<u32>, Option<u32>) {
    let Ok(status) = fs::read_to_string("/proc/self/status") else {
        return (None, None);
    };
    let uid = status.lines().find_map(|line| {
        let values = line.strip_prefix("Uid:")?;
        values.split_whitespace().nth(1)?.parse::<u32>().ok()
    });
    let gid = status.lines().find_map(|line| {
        let values = line.strip_prefix("Gid:")?;
        values.split_whitespace().nth(1)?.parse::<u32>().ok()
    });
    (uid, gid)
}

fn node_maintenance(
    backend: &NodeBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    selector: Option<&str>,
    target: Option<DeveloperTarget>,
    operation: OperationKind,
) -> AllpResult<MaintenancePlan> {
    let mut output = MaintenancePlan::default();
    let target = target.unwrap_or(DeveloperTarget::All);
    let project_root = find_project_root();

    append_node_component_records(backend, commands, runner, selector, operation, &mut output)?;

    if selector_allows(selector, "npm") {
        append_npm_plans(
            backend,
            commands,
            runner,
            target,
            operation,
            project_root.as_deref(),
            &mut output,
        )?;
    }
    if selector_allows(selector, "pnpm") {
        append_pnpm_plans(
            backend,
            commands,
            runner,
            target,
            operation,
            project_root.as_deref(),
            &mut output,
        )?;
    }
    if selector_allows(selector, "yarn") {
        append_yarn_plans(
            backend,
            commands,
            runner,
            target,
            operation,
            project_root.as_deref(),
            &mut output,
        )?;
    }

    if output.plans.is_empty() && output.records.is_empty() {
        output.records.push(node_record(
            backend,
            "Node.js",
            crate::domain::OperationStatus::NotApplicable,
            "target is not applicable to installed Node tools",
        ));
    }

    Ok(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeRuntimeOwner {
    OsPackageManager,
    Homebrew,
    Nvm,
    Fnm,
    Volta,
    Asdf,
    Manual,
    Unknown,
}

impl NodeRuntimeOwner {
    fn label(self) -> &'static str {
        match self {
            Self::OsPackageManager => "OS package manager",
            Self::Homebrew => "Homebrew",
            Self::Nvm => "nvm",
            Self::Fnm => "fnm",
            Self::Volta => "Volta",
            Self::Asdf => "asdf",
            Self::Manual => "manual installation",
            Self::Unknown => "unknown",
        }
    }

    fn safe_runtime_status(self, operation: OperationKind) -> (OperationStatus, &'static str) {
        match self {
            Self::OsPackageManager => (
                OperationStatus::NotApplicable,
                "runtime is owned by an OS package manager; use the owning system backend",
            ),
            Self::Homebrew => (
                OperationStatus::NotApplicable,
                "runtime is owned by Homebrew; use the Homebrew backend",
            ),
            Self::Manual | Self::Unknown => (
                OperationStatus::Protected,
                "ownership could not be safely automated; runtime will not be modified automatically",
            ),
            Self::Nvm | Self::Fnm | Self::Volta | Self::Asdf => match operation {
                OperationKind::Update => (
                    OperationStatus::UpToDate,
                    "runtime manager detected; update would preserve the active major/channel",
                ),
                OperationKind::Upgrade => (
                    OperationStatus::Protected,
                    "major-version changes require explicit runtime target selection",
                ),
                _ => (
                    OperationStatus::NotApplicable,
                    "runtime manager detected outside Node maintenance",
                ),
            },
        }
    }
}

fn append_node_component_records(
    backend: &NodeBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    selector: Option<&str>,
    operation: OperationKind,
    output: &mut MaintenancePlan,
) -> AllpResult<()> {
    if selector_allows_runtime(selector) {
        output.records.push(node_runtime_record(
            backend,
            commands.get("node").map(PathBuf::as_path),
            runner,
            operation,
        )?);
    }

    if selector_allows(selector, "npm") {
        output.records.push(node_cli_record(
            backend,
            "npm CLI",
            commands.get("npm").map(PathBuf::as_path),
            runner,
            &["--version"],
            true,
        )?);
    }
    if selector_allows(selector, "pnpm") {
        output.records.push(node_cli_record(
            backend,
            "pnpm CLI",
            commands.get("pnpm").map(PathBuf::as_path),
            runner,
            &["--version"],
            false,
        )?);
    }
    if selector_allows(selector, "yarn") {
        output.records.push(node_cli_record(
            backend,
            "Yarn CLI",
            commands.get("yarn").map(PathBuf::as_path),
            runner,
            &["--version"],
            false,
        )?);
    }
    if selector_allows(selector, "corepack") {
        output.records.push(node_cli_record(
            backend,
            "Corepack",
            commands.get("corepack").map(PathBuf::as_path),
            runner,
            &["--version"],
            false,
        )?);
    }

    Ok(())
}

fn node_runtime_record(
    backend: &NodeBackend,
    node: Option<&Path>,
    runner: &dyn ProcessRunner,
    operation: OperationKind,
) -> AllpResult<crate::domain::BackendOperationRecord> {
    let Some(node) = node else {
        return Ok(node_record(
            backend,
            "Node.js runtime",
            OperationStatus::Unavailable,
            "node executable not installed or not on PATH",
        ));
    };
    let version = capture_tool_version(backend, runner, node, &["--version"])?;
    let owner = detect_node_runtime_owner(node);
    let installed_versions = detect_installed_node_versions(node);
    let (status, reason) = owner.safe_runtime_status(operation);
    let mut message = format!(
        "{} · active: yes · default: yes · owner: {} · path: {} · {reason}",
        version.unwrap_or_else(|| "version unknown".to_owned()),
        owner.label(),
        node.display()
    );
    if !installed_versions.is_empty() {
        message.push_str(" · installed versions: ");
        message.push_str(&installed_versions.join(", "));
    }
    Ok(node_record(backend, "Node.js runtime", status, &message))
}

fn node_cli_record(
    backend: &NodeBackend,
    target_name: &str,
    command_path: Option<&Path>,
    runner: &dyn ProcessRunner,
    version_args: &[&str],
    required: bool,
) -> AllpResult<crate::domain::BackendOperationRecord> {
    let Some(command_path) = command_path else {
        let status = if required {
            OperationStatus::Protected
        } else {
            OperationStatus::Unavailable
        };
        return Ok(node_record(
            backend,
            target_name,
            status,
            "CLI not installed or not on PATH",
        ));
    };
    let version = capture_tool_version(backend, runner, command_path, version_args)?
        .unwrap_or_else(|| "version unknown".to_owned());
    Ok(node_record(
        backend,
        target_name,
        OperationStatus::UpToDate,
        &format!(
            "{version} · CLI self-update is separate from global packages, projects, and workspaces"
        ),
    ))
}

fn capture_tool_version(
    backend: &NodeBackend,
    runner: &dyn ProcessRunner,
    executable: &Path,
    args: &[&str],
) -> AllpResult<Option<String>> {
    let command = NativeCommand::new(executable).args(args.iter().copied());
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
    Ok(output
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned))
}

fn detect_node_runtime_owner(path: &Path) -> NodeRuntimeOwner {
    let path = resolve_symlink_chain(path);
    let value = path.to_string_lossy().to_ascii_lowercase();
    if value.contains("/.nvm/versions/node/") {
        NodeRuntimeOwner::Nvm
    } else if value.contains("/.fnm/") || value.contains("/fnm/node-versions/") {
        NodeRuntimeOwner::Fnm
    } else if value.contains("/.volta/") {
        NodeRuntimeOwner::Volta
    } else if value.contains("/.asdf/") || value.contains("/asdf/installs/nodejs/") {
        NodeRuntimeOwner::Asdf
    } else if value.contains("/homebrew/") || value.contains("/linuxbrew/") {
        NodeRuntimeOwner::Homebrew
    } else if value.starts_with("/usr/bin/")
        || value.starts_with("/usr/local/bin/")
        || value.starts_with("/bin/")
    {
        NodeRuntimeOwner::OsPackageManager
    } else if value.ends_with("/bin/node") || value.ends_with("/bin/nodejs") {
        NodeRuntimeOwner::Manual
    } else {
        NodeRuntimeOwner::Unknown
    }
}

fn resolve_symlink_chain(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    for _ in 0..8 {
        let Ok(target) = fs::read_link(&current) else {
            break;
        };
        current = if target.is_absolute() {
            target
        } else {
            current
                .parent()
                .map(|parent| parent.join(&target))
                .unwrap_or(target)
        };
    }
    current
}

fn detect_installed_node_versions(path: &Path) -> Vec<String> {
    let resolved = resolve_symlink_chain(path);
    let components = resolved
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    components
        .iter()
        .filter(|component| looks_like_node_version(component))
        .map(|component| (*component).to_owned())
        .collect()
}

fn looks_like_node_version(value: &str) -> bool {
    let value = value.strip_prefix('v').unwrap_or(value);
    let mut parts = value.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(major), Some(minor), Some(patch))
            if major.chars().all(|character| character.is_ascii_digit())
                && minor.chars().all(|character| character.is_ascii_digit())
                && patch.chars().all(|character| character.is_ascii_digit())
    )
}

fn append_npm_plans(
    backend: &NodeBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    target: DeveloperTarget,
    operation: OperationKind,
    project_root: Option<&Path>,
    output: &mut MaintenancePlan,
) -> AllpResult<()> {
    let npm = command_path(backend, commands, "npm")?;
    if target.includes(DeveloperTarget::Project) {
        if let Some(root) = project_root {
            match npm_outdated(backend, runner, npm, Some(root), false)? {
                OutdatedState::Outdated => {
                    let mut command = NativeCommand::new(npm).arg("update");
                    command.current_dir = Some(root.to_path_buf());
                    let action = match operation {
                        OperationKind::Update => {
                            "Update npm project packages within declared version ranges"
                        }
                        OperationKind::Upgrade => {
                            "Update npm project packages; latest crossing is not automatic"
                        }
                        _ => unreachable!("node maintenance only handles update and upgrade"),
                    };
                    output.plans.push(node_plan(
                        backend,
                        "npm project",
                        operation,
                        action,
                        Some("npm registry".to_owned()),
                        Some(format!(
                            "project · {} · affects package.json, package-lock.json, node_modules",
                            root.display()
                        )),
                        command,
                    ));
                }
                OutdatedState::None => output.records.push(node_record(
                    backend,
                    "npm project",
                    crate::domain::OperationStatus::UpToDate,
                    "no outdated packages found after npm outdated --json inspection",
                )),
            }
        } else {
            output.records.push(node_record(
                backend,
                "npm project",
                crate::domain::OperationStatus::NotApplicable,
                "no project manifest found in the current directory or its parents",
            ));
        }
    }

    if target.includes(DeveloperTarget::Global) {
        match npm_outdated(backend, runner, npm, None, true)? {
            OutdatedState::Outdated => output.plans.push(node_plan(
                backend,
                "npm global",
                operation,
                "Update npm global user packages within allowed version ranges",
                Some("npm registry".to_owned()),
                Some("global user packages".to_owned()),
                NativeCommand::new(npm).args(["update", "--global"]),
            )),
            OutdatedState::None => output.records.push(node_record(
                backend,
                "npm global",
                crate::domain::OperationStatus::UpToDate,
                "no globally installed outdated npm packages found",
            )),
        }
    }

    Ok(())
}

fn append_pnpm_plans(
    backend: &NodeBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    target: DeveloperTarget,
    operation: OperationKind,
    project_root: Option<&Path>,
    output: &mut MaintenancePlan,
) -> AllpResult<()> {
    let Some(pnpm) = commands.get("pnpm").map(PathBuf::as_path) else {
        if target.includes(DeveloperTarget::Project)
            || target.includes(DeveloperTarget::Workspace)
            || target.includes(DeveloperTarget::Global)
        {
            output.records.push(node_record(
                backend,
                "pnpm",
                crate::domain::OperationStatus::Unavailable,
                "backend not installed",
            ));
        }
        return Ok(());
    };

    if target.includes(DeveloperTarget::Project) {
        if let Some(root) = project_root {
            inspect_pnpm(backend, runner, pnpm, root)?;
            let mut command = NativeCommand::new(pnpm).arg("update");
            if operation == OperationKind::Upgrade {
                command = command.arg("--latest");
            }
            command.current_dir = Some(root.to_path_buf());
            output.plans.push(node_plan(
                backend,
                "pnpm project",
                operation,
                if operation == OperationKind::Upgrade {
                    "Update pnpm project packages to latest versions"
                } else {
                    "Update pnpm project packages within declared ranges"
                },
                Some("npm registry".to_owned()),
                Some(format!(
                    "project · {} · affects package.json, pnpm-lock.yaml, node_modules",
                    root.display()
                )),
                command,
            ));
        } else {
            output.records.push(node_record(
                backend,
                "pnpm project",
                crate::domain::OperationStatus::NotApplicable,
                "no project manifest found in the current directory or its parents",
            ));
        }
    }

    if target.includes(DeveloperTarget::Workspace) {
        if let Some(root) = project_root {
            if is_pnpm_workspace(root) {
                inspect_pnpm(backend, runner, pnpm, root)?;
                let mut command = NativeCommand::new(pnpm).arg("update");
                if operation == OperationKind::Upgrade {
                    command = command.arg("--latest");
                }
                command.current_dir = Some(root.to_path_buf());
                output.plans.push(node_plan(
                    backend,
                    "pnpm workspace",
                    operation,
                    "Update selected pnpm workspace packages",
                    Some("npm registry".to_owned()),
                    Some(format!(
                        "workspace · {} · affects workspace manifests and pnpm-lock.yaml",
                        root.display()
                    )),
                    command,
                ));
            } else {
                output.records.push(node_record(
                    backend,
                    "pnpm workspace",
                    crate::domain::OperationStatus::NotApplicable,
                    "no pnpm workspace manifest found",
                ));
            }
        } else {
            output.records.push(node_record(
                backend,
                "pnpm workspace",
                crate::domain::OperationStatus::NotApplicable,
                "no project manifest found in the current directory or its parents",
            ));
        }
    }

    if target.includes(DeveloperTarget::Global) {
        let mut command = NativeCommand::new(pnpm).args(["update", "--global"]);
        if operation == OperationKind::Upgrade {
            command = command.arg("--latest");
        }
        output.plans.push(node_plan(
            backend,
            "pnpm global",
            operation,
            if operation == OperationKind::Upgrade {
                "Update pnpm global packages to latest versions"
            } else {
                "Update pnpm global packages"
            },
            Some("npm registry".to_owned()),
            Some("global user packages".to_owned()),
            command,
        ));
    }

    Ok(())
}

fn append_yarn_plans(
    backend: &NodeBackend,
    commands: &CommandMap,
    runner: &dyn ProcessRunner,
    target: DeveloperTarget,
    operation: OperationKind,
    project_root: Option<&Path>,
    output: &mut MaintenancePlan,
) -> AllpResult<()> {
    let Some(yarn) = commands.get("yarn").map(PathBuf::as_path) else {
        if target.includes(DeveloperTarget::Project) || target.includes(DeveloperTarget::Workspace)
        {
            output.records.push(node_record(
                backend,
                "Yarn",
                crate::domain::OperationStatus::Unavailable,
                "backend not installed",
            ));
        }
        return Ok(());
    };

    if target.includes(DeveloperTarget::Global) {
        output.records.push(node_record(
            backend,
            "Yarn global",
            crate::domain::OperationStatus::NotApplicable,
            "capability unsupported by detected backend version",
        ));
    }

    if !target.includes(DeveloperTarget::Project) && !target.includes(DeveloperTarget::Workspace) {
        return Ok(());
    }

    let Some(root) = project_root else {
        output.records.push(node_record(
            backend,
            "Yarn project",
            crate::domain::OperationStatus::NotApplicable,
            "no project manifest found in the current directory or its parents",
        ));
        return Ok(());
    };

    if !uses_yarn(root) && selector_allows(None, "npm") {
        output.records.push(node_record(
            backend,
            "Yarn project",
            crate::domain::OperationStatus::NotApplicable,
            "no Yarn lockfile or packageManager marker found",
        ));
        return Ok(());
    }

    let major = yarn_major_version(backend, runner, yarn)?;
    let mut command = NativeCommand::new(yarn);
    let action = match (major, operation) {
        (Some(1), OperationKind::Update) => {
            command = command.arg("upgrade");
            "Update Yarn 1 project dependencies"
        }
        (Some(1), OperationKind::Upgrade) => {
            command = command.args(["upgrade", "--latest"]);
            "Update Yarn 1 project dependencies to latest versions"
        }
        (_, OperationKind::Update) => {
            command = command.arg("up");
            "Update modern Yarn project dependencies"
        }
        (_, OperationKind::Upgrade) => {
            command = command.args(["up", "*"]);
            "Update modern Yarn project dependencies to latest versions"
        }
        _ => unreachable!("node maintenance only handles update and upgrade"),
    };
    command.current_dir = Some(root.to_path_buf());
    output.plans.push(node_plan(
        backend,
        if target.includes(DeveloperTarget::Workspace) {
            "Yarn workspace"
        } else {
            "Yarn project"
        },
        operation,
        action,
        Some("npm registry".to_owned()),
        Some(format!(
            "project · {} · affects package.json, yarn.lock, .yarn state",
            root.display()
        )),
        command,
    ));
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutdatedState {
    Outdated,
    None,
}

fn npm_outdated(
    backend: &NodeBackend,
    runner: &dyn ProcessRunner,
    npm: &Path,
    project_root: Option<&Path>,
    global: bool,
) -> AllpResult<OutdatedState> {
    let mut command = NativeCommand::new(npm);
    if global {
        command = command.args(["outdated", "--global", "--depth=0", "--json"]);
    } else {
        command = command.args(["outdated", "--json"]);
    }
    if let Some(root) = project_root {
        command.current_dir = Some(root.to_path_buf());
    }
    let rendered = render_native_command(&command);
    let output = runner.capture(&command)?;
    if !output.success && output.code != Some(1) {
        return Err(AllpError::CommandFailed {
            backend: backend.display_name().to_owned(),
            command: rendered,
            code: output.code,
            stderr: output.stderr,
        });
    }
    Ok(if json_object_has_entries(&output.stdout) {
        OutdatedState::Outdated
    } else {
        OutdatedState::None
    })
}

fn inspect_pnpm(
    backend: &NodeBackend,
    runner: &dyn ProcessRunner,
    pnpm: &Path,
    root: &Path,
) -> AllpResult<()> {
    let mut command = NativeCommand::new(pnpm).args(["outdated", "--format", "json"]);
    command.current_dir = Some(root.to_path_buf());
    let rendered = render_native_command(&command);
    let output = runner.capture(&command)?;
    if output.success || output.code == Some(1) {
        Ok(())
    } else {
        Err(AllpError::CommandFailed {
            backend: backend.display_name().to_owned(),
            command: rendered,
            code: output.code,
            stderr: output.stderr,
        })
    }
}

fn yarn_major_version(
    backend: &NodeBackend,
    runner: &dyn ProcessRunner,
    yarn: &Path,
) -> AllpResult<Option<u64>> {
    let command = NativeCommand::new(yarn).arg("--version");
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
    Ok(output
        .stdout
        .trim()
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok()))
}

fn json_object_has_entries(output: &str) -> bool {
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

fn find_project_root() -> Option<PathBuf> {
    let mut current = env::current_dir().ok()?;
    loop {
        if current.join("package.json").is_file() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn is_pnpm_workspace(root: &Path) -> bool {
    root.join("pnpm-workspace.yaml").is_file()
}

fn uses_yarn(root: &Path) -> bool {
    if root.join("yarn.lock").is_file() {
        return true;
    }
    let Ok(package_json) = fs::read_to_string(root.join("package.json")) else {
        return false;
    };
    serde_json::from_str::<Value>(&package_json)
        .ok()
        .and_then(|value| {
            value
                .get("packageManager")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|value| value.starts_with("yarn@"))
}

fn selector_allows(selector: Option<&str>, installer: &str) -> bool {
    let Some(selector) = selector else {
        return true;
    };
    let selector = selector.to_ascii_lowercase();
    selector == "node" || selector == installer
}

fn selector_allows_runtime(selector: Option<&str>) -> bool {
    let Some(selector) = selector else {
        return true;
    };
    matches!(
        selector.to_ascii_lowercase().as_str(),
        "node" | "nodejs" | "node.js"
    )
}

fn node_record(
    backend: &NodeBackend,
    target_name: &str,
    status: crate::domain::OperationStatus,
    reason: &str,
) -> crate::domain::BackendOperationRecord {
    MaintenancePlan::record(backend.id(), target_name, status, reason)
}

fn node_plan(
    backend: &NodeBackend,
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
        command,
        privilege: PrivilegeRequirement::OriginalUserRequired,
        requires_root: false,
        interactive: true,
    }
}

fn parse_npm_search(
    backend: &NodeBackend,
    output: &str,
    query: &str,
    installers: &[String],
) -> Vec<PackageCandidate> {
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(output) {
        return items
            .iter()
            .filter_map(|item| {
                let package_id = item.get("name")?.as_str()?;
                Some(candidate(
                    backend,
                    package_id,
                    item.get("version")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    item.get("description")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    match_kind(package_id, query),
                    installers.to_vec(),
                ))
            })
            .collect();
    }

    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let package_id = columns.next()?;
            Some(candidate(
                backend,
                package_id,
                None,
                Some(columns.collect::<Vec<_>>().join(" ")),
                match_kind(package_id, query),
                installers.to_vec(),
            ))
        })
        .collect()
}

fn parse_npm_list(backend: &NodeBackend, output: &str) -> Vec<InstalledPackage> {
    let Ok(json) = serde_json::from_str::<Value>(output) else {
        return Vec::new();
    };
    let Some(dependencies) = json.get("dependencies").and_then(Value::as_object) else {
        return Vec::new();
    };
    dependencies
        .iter()
        .map(|(package_id, value)| InstalledPackage {
            backend_id: backend.id().to_owned(),
            backend_name: backend.display_name().to_owned(),
            category: backend.category(),
            domain: PackageDomain::Node,
            package_id: package_id.clone(),
            display_name: package_id.clone(),
            version: value
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_owned),
            description: None,
            source: Some("npm registry".to_owned()),
            scope: Some("global user tool".to_owned()),
        })
        .collect()
}

fn info_from_npm_view(backend: &NodeBackend, package_id: &str, output: &str) -> PackageInfo {
    let json = serde_json::from_str::<Value>(output).unwrap_or(Value::Null);
    PackageInfo {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        category: backend.category(),
        domain: PackageDomain::Node,
        package_id: json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(package_id)
            .to_owned(),
        display_name: json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(package_id)
            .to_owned(),
        version: json
            .get("version")
            .and_then(Value::as_str)
            .map(str::to_owned),
        description: json
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
        source: Some("npm registry".to_owned()),
        scope: Some("global user tool".to_owned()),
        artifact_kind: Some("Node package".to_owned()),
        installed: None,
        extra: vec![("Native JSON".to_owned(), output.trim().to_owned())],
    }
}

fn candidate(
    backend: &NodeBackend,
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
        domain: PackageDomain::Node,
        package_id: package_id.to_owned(),
        display_name: package_id.to_owned(),
        version,
        description,
        source: Some("npm registry".to_owned()),
        installers,
        artifact_kind: "Node package".to_owned(),
        scope: Some("global user tool".to_owned()),
        match_kind,
        identity: PackageCandidate::infer_identity(match_kind, PackageDomain::Node, "Node package"),
    }
}

fn installer_choices(commands: &CommandMap) -> Vec<String> {
    let mut choices = vec!["npm".to_owned()];
    if commands.contains_key("pnpm") {
        choices.push("pnpm".to_owned());
    }
    if commands.contains_key("yarn") {
        choices.push("yarn".to_owned());
    }
    choices
}

fn preferred_installer(candidate: &PackageCandidate, commands: &CommandMap) -> String {
    candidate
        .installers
        .iter()
        .find(|installer| commands.contains_key(installer.as_str()))
        .cloned()
        .unwrap_or_else(|| "npm".to_owned())
}

fn plan(
    backend: &NodeBackend,
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
        source: Some("npm registry".to_owned()),
        scope: Some("global user tool".to_owned()),
        command,
        privilege: PrivilegeRequirement::OriginalUserRequired,
        requires_root: false,
        interactive: true,
    }
}
