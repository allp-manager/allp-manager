use crate::{
    backends::{
        contract::command_path,
        util::{capture_checked, match_kind, parse_key_value_lines},
        Backend, CommandMap, CommandRequirement,
    },
    domain::{
        AllpError, AllpResult, BackendCategory, BackendSearchIssue, BackendSearchIssueKind,
        BackendSearchReport, Capability, DeveloperTarget, ExecutionPlan, InstalledPackage,
        MaintenancePlan, NativeCommand, OperationKind, OperationStatus, PackageCandidate,
        PackageDomain, PackageInfo, PrivilegeRequirement,
    },
    execution::{render_native_command, ProcessRunner},
    platform::{OperatingSystem, PlatformContext},
};
use std::{path::Path, time::Duration};
use unicode_width::UnicodeWidthChar;

pub struct WinGetBackend;

const SOURCE: &str = "winget";
const ARTIFACT: &str = "Windows application";
const NO_APPLICATIONS_FOUND: i32 = 0x8a15_0014_u32 as i32;
const UPDATE_NOT_APPLICABLE: i32 = 0x8a15_002b_u32 as i32;
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
    key: "winget",
    alternatives: &["winget.exe"],
}];

impl Backend for WinGetBackend {
    fn id(&self) -> &'static str {
        SOURCE
    }

    fn display_name(&self) -> &'static str {
        "WinGet"
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

    fn platform_incompatibility(&self, platform: &PlatformContext) -> Option<String> {
        (platform.os != OperatingSystem::Windows).then(|| "WinGet is available on Windows".into())
    }

    fn probe(&self, commands: &CommandMap, runner: &dyn ProcessRunner) -> AllpResult<()> {
        let output = capture_checked(
            self,
            runner,
            NativeCommand::new(command_path(self, commands, SOURCE)?).arg("--version"),
        )?;
        if output.lines().any(|line| {
            let version = line
                .trim()
                .trim_start_matches('\u{feff}')
                .trim_start_matches('v');
            version.split('.').next().is_some_and(|major| {
                !major.is_empty() && major.bytes().all(|byte| byte.is_ascii_digit())
            }) && version.contains('.')
        }) {
            Ok(())
        } else {
            Err(parse_error(
                "unrecognized version. Check that Windows App Installer is registered",
            ))
        }
    }

    fn search(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<BackendSearchReport> {
        crate::operations::validate_package_id(query)?;
        let program = command_path(self, commands, SOURCE)?;
        let table = read_table(
            runner,
            program,
            &["search", "--query", query, "--count", "30"],
        )?;
        let candidates = table
            .rows
            .into_iter()
            .map(|row| {
                let matched = match_kind(&row.id, query).min(match_kind(&row.name, query));
                PackageCandidate {
                    backend_id: SOURCE.to_owned(),
                    backend_name: self.display_name().to_owned(),
                    category: self.category(),
                    domain: PackageDomain::System,
                    package_id: row.id,
                    display_name: row.name,
                    version: row.version,
                    description: None,
                    source: Some(SOURCE.to_owned()),
                    installers: vec![SOURCE.to_owned()],
                    artifact_kind: ARTIFACT.to_owned(),
                    scope: None,
                    match_kind: matched,
                    identity: PackageCandidate::infer_identity(
                        matched,
                        PackageDomain::System,
                        ARTIFACT,
                    ),
                    metadata: Default::default(),
                }
            })
            .collect();
        let mut report = BackendSearchReport::complete(candidates);
        if table.rejected != 0 {
            report.issues.push(BackendSearchIssue {
                kind: BackendSearchIssueKind::UnrecognizedOutput,
                stage: Some("winget search".to_owned()),
                message: format!(
                    "{} WinGet result row(s) had truncated or unrecognized IDs",
                    table.rejected
                ),
            });
        }
        Ok(report)
    }

    fn list_installed(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
    ) -> AllpResult<Vec<InstalledPackage>> {
        let program = command_path(self, commands, SOURCE)?;
        let table = read_table(runner, program, &["list"])?;
        if table.rejected != 0 {
            return Err(parse_error(
                "the installed package table contains truncated or unrecognized IDs",
            ));
        }
        Ok(table
            .rows
            .into_iter()
            .map(|row| InstalledPackage {
                installer: Some(SOURCE.to_owned()),
                backend_id: SOURCE.to_owned(),
                backend_name: self.display_name().to_owned(),
                category: self.category(),
                domain: PackageDomain::System,
                package_id: row.id,
                display_name: row.name,
                version: row.version,
                description: None,
                source: Some(SOURCE.to_owned()),
                scope: None,
            })
            .collect())
    }

    fn info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<PackageInfo> {
        let output = self.raw_info(commands, runner, package_id)?;
        let (name, mut fields) = parse_info(&output, package_id)?;
        let version = fields.remove("Version");
        let description = fields.remove("Description");
        Ok(PackageInfo {
            backend_id: SOURCE.to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::System,
            package_id: package_id.to_owned(),
            display_name: name.to_owned(),
            version,
            description,
            source: Some(SOURCE.to_owned()),
            scope: None,
            artifact_kind: Some(ARTIFACT.to_owned()),
            installed: None,
            extra: fields.into_iter().collect(),
        })
    }

    fn raw_info(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        package_id: &str,
    ) -> AllpResult<String> {
        validate_id(package_id)?;
        let program = command_path(self, commands, SOURCE)?;
        let command = read_command(program, &["show", "--id", package_id, "--exact"]);
        let output = capture_winget(runner, &command, false)?
            .ok_or_else(|| AllpError::PackageNotFound(package_id.to_owned()))?;
        if parse_info(&output, package_id).is_err() {
            if let Ok(localized) =
                capture_checked(self, runner, command.args(["--output-locale", "en-US"]))
            {
                if parse_info(&localized, package_id).is_ok() {
                    return Ok(localized);
                }
            }
        }
        Ok(output)
    }

    fn plan_install(
        &self,
        commands: &CommandMap,
        candidate: &PackageCandidate,
    ) -> AllpResult<ExecutionPlan> {
        self.package_plan(commands, OperationKind::Install, &candidate.package_id)
    }

    fn plan_remove(
        &self,
        commands: &CommandMap,
        package: &InstalledPackage,
    ) -> AllpResult<ExecutionPlan> {
        self.package_plan(commands, OperationKind::Remove, &package.package_id)
    }

    fn plan_update(
        &self,
        commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let command = NativeCommand::new(command_path(self, commands, SOURCE)?)
            .args(["source", "update", "--name", SOURCE]);
        Ok(MaintenancePlan::from_plans(vec![plan(
            self,
            OperationKind::Update,
            "Refresh the WinGet community source",
            None,
            command,
        )]))
    }

    fn plan_upgrade(
        &self,
        commands: &CommandMap,
        runner: &dyn ProcessRunner,
        _selector: Option<&str>,
        _target: Option<DeveloperTarget>,
    ) -> AllpResult<MaintenancePlan> {
        let program = command_path(self, commands, SOURCE)?;
        let table = read_table(runner, program, &["upgrade"])?;
        if table.rejected != 0 {
            return Err(parse_error(
                "WinGet did not identify all upgradeable packages",
            ));
        }
        if table.rows.is_empty() {
            return Ok(MaintenancePlan {
                plans: Vec::new(),
                records: vec![MaintenancePlan::record(
                    SOURCE,
                    self.display_name(),
                    OperationStatus::UpToDate,
                    "WinGet reported no applicable upgrades in its community source",
                )],
            });
        }
        let command = NativeCommand::new(program).args(["upgrade", "--all", "--source", SOURCE]);
        let mut upgrade = plan(
            self,
            OperationKind::Upgrade,
            "Upgrade applicable WinGet community packages",
            None,
            command,
        );
        let count = table.rows.len();
        let ids = table
            .rows
            .iter()
            .take(8)
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let additional = if count > 8 {
            format!(" and {} more", count - 8)
        } else {
            String::new()
        };
        upgrade.details.push((
            "Previewed packages".to_owned(),
            format!("{count}: {ids}{additional}"),
        ));
        Ok(MaintenancePlan::from_plans(vec![upgrade]))
    }

    fn authorize_noninteractive(&self, plan: &mut ExecutionPlan) {
        plan.command.args.push("--disable-interactivity".into());
    }
}

