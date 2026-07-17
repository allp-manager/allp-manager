use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked, match_kind, parse_key_value_lines},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpResult, BackendCategory, Capability, DeveloperTarget, ExecutionPlan, InstalledPackage,
        MaintenancePlan, NativeCommand, OperationKind, PackageCandidate, PackageDomain,
        PackageInfo, PrivilegeRequirement,
    },
    execution::ProcessRunner,
};
use std::path::Path;

#[derive(Clone, Copy)]
pub struct SystemFamilyBackend {
    config: &'static SystemFamilyConfig,
}

impl SystemFamilyBackend {
    pub const fn new(config: &'static SystemFamilyConfig) -> Self {
        Self { config }
    }
}

pub struct SystemFamilyConfig {
    pub id: &'static str,
    pub display_name: &'static str,
    pub source: &'static str,
    pub requirements: &'static [CommandRequirement],
    pub capabilities: &'static [Capability],
    pub search: Option<QueryTemplate>,
    pub list: Option<QueryTemplate>,
    pub info: Option<QueryTemplate>,
    pub install: Option<PackageTemplate>,
    pub remove: Option<PackageTemplate>,
    pub update: Option<BulkTemplate>,
    pub upgrade: Option<BulkTemplate>,
}

#[derive(Clone, Copy)]
pub struct QueryTemplate {
    pub key: &'static str,
    pub args: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub struct PackageTemplate {
    pub key: &'static str,
    pub args: &'static [&'static str],
    pub action: &'static str,
}

#[derive(Clone, Copy)]
pub struct BulkTemplate {
    pub key: &'static str,
    pub args: &'static [&'static str],
    pub action: &'static str,
    pub source: &'static str,
}

impl Backend for SystemFamilyBackend {
    fn id(&self) -> &'static str {
        self.config.id
    }

    fn display_name(&self) -> &'static str {
        self.config.display_name
    }

    fn category(&self) -> BackendCategory {
        BackendCategory::System
    }

    fn capabilities(&self) -> &'static [Capability] {
        self.config.capabilities
    }

    fn command_requirements(&self) -> &'static [CommandRequirement] {
        self.config.requirements
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        let Some(template) = self.config.search else {
            return Err(self.unsupported("search"));
        };
        let program = command_path(self, commands, template.key)?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(program).args(template.args).arg(query),
        )?;
        Ok(parse_search(self, &output, query))
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let Some(template) = self.config.list else {
            return Err(self.unsupported("list"));
        };
        let program = command_path(self, commands, template.key)?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(program).args(template.args),
        )?;
        Ok(parse_installed(self, &output))
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        let Some(template) = self.config.info else {
            return Err(self.unsupported("info"));
        };
        let program = command_path(self, commands, template.key)?;
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(program)
                .args(template.args)
                .arg(package_id),
        )?;
        Ok(info_from_output(self, package_id, &output))
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        let Some(template) = self.config.info else {
            return Err(self.unsupported("raw info"));
        };
        let program = command_path(self, commands, template.key)?;
        capture_checked(
            self,
            runner,
            NativeCommand::new(program)
                .args(template.args)
                .arg(package_id),
        )
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        let Some(template) = self.config.install else {
            return Err(self.unsupported("install"));
        };
        let program = command_path(self, commands, template.key)?;
        Ok(package_plan(
            self,
            PackagePlanInput {
                program,
                operation: OperationKind::Install,
                action: template.action,
                args: template.args,
                package_id: &candidate.package_id,
                source: candidate.source.clone(),
                scope: candidate.scope.clone(),
            },
        ))
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        let Some(template) = self.config.remove else {
            return Err(self.unsupported("remove"));
        };
        let program = command_path(self, commands, template.key)?;
        Ok(package_plan(
            self,
            PackagePlanInput {
                program,
                operation: OperationKind::Remove,
                action: template.action,
                args: template.args,
                package_id: &package.package_id,
                source: package.source.clone(),
                scope: package.scope.clone(),
            },
        ))
    }

    fn plan_update(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let Some(template) = self.config.update else {
            return Err(self.unsupported("update"));
        };
        Ok(MaintenancePlan::from_plans(vec![bulk_plan(
            self,
            commands,
            OperationKind::Update,
            template,
        )?]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let Some(template) = self.config.upgrade else {
            return Err(self.unsupported("upgrade"));
        };
        Ok(MaintenancePlan::from_plans(vec![bulk_plan(
            self,
            commands,
            OperationKind::Upgrade,
            template,
        )?]))
    }
}

struct PackagePlanInput<'a> {
    program: &'a Path,
    operation: OperationKind,
    action: &'a str,
    args: &'a [&'a str],
    package_id: &'a str,
    source: Option<String>,
    scope: Option<String>,
}

