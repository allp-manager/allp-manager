use crate::{
    cli::Spinner,
    discovery::path::find_executable,
    domain::{AllpResult, BackendIssue, Capability, InstalledPackage},
    operations::OperationContext,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
};

const MAX_LIST_CONCURRENCY: usize = 4;
const PAGER_THRESHOLD: usize = 100;

#[derive(Debug, Clone, Serialize)]
pub struct InstalledReport {
    pub packages: Vec<InstalledPackage>,
    pub issues: Vec<BackendIssue>,
    pub complete: bool,
}

pub fn run(
    context: &OperationContext<'_>,
    filter: Option<&str>,
    limit: Option<usize>,
    no_pager: bool,
) -> AllpResult<InstalledReport> {
    if matches!(limit, Some(0)) {
        return Err(crate::domain::AllpError::InvalidInput(
            "--limit must be greater than zero".to_owned(),
        ));
    }

    let mut report = gather(context)?;
    apply_filter_and_limit(&mut report.packages, filter, limit);

    if context.renderer.json() {
        context.renderer.render_json_envelope(
            "list",
            report.complete,
            &report.packages,
            &report.issues,
        );
    } else {
        let output = render_installed(&report.packages);
        write_human_output(&output, report.packages.len(), no_pager)?;
        for issue in &report.issues {
            context
                .renderer
                .warn(&format!("{}: {}", issue.backend_name, issue.message));
        }
    }
    Ok(report)
}

pub fn gather(context: &OperationContext<'_>) -> AllpResult<InstalledReport> {
    let eligible: Vec<_> = context
        .eligible_backends()?
        .into_iter()
        .filter(|runtime| runtime.backend.has_capability(Capability::List))
        .cloned()
        .collect();

    let spinner = Spinner::start(
        format!(
            "Reading installed packages from {} manager(s)",
            eligible.len()
        ),
        context.renderer.spinner_enabled(),
    );

    let mut packages = Vec::new();
    let mut issues = Vec::new();

    for chunk in eligible.chunks(MAX_LIST_CONCURRENCY) {
        let (sender, receiver) = mpsc::channel();
        thread::scope(|scope| {
            for runtime in chunk.iter().cloned() {
                let sender = sender.clone();
                let runner = context.runner;
                scope.spawn(move || {
                    let backend_id = runtime.backend.id().to_owned();
                    let backend_name = runtime.backend.display_name().to_owned();
                    let result = runtime.backend.list_installed(&runtime.commands, runner);
                    let _ = sender.send((backend_id, backend_name, result));
                });
            }
            drop(sender);
        });

        for (backend_id, backend_name, result) in receiver {
            match result {
                Ok(mut found) => packages.append(&mut found),
                Err(error) => issues.push(BackendIssue {
                    backend_id,
                    backend_name,
                    message: error.to_string(),
                }),
            }
        }
    }

    spinner.stop();

    packages.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then(left.backend_id.cmp(&right.backend_id))
            .then(left.package_id.cmp(&right.package_id))
    });

    Ok(InstalledReport {
        complete: issues.is_empty(),
        packages,
        issues,
    })
}

fn apply_filter_and_limit(
    packages: &mut Vec<InstalledPackage>,
    filter: Option<&str>,
    limit: Option<usize>,
) {
    if let Some(filter) = filter.map(str::trim).filter(|filter| !filter.is_empty()) {
        let filter = filter.to_ascii_lowercase();
        packages.retain(|package| {
            package.package_id.to_ascii_lowercase().contains(&filter)
                || package.display_name.to_ascii_lowercase().contains(&filter)
        });
    }
    if let Some(limit) = limit {
        packages.truncate(limit);
    }
}

fn render_installed(packages: &[InstalledPackage]) -> String {
    let mut by_backend: BTreeMap<&str, Vec<&InstalledPackage>> = BTreeMap::new();
    for package in packages {
        by_backend
            .entry(&package.backend_name)
            .or_default()
            .push(package);
    }

    let mut output = String::new();
    if packages.is_empty() {
        output.push_str("Installed Packages\n0 packages\n");
        return output;
    }

    for (backend, packages) in by_backend {
        output.push_str(&format!("\nInstalled Packages · {backend}\n"));
        output.push_str(&format!("{} package(s)\n\n", packages.len()));
        for (index, package) in packages.iter().enumerate() {
            output.push_str(&format!(
                "[{}] {:<36} {}\n",
                index + 1,
                package.package_id,
                package.version.as_deref().unwrap_or("unknown")
            ));
        }
    }
    output
}

fn write_human_output(output: &str, package_count: usize, no_pager: bool) -> AllpResult<()> {
    if should_page(package_count, io::stdout().is_terminal(), false, no_pager) {
        if let Some(pager) = resolve_pager() {
            if write_to_pager(output, pager).is_ok() {
                return Ok(());
            }
        }
    }
    print!("{output}");
    Ok(())
}

pub fn should_page(
    package_count: usize,
    stdout_is_terminal: bool,
    json: bool,
    no_pager: bool,
) -> bool {
    stdout_is_terminal && !json && !no_pager && package_count > PAGER_THRESHOLD
}

fn resolve_pager() -> Option<PagerCommand> {
    if let Some(value) = std::env::var_os("PAGER").filter(safe_pager_value) {
        let path = PathBuf::from(&value);
        let program = if path.components().count() > 1 {
            Some(path)
        } else {
            find_executable(&value.to_string_lossy())
        }?;
        return Some(PagerCommand {
            program,
            args: Vec::new(),
        });
    }

    find_executable("less")
        .map(|program| PagerCommand {
            program,
            args: vec![OsString::from("-FRSX")],
        })
        .or_else(|| {
            find_executable("more").map(|program| PagerCommand {
                program,
                args: Vec::new(),
            })
        })
}

fn safe_pager_value(value: &OsString) -> bool {
    let value = value.to_string_lossy();
    !value.trim().is_empty()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '|' | '&' | ';'))
}

struct PagerCommand {
    program: PathBuf,
    args: Vec<OsString>,
}

fn write_to_pager(output: &str, pager: PagerCommand) -> io::Result<()> {
    let mut child = Command::new(pager.program)
        .args(pager.args)
        .stdin(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(output.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{apply_filter_and_limit, should_page};
    use crate::domain::{BackendCategory, InstalledPackage, PackageDomain};

    #[test]
    fn filter_is_applied_before_limit() {
        let mut packages = vec![
            installed("alpha"),
            installed("git"),
            installed("git-gui"),
            installed("zeta"),
        ];

        apply_filter_and_limit(&mut packages, Some("git"), Some(1));

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package_id, "git");
    }

    #[test]
    fn pager_decision_respects_json_redirection_and_no_pager() {
        assert!(should_page(200, true, false, false));
        assert!(!should_page(200, false, false, false));
        assert!(!should_page(200, true, true, false));
        assert!(!should_page(200, true, false, true));
        assert!(!should_page(20, true, false, false));
    }

    fn installed(package_id: &str) -> InstalledPackage {
        InstalledPackage {
            backend_id: "example".to_owned(),
            backend_name: "Example".to_owned(),
            category: BackendCategory::Development,
            domain: PackageDomain::Python,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: None,
            description: None,
            source: None,
            scope: None,
        }
    }
}