impl WinGetBackend {
    fn package_plan(
        &self,
        commands: &CommandMap,
        operation: OperationKind,
        package_id: &str,
    ) -> AllpResult<ExecutionPlan> {
        validate_id(package_id)?;
        let (verb, action) = match operation {
            OperationKind::Install => ("install", "Install a Windows application with WinGet"),
            OperationKind::Remove => ("uninstall", "Uninstall a Windows application with WinGet"),
            _ => unreachable!("only package installation and removal use this helper"),
        };
        let mut command = NativeCommand::new(command_path(self, commands, SOURCE)?)
            .args([verb, "--id", package_id, "--exact", "--source", SOURCE]);
        if operation == OperationKind::Install {
            command = command.arg("--no-upgrade");
        }
        Ok(plan(self, operation, action, Some(package_id), command))
    }
}

fn plan(
    backend: &WinGetBackend,
    operation: OperationKind,
    action: &str,
    package_id: Option<&str>,
    command: NativeCommand,
) -> ExecutionPlan {
    ExecutionPlan {
        backend_id: SOURCE.to_owned(),
        backend_name: backend.display_name().to_owned(),
        operation,
        action: action.to_owned(),
        package_id: package_id.map(str::to_owned),
        source: Some("WinGet community repository".to_owned()),
        scope: None,
        details: Vec::new(),
        command,
        privilege: PrivilegeRequirement::NoElevation,
        requires_root: false,
        interactive: true,
    }
}