fn package_plan(backend: &SystemFamilyBackend, input: PackagePlanInput<'_>) -> ExecutionPlan {
    let mut command = NativeCommand::new(input.program).args(input.args.iter().copied());
    command = command.arg(input.package_id);
    ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation: input.operation,
        action: input.action.to_owned(),
        package_id: Some(input.package_id.to_owned()),
        source: input.source,
        scope: input.scope,
        details: Vec::new(),
        command,
        privilege: PrivilegeRequirement::RootRequired,
        requires_root: true,
        interactive: true,
    }
}

fn bulk_plan(
    backend: &SystemFamilyBackend,
    commands: &CommandMap,
    operation: OperationKind,
    template: BulkTemplate,
) -> AllpResult<ExecutionPlan> {
    let program = command_path(backend, commands, template.key)?;
    Ok(ExecutionPlan {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation,
        action: template.action.to_owned(),
        package_id: None,
        source: Some(template.source.to_owned()),
        scope: Some("system".to_owned()),
        details: Vec::new(),
        command: NativeCommand::new(program).args(template.args.iter().copied()),
        privilege: PrivilegeRequirement::RootRequired,
        requires_root: true,
        interactive: true,
    })
}

fn parse_search(backend: &SystemFamilyBackend, output: &str, query: &str) -> Vec<PackageCandidate> {
    output
        .lines()
        .filter_map(|line| parse_candidate_line(backend, line, query))
        .collect()
}

fn parse_candidate_line(
    backend: &SystemFamilyBackend,
    line: &str,
    query: &str,
) -> Option<PackageCandidate> {
    let line = line.trim();
    if line.is_empty()
        || line.starts_with("Loading ")
        || line.starts_with("Repository ")
        || line.starts_with("S |")
        || line.starts_with("Name ")
        || line.starts_with("Available ")
        || line.starts_with("Installed ")
    {
        return None;
    }

    let (raw_id, description, source) = if line.contains('|') {
        let columns = line
            .split('|')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let id = columns
            .iter()
            .find(|part| !matches!(**part, "i" | "v" | "+" | "-"))
            .copied()?;
        let description = columns.get(1).map(|value| (*value).to_owned());
        (
            id.to_owned(),
            description,
            Some(backend.config.source.to_owned()),
        )
    } else if let Some((left, right)) = line.split_once(" - ") {
        (left.trim().to_owned(), Some(right.trim().to_owned()), None)
    } else if let Some((left, right)) = line.split_once(" : ") {
        (left.trim().to_owned(), Some(right.trim().to_owned()), None)
    } else {
        let mut parts = line.split_whitespace();
        let first = parts.next()?;
        (
            first.to_owned(),
            Some(parts.collect::<Vec<_>>().join(" ")),
            None,
        )
    };

    let (package_id, source) = split_repo_prefix(&raw_id, source);
    let package_id = strip_version_suffix(&package_id);
    if package_id.is_empty() {
        return None;
    }

    let candidate_match = match_kind(&package_id, query);
    Some(PackageCandidate {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        category: backend.category(),
        domain: PackageDomain::System,
        package_id: package_id.clone(),
        display_name: package_id.clone(),
        version: None,
        description: description.filter(|value| !value.is_empty()),
        source: source.or_else(|| Some(backend.config.source.to_owned())),
        installers: vec![backend.display_name().to_owned()],
        artifact_kind: "system package".to_owned(),
        scope: Some("system".to_owned()),
        match_kind: candidate_match,
        identity: PackageCandidate::infer_identity(
            candidate_match,
            PackageDomain::System,
            "system package",
        ),
        metadata: Default::default(),
    })
}