fn validate_id(id: &str) -> AllpResult<()> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(AllpError::InvalidInput(
            "WinGet requires a full package identifier such as Git.Git".to_owned(),
        ))
    }
}

fn valid_id(id: &str) -> bool {
    if id.starts_with('-') || id.contains('…') || id.chars().count() > 128 {
        return false;
    }
    let mut components = id.split('.');
    (2..=8).contains(&components.clone().count())
        && components.all(|component| {
            (1..=32).contains(&component.chars().count())
                && component.chars().all(|character| {
                    !character.is_whitespace()
                        && !character.is_control()
                        && !matches!(
                            character,
                            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                        )
                })
        })
}

fn read_command(program: &Path, args: &[&str]) -> NativeCommand {
    NativeCommand::new(program)
        .args(args.iter().copied())
        .args(["--source", SOURCE, "--disable-interactivity"])
        .timeout(Duration::from_secs(60))
}

fn read_table(
    runner: &dyn ProcessRunner,
    program: &Path,
    args: &[&str],
) -> AllpResult<PackageTable> {
    let command = read_command(program, args);
    let is_upgrade = args.first() == Some(&"upgrade");
    let Some(output) = capture_winget(runner, &command, is_upgrade)? else {
        return Ok(PackageTable::default());
    };
    if let Some(table) = parse_table(&output) {
        return Ok(table);
    }
    // Newer App Installer versions can override the output language per call.
    match capture_winget(
        runner,
        &command.args(["--output-locale", "en-US"]),
        is_upgrade,
    ) {
        Ok(None) => return Ok(PackageTable::default()),
        Ok(Some(output)) => {
            if let Some(table) = parse_table(&output) {
                return Ok(table);
            }
        }
        Err(_) => {}
    }
    Err(parse_error(
        "unrecognized package table. Update App Installer or configure English WinGet output",
    ))
}

fn capture_winget(
    runner: &dyn ProcessRunner,
    command: &NativeCommand,
    is_upgrade: bool,
) -> AllpResult<Option<String>> {
    let output = runner.capture(command)?;
    if output.success {
        return Ok(Some(output.stdout));
    }
    if output.code == Some(NO_APPLICATIONS_FOUND)
        || (is_upgrade && output.code == Some(UPDATE_NOT_APPLICABLE))
    {
        return Ok(None);
    }
    Err(AllpError::CommandFailed {
        backend: "WinGet".to_owned(),
        command: render_native_command(command),
        code: output.code,
        stderr: if output.stderr.trim().is_empty() {
            output.stdout
        } else {
            output.stderr
        },
    })
}