fn parse_installed(backend: &SystemFamilyBackend, output: &str) -> Vec<InstalledPackage> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with("Loading ")
                || line.starts_with("S |")
                || line.starts_with("Name ")
            {
                return None;
            }
            let columns = line
                .split_whitespace()
                .filter(|part| !matches!(*part, "ii" | "i" | "[installed]"))
                .collect::<Vec<_>>();
            let raw_id = columns.first()?;
            let package_id = strip_version_suffix(raw_id);
            Some(InstalledPackage {
                backend_id: backend.id().to_owned(),
                backend_name: backend.display_name().to_owned(),
                category: backend.category(),
                domain: PackageDomain::System,
                package_id: package_id.clone(),
                display_name: package_id,
                version: columns.get(1).map(|value| (*value).to_owned()),
                description: None,
                source: Some(backend.config.source.to_owned()),
                scope: Some("system".to_owned()),
            })
        })
        .collect()
}

fn info_from_output(backend: &SystemFamilyBackend, package_id: &str, output: &str) -> PackageInfo {
    let fields = parse_key_value_lines(output);
    PackageInfo {
        backend_id: backend.id().to_owned(),
        backend_name: backend.display_name().to_owned(),
        category: backend.category(),
        domain: PackageDomain::System,
        package_id: fields
            .get("Name")
            .or_else(|| fields.get("Package"))
            .cloned()
            .unwrap_or_else(|| package_id.to_owned()),
        display_name: fields
            .get("Name")
            .or_else(|| fields.get("Package"))
            .cloned()
            .unwrap_or_else(|| package_id.to_owned()),
        version: fields.get("Version").cloned(),
        description: fields
            .get("Summary")
            .or_else(|| fields.get("Description"))
            .cloned()
            .or_else(|| first_nonempty_line(output)),
        source: fields
            .get("Repository")
            .or_else(|| fields.get("From repo"))
            .cloned()
            .or_else(|| Some(backend.config.source.to_owned())),
        scope: Some("system".to_owned()),
        artifact_kind: Some("system package".to_owned()),
        installed: None,
        extra: fields.into_iter().collect(),
    }
}

fn split_repo_prefix(value: &str, fallback: Option<String>) -> (String, Option<String>) {
    if let Some((repo, package_id)) = value.split_once('/') {
        (package_id.to_owned(), Some(repo.to_owned()))
    } else {
        (value.to_owned(), fallback)
    }
}

fn strip_version_suffix(value: &str) -> String {
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() > 1
        && parts.last().is_some_and(|last| {
            last.chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
        })
    {
        parts[..parts.len() - 1].join("-")
    } else {
        value.to_owned()
    }
}