#[derive(Default)]
struct PackageTable {
    rows: Vec<PackageRow>,
    rejected: usize,
}

struct PackageRow {
    name: String,
    id: String,
    version: Option<String>,
}

struct TableColumns {
    id: usize,
    version: usize,
    version_end: Option<usize>,
    source: Option<usize>,
}

fn parse_table(output: &str) -> Option<PackageTable> {
    let mut lines = output
        .split(['\r', '\n'])
        .map(|line| line.trim_matches('\u{feff}'));
    let mut header = None;
    let mut separator_seen = false;
    for line in lines.by_ref() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if matches!(
            line.trim(),
            "No package found matching input criteria."
                | "No installed package found matching input criteria."
                | "No applicable update found."
                | "No applicable upgrade found."
                | "No available upgrade found."
        ) {
            return Some(PackageTable::default());
        }
        if line.len() >= 3 && line.bytes().all(|byte| byte == b'-') && header.is_some() {
            separator_seen = true;
            break;
        }
        header = parse_header(line);
    }
    let columns = header.filter(|_| separator_seen)?;
    let mut table = PackageTable::default();
    let mut saw_data = false;
    for line in lines {
        if line.trim().is_empty() || line.starts_with('<') {
            continue;
        }
        let mut words = line.split_whitespace();
        if words.next().is_some_and(|word| word.parse::<u32>().is_ok())
            && words
                .next()
                .is_some_and(|word| matches!(word, "upgrade" | "upgrades"))
            && words.next() == Some("available.")
        {
            break;
        }
        saw_data = true;
        if let Some(start) = columns.source {
            let source = line[display_byte_offset(line, start).unwrap_or(line.len())..].trim();
            if !source.eq_ignore_ascii_case(SOURCE) {
                continue;
            }
        }
        let Some(row) = parse_row(line, &columns) else {
            table.rejected += 1;
            continue;
        };
        // Older WinGet releases include unmatched installed entries even with --source.
        if row.id.starts_with("ARP\\")
            || row.id.starts_with("MSIX\\")
            || row.id.starts_with("MSI\\")
        {
            continue;
        }
        if valid_id(&row.id) {
            table.rows.push(row);
        } else {
            table.rejected += 1;
        }
    }
    saw_data.then_some(table)
}

fn parse_header(header: &str) -> Option<TableColumns> {
    let mut names = header.split_whitespace();
    if names.next()? != "Name"
        || !names.next()?.eq_ignore_ascii_case("Id")
        || names.next()? != "Version"
    {
        return None;
    }
    let id_start = header.find(['I', 'i'])?;
    let version_start = header.find("Version")?;
    let version_end = names.next().and_then(|name| {
        header[version_start + "Version".len()..]
            .find(name)
            .map(|offset| version_start + "Version".len() + offset)
    });
    let source = header
        .ends_with(" Source")
        .then(|| header.len() - "Source".len());
    Some(TableColumns {
        id: id_start,
        version: version_start,
        version_end,
        source,
    })
}

fn parse_row(line: &str, columns: &TableColumns) -> Option<PackageRow> {
    let id_at = display_byte_offset(line, columns.id)?;
    let version_at = display_byte_offset(line, columns.version)?;
    let name = line[..id_at].strip_suffix(' ')?.trim();
    let id = line[id_at..version_at].strip_suffix(' ')?.trim();
    let version = match columns.version_end {
        Some(end) => &line[version_at..display_byte_offset(line, end).unwrap_or(line.len())],
        None => &line[version_at..],
    }
    .trim();
    if name.is_empty() || id.is_empty() {
        return None;
    }
    Some(PackageRow {
        name: name.to_owned(),
        id: id.to_owned(),
        version: (!version.is_empty()
            && !version.ends_with('…')
            && !version.eq_ignore_ascii_case("unknown"))
        .then(|| version.to_owned()),
    })
}

fn display_byte_offset(line: &str, column: usize) -> Option<usize> {
    let mut width = 0;
    for (index, character) in line.char_indices() {
        if width == column {
            return Some(index);
        }
        width += character.width()?;
        if width > column {
            return None;
        }
    }
    (width == column).then_some(line.len())
}

fn parse_info<'a>(
    output: &'a str,
    package_id: &str,
) -> AllpResult<(&'a str, std::collections::BTreeMap<String, String>)> {
    let mut lines = output.lines();
    for line in lines.by_ref() {
        // WinGet can overwrite a progress indicator before printing the identity.
        let line = line.rsplit_once('\r').map_or(line, |(_, visible)| visible);
        let Some(found) = line.trim_start_matches('\u{feff}').strip_prefix("Found ") else {
            continue;
        };
        let Some((name, suffix)) = found.rsplit_once(" [") else {
            continue;
        };
        let Some((id, version)) = suffix.split_once(']') else {
            continue;
        };
        if !id.eq_ignore_ascii_case(package_id) {
            continue;
        }
        let mut fields = parse_key_value_lines(&lines.collect::<Vec<_>>().join("\n"));
        if let Some(version) = version.trim().strip_prefix("Version ") {
            fields
                .entry("Version".to_owned())
                .or_insert_with(|| version.to_owned());
        }
        return Ok((name, fields));
    }
    Err(parse_error("unrecognized WinGet package details. Update App Installer or configure English WinGet output"))
}