fn first_nonempty_line(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

const ZYPPER_REQ: &[CommandRequirement] = &[CommandRequirement {
    key: "zypper",
    alternatives: &["zypper"],
}];
const APK_REQ: &[CommandRequirement] = &[CommandRequirement {
    key: "apk",
    alternatives: &["apk"],
}];
const XBPS_REQ: &[CommandRequirement] = &[
    CommandRequirement {
        key: "xbps-query",
        alternatives: &["xbps-query"],
    },
    CommandRequirement {
        key: "xbps-install",
        alternatives: &["xbps-install"],
    },
    CommandRequirement {
        key: "xbps-remove",
        alternatives: &["xbps-remove"],
    },
];
const EMERGE_REQ: &[CommandRequirement] = &[CommandRequirement {
    key: "emerge",
    alternatives: &["emerge"],
}];
const EOPKG_REQ: &[CommandRequirement] = &[CommandRequirement {
    key: "eopkg",
    alternatives: &["eopkg"],
}];
const SWUPD_REQ: &[CommandRequirement] = &[CommandRequirement {
    key: "swupd",
    alternatives: &["swupd"],
}];

const FULL_CAPS: &[Capability] = &[
    Capability::Search,
    Capability::Install,
    Capability::Remove,
    Capability::Update,
    Capability::Upgrade,
    Capability::List,
    Capability::Info,
];
const PORTAGE_CAPS: &[Capability] = &[
    Capability::Search,
    Capability::Install,
    Capability::Update,
    Capability::Upgrade,
    Capability::Info,
];

pub const ZYPPER: SystemFamilyConfig = SystemFamilyConfig {
    id: "zypper",
    display_name: "Zypper",
    source: "Zypper repositories",
    requirements: ZYPPER_REQ,
    capabilities: FULL_CAPS,
    search: Some(QueryTemplate {
        key: "zypper",
        args: &["--non-interactive", "search", "--details"],
    }),
    list: Some(QueryTemplate {
        key: "zypper",
        args: &["--non-interactive", "search", "--installed-only"],
    }),
    info: Some(QueryTemplate {
        key: "zypper",
        args: &["--non-interactive", "info"],
    }),
    install: Some(PackageTemplate {
        key: "zypper",
        args: &["--non-interactive", "install", "--"],
        action: "Install system package",
    }),
    remove: Some(PackageTemplate {
        key: "zypper",
        args: &["--non-interactive", "remove", "--"],
        action: "Remove system package",
    }),
    update: Some(BulkTemplate {
        key: "zypper",
        args: &["--non-interactive", "refresh"],
        action: "Refresh repository metadata",
        source: "Zypper repositories",
    }),
    upgrade: Some(BulkTemplate {
        key: "zypper",
        args: &["--non-interactive", "update"],
        action: "Upgrade installed Zypper packages",
        source: "Zypper repositories",
    }),
};

pub const APK: SystemFamilyConfig = SystemFamilyConfig {
    id: "apk",
    display_name: "APK",
    source: "Alpine repositories",
    requirements: APK_REQ,
    capabilities: FULL_CAPS,
    search: Some(QueryTemplate {
        key: "apk",
        args: &["search", "-v"],
    }),
    list: Some(QueryTemplate {
        key: "apk",
        args: &["info", "-v"],
    }),
    info: Some(QueryTemplate {
        key: "apk",
        args: &["info", "-a"],
    }),
    install: Some(PackageTemplate {
        key: "apk",
        args: &["add", "--"],
        action: "Install system package",
    }),
    remove: Some(PackageTemplate {
        key: "apk",
        args: &["del", "--"],
        action: "Remove system package",
    }),
    update: Some(BulkTemplate {
        key: "apk",
        args: &["update"],
        action: "Refresh package indexes",
        source: "Alpine repositories",
    }),
    upgrade: Some(BulkTemplate {
        key: "apk",
        args: &["upgrade"],
        action: "Upgrade installed APK packages",
        source: "Alpine repositories",
    }),
};

pub const XBPS: SystemFamilyConfig = SystemFamilyConfig {
    id: "xbps",
    display_name: "XBPS",
    source: "XBPS repositories",
    requirements: XBPS_REQ,
    capabilities: FULL_CAPS,
    search: Some(QueryTemplate {
        key: "xbps-query",
        args: &["-Rs"],
    }),
    list: Some(QueryTemplate {
        key: "xbps-query",
        args: &["-l"],
    }),
    info: Some(QueryTemplate {
        key: "xbps-query",
        args: &["-RS"],
    }),
    install: Some(PackageTemplate {
        key: "xbps-install",
        args: &["--"],
        action: "Install system package",
    }),
    remove: Some(PackageTemplate {
        key: "xbps-remove",
        args: &["--"],
        action: "Remove system package",
    }),
    update: Some(BulkTemplate {
        key: "xbps-install",
        args: &["-S"],
        action: "Synchronize repository indexes",
        source: "XBPS repositories",
    }),
    upgrade: Some(BulkTemplate {
        key: "xbps-install",
        args: &["-Syu"],
        action: "Synchronize repositories and upgrade installed packages",
        source: "XBPS repositories",
    }),
};

pub const PORTAGE: SystemFamilyConfig = SystemFamilyConfig {
    id: "portage",
    display_name: "Portage",
    source: "Portage tree",
    requirements: EMERGE_REQ,
    capabilities: PORTAGE_CAPS,
    search: Some(QueryTemplate {
        key: "emerge",
        args: &["--search"],
    }),
    list: None,
    info: Some(QueryTemplate {
        key: "emerge",
        args: &["--search"],
    }),
    install: Some(PackageTemplate {
        key: "emerge",
        args: &["--"],
        action: "Install Portage package",
    }),
    remove: None,
    update: Some(BulkTemplate {
        key: "emerge",
        args: &["--sync"],
        action: "Synchronize the Portage tree",
        source: "Portage tree",
    }),
    upgrade: Some(BulkTemplate {
        key: "emerge",
        args: &["--update", "--deep", "@world"],
        action: "Upgrade world set through Portage",
        source: "Portage tree",
    }),
};

pub const EOPKG: SystemFamilyConfig = SystemFamilyConfig {
    id: "eopkg",
    display_name: "eopkg",
    source: "Solus repositories",
    requirements: EOPKG_REQ,
    capabilities: FULL_CAPS,
    search: Some(QueryTemplate {
        key: "eopkg",
        args: &["search"],
    }),
    list: Some(QueryTemplate {
        key: "eopkg",
        args: &["list-installed"],
    }),
    info: Some(QueryTemplate {
        key: "eopkg",
        args: &["info"],
    }),
    install: Some(PackageTemplate {
        key: "eopkg",
        args: &["install", "--"],
        action: "Install system package",
    }),
    remove: Some(PackageTemplate {
        key: "eopkg",
        args: &["remove", "--"],
        action: "Remove system package",
    }),
    update: Some(BulkTemplate {
        key: "eopkg",
        args: &["update-repo"],
        action: "Refresh repository metadata",
        source: "Solus repositories",
    }),
    upgrade: Some(BulkTemplate {
        key: "eopkg",
        args: &["upgrade"],
        action: "Upgrade installed eopkg packages",
        source: "Solus repositories",
    }),
};

pub const SWUPD: SystemFamilyConfig = SystemFamilyConfig {
    id: "swupd",
    display_name: "swupd",
    source: "Clear Linux bundles",
    requirements: SWUPD_REQ,
    capabilities: FULL_CAPS,
    search: Some(QueryTemplate {
        key: "swupd",
        args: &["search"],
    }),
    list: Some(QueryTemplate {
        key: "swupd",
        args: &["bundle-list"],
    }),
    info: Some(QueryTemplate {
        key: "swupd",
        args: &["bundle-info"],
    }),
    install: Some(PackageTemplate {
        key: "swupd",
        args: &["bundle-add"],
        action: "Install Clear Linux bundle",
    }),
    remove: Some(PackageTemplate {
        key: "swupd",
        args: &["bundle-remove"],
        action: "Remove Clear Linux bundle",
    }),
    update: Some(BulkTemplate {
        key: "swupd",
        args: &["check-update"],
        action: "Check for Clear Linux updates",
        source: "Clear Linux bundles",
    }),
    upgrade: Some(BulkTemplate {
        key: "swupd",
        args: &["update"],
        action: "Update Clear Linux system bundles",
        source: "Clear Linux bundles",
    }),
};