fn parse_error(message: &str) -> AllpError {
    AllpError::Parse {
        backend: "WinGet".to_owned(),
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{CommandOutput, ProcessStatus};
    use std::{collections::VecDeque, path::PathBuf, sync::Mutex};

    const HEADER: &str = "Name         Id               Version  Match\n--------------------------------------------\n";
    const RESULTS: &str =
        "7-Zip        7zip.7zip        25.01    Tag: archive\n資料庫       Example.Database 3.2\n";

    fn commands() -> CommandMap {
        [(SOURCE.to_owned(), PathBuf::from("winget.exe"))].into()
    }

    struct ScriptedRunner(Mutex<VecDeque<CommandOutput>>);

    impl ScriptedRunner {
        fn new(items: &[(i32, &str)]) -> Self {
            Self(Mutex::new(
                items
                    .iter()
                    .map(|(code, stdout)| CommandOutput {
                        success: *code == 0,
                        code: Some(*code),
                        signal: None,
                        duration: Duration::ZERO,
                        stdout: (*stdout).to_owned(),
                        stderr: String::new(),
                    })
                    .collect(),
            ))
        }
    }

    impl ProcessRunner for ScriptedRunner {
        fn capture(&self, _command: &NativeCommand) -> AllpResult<CommandOutput> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected WinGet capture"))
        }

        fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
            unreachable!("backend planning must not execute a mutation")
        }
    }

    #[test]
    fn parses_aligned_columns_including_wide_unicode_names() {
        let output = format!("\r  |\r\n{HEADER}{RESULTS}");
        let table = parse_table(&output).unwrap();
        assert_eq!(table.rejected, 0);
        assert_eq!(table.rows[0].id, "7zip.7zip");
        assert_eq!(table.rows[0].version.as_deref(), Some("25.01"));
        assert_eq!(table.rows[1].name, "資料庫");
        assert_eq!(table.rows[1].id, "Example.Database");
    }

    #[test]
    fn parses_the_native_windows_crlf_search_table() {
        let output = "\r\n   - \r\n    \r\nName Id         Version\r\n-----------------------\r\nfd   sharkdp.fd 10.5.0\r\n";
        let result = WinGetBackend
            .search(
                &commands(),
                &ScriptedRunner::new(&[(0, output)]),
                "sharkdp.fd",
            )
            .unwrap();
        assert!(result.issues.is_empty());
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].package_id, "sharkdp.fd");
        assert_eq!(result.candidates[0].version.as_deref(), Some("10.5.0"));
    }

    #[test]
    fn installed_inventory_skips_uncorrelated_arp_entries() {
        let output = format!("{HEADER}Legacy       ARP\\User\\Example 1.0\n{RESULTS}");
        let packages = WinGetBackend
            .list_installed(&commands(), &ScriptedRunner::new(&[(0, &output)]))
            .unwrap();
        assert_eq!(packages.len(), 2);
        assert!(packages
            .iter()
            .all(|package| package.source.as_deref() == Some("winget")));
    }

    #[test]
    fn a_truncated_id_is_reported_and_never_used_for_an_install() {
        let output = format!("{HEADER}7-Zip        7zip.7z…         25.01\n");
        let result = WinGetBackend
            .search(&commands(), &ScriptedRunner::new(&[(0, &output)]), "7-Zip")
            .unwrap();
        assert!(result.candidates.is_empty());
        assert_eq!(result.issues.len(), 1);
        assert!(WinGetBackend
            .package_plan(&commands(), OperationKind::Install, "7zip.7z…")
            .is_err());
        assert!(valid_id("André.Example"));
        assert!(!valid_id("Example..."));
    }

    #[test]
    fn plans_only_exact_packages_and_leaves_elevation_to_windows() {
        let plan = WinGetBackend
            .package_plan(&commands(), OperationKind::Install, "7zip.7zip")
            .unwrap();
        assert_eq!(
            plan.command.args,
            [
                "install",
                "--id",
                "7zip.7zip",
                "--exact",
                "--source",
                "winget",
                "--no-upgrade"
            ]
        );
        assert_eq!(plan.privilege, PrivilegeRequirement::NoElevation);
        assert!(!plan.requires_root);
        let plan = WinGetBackend
            .package_plan(&commands(), OperationKind::Remove, "7zip.7zip")
            .unwrap();
        assert_eq!(
            plan.command.args,
            [
                "uninstall",
                "--id",
                "7zip.7zip",
                "--exact",
                "--source",
                "winget"
            ]
        );
        assert!(WinGetBackend
            .package_plan(&commands(), OperationKind::Remove, "--all")
            .is_err());
    }

    #[test]
    fn no_applicable_upgrade_is_distinct_from_a_failed_source() {
        let backend = WinGetBackend;
        let no_updates = backend
            .plan_upgrade(
                &commands(),
                &ScriptedRunner::new(&[(UPDATE_NOT_APPLICABLE, "No applicable update found.")]),
                None,
                None,
            )
            .unwrap();
        assert!(no_updates.plans.is_empty());
        assert!(matches!(
            no_updates.records[0].status,
            OperationStatus::UpToDate
        ));
        let failure = backend
            .search(
                &commands(),
                &ScriptedRunner::new(&[(1, "Failed to open source.")]),
                "7zip",
            )
            .unwrap_err();
        assert!(failure.to_string().contains("Failed to open source."));
    }

    #[test]
    fn source_refresh_and_app_upgrade_are_distinct_plans() {
        let backend = WinGetBackend;
        let output = format!("{HEADER}{RESULTS}2 upgrades available.\n");
        let runner = ScriptedRunner::new(&[(0, &output)]);
        let update = backend
            .plan_update(&commands(), &runner, None, None)
            .unwrap();
        assert_eq!(
            update.plans[0].command.args,
            ["source", "update", "--name", "winget"]
        );
        let upgrade = backend
            .plan_upgrade(&commands(), &runner, None, None)
            .unwrap();
        assert_eq!(
            upgrade.plans[0].command.args,
            ["upgrade", "--all", "--source", "winget"]
        );
    }

    #[test]
    fn show_requires_the_requested_identity_before_extracting_metadata() {
        let details = "\r -\rFound 7-Zip [7zip.7zip] Version 25.01\r\nVersion: 25.01\r\nPublisher: Igor Pavlov\r\nDescription: Archive manager\r\n";
        let info = WinGetBackend
            .info(
                &commands(),
                &ScriptedRunner::new(&[(0, details)]),
                "7zip.7zip",
            )
            .unwrap();
        assert_eq!(info.version.as_deref(), Some("25.01"));
        assert_eq!(info.description.as_deref(), Some("Archive manager"));
        assert!(parse_info(details, "Wrong.Package").is_err());
    }
}
