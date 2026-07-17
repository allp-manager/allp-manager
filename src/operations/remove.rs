use crate::{
    cli::{confirm_execution, select_installed, ConfirmationRequest},
    domain::{AllpError, AllpResult, OperationStatus},
    execution::render_execution_plan_with_context,
    operations::{list, validate_package_id, OperationContext},
};
use std::time::Instant;

pub fn run(context: &OperationContext<'_>, package: &str) -> AllpResult<()> {
    let report = list::gather(context)?;
    for issue in &report.issues {
        context
            .renderer
            .warn(&format!("{}: {}", issue.backend_name, issue.message));
    }

    let query = package.to_ascii_lowercase();
    let mut matches: Vec<_> = report
        .packages
        .iter()
        .filter(|installed| {
            installed.package_id.eq_ignore_ascii_case(package)
                || installed.display_name.eq_ignore_ascii_case(package)
                || installed.package_id.to_ascii_lowercase().contains(&query)
                || installed.display_name.to_ascii_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    let exact_package_id: Vec<_> = matches
        .iter()
        .filter(|installed| installed.package_id.eq_ignore_ascii_case(package))
        .cloned()
        .collect();
    let has_meaningful_related = matches.iter().any(|installed| {
        !installed.package_id.eq_ignore_ascii_case(package)
            && (installed.package_id.contains('.')
                || !installed
                    .display_name
                    .eq_ignore_ascii_case(&installed.package_id))
    });
    if exact_package_id.len() == 1 && !has_meaningful_related {
        matches = exact_package_id;
    }
    matches.sort_by(|left, right| {
        let left_exact = left.package_id.eq_ignore_ascii_case(package)
            || left.display_name.eq_ignore_ascii_case(package);
        let right_exact = right.package_id.eq_ignore_ascii_case(package)
            || right.display_name.eq_ignore_ascii_case(package);
        right_exact
            .cmp(&left_exact)
            .then(left.category.cmp(&right.category))
            .then(left.backend_id.cmp(&right.backend_id))
            .then(left.package_id.cmp(&right.package_id))
    });

    if matches.is_empty() {
        return Err(AllpError::PackageNotFound(package.to_owned()));
    }

    if !context.renderer.json() {
        context.renderer.installed_choices(&matches);
    }
    if matches.len() > 1 && context.no_interactive {
        return Err(AllpError::AmbiguousSelection(remove_ambiguity_message(
            package, &matches,
        )));
    }
    let selected_index = select_installed(&matches, context.no_interactive)?;
    let installed = &matches[selected_index];
    validate_package_id(&installed.package_id)?;

    let runtime = context.backend(&installed.backend_id)?;
    let plan = runtime.backend.plan_remove(&runtime.commands, installed)?;

    if context.renderer.json() {
        context.renderer.plan(&plan, context.privilege_context);
    } else {
        context
            .renderer
            .planned_operations(std::slice::from_ref(&plan), context.privilege_context);
    }
    if context.dry_run {
        context
            .renderer
            .success_message("Dry run complete; no command was executed.");
        return Ok(());
    }

    context.renderer.privilege_notice(
        std::slice::from_ref(&plan),
        context.no_interactive,
        context.privilege_context,
        context.root_context_notice_shown,
    );
    let confirmed = confirm_execution(
        context.no_interactive,
        context.yes,
        ConfirmationRequest {
            prompt: "Remove it?".to_owned(),
            default_yes: false,
            non_interactive_hint: format!(
                "Review with:\n  allp remove {} --from {} --dry-run\n\nExecute explicitly with:\n  allp remove {} --from {} --yes",
                installed.package_id,
                installed.backend_id,
                installed.package_id,
                installed.backend_id
            ),
        },
    )?;
    if !confirmed {
        context.renderer.info_message("Removal cancelled");
        context.renderer.plain_message("0 commands executed");
        return Ok(());
    }
    context
        .renderer
        .execution_started(1, 1, &plan, context.privilege_context);
    let started = Instant::now();
    let status = context.runner.execute(&plan)?;
    if status.success {
        context.renderer.execution_finished(
            1,
            1,
            &plan.backend_name,
            &OperationStatus::Completed,
            None,
            started.elapsed(),
        );
        context
            .renderer
            .success_message("Removal command completed successfully.");
        Ok(())
    } else {
        let command = render_execution_plan_with_context(&plan, context.privilege_context);
        if let Some(error) = runtime
            .backend
            .classify_execution_failure(&plan, &status, &command)
        {
            let message = error.to_string();
            context.renderer.execution_finished(
                1,
                1,
                &plan.backend_name,
                &OperationStatus::Busy,
                Some(&message),
                started.elapsed(),
            );
            return Err(error);
        }
        context.renderer.execution_finished(
            1,
            1,
            &plan.backend_name,
            &OperationStatus::Failed,
            Some(&status.stderr),
            started.elapsed(),
        );
        Err(AllpError::CommandFailed {
            backend: plan.backend_name.clone(),
            command,
            code: status.code,
            stderr: status.stderr,
        })
    }
}

fn remove_ambiguity_message(package: &str, matches: &[crate::domain::InstalledPackage]) -> String {
    let mut message =
        format!("Multiple installed copies were found for \"{package}\".\n\nUse one of:");
    for installed in matches.iter().take(8) {
        message.push_str(&format!(
            "\n  allp remove {} --from {}",
            installed.package_id, installed.backend_id
        ));
    }
    message
}

#[cfg(test)]
mod tests {
    use super::remove_ambiguity_message;
    use crate::domain::{BackendCategory, InstalledPackage, PackageDomain};

    #[test]
    fn remove_ambiguity_includes_backend_specific_recovery() {
        let matches = vec![
            installed("first", "code"),
            installed("second", "com.visualstudio.code"),
        ];

        let message = remove_ambiguity_message("code", &matches);

        assert!(message.contains("allp remove code --from first"));
        assert!(message.contains("allp remove com.visualstudio.code --from second"));
    }

    fn installed(backend_id: &str, package_id: &str) -> InstalledPackage {
        InstalledPackage {
            backend_id: backend_id.to_owned(),
            backend_name: backend_id.to_ascii_uppercase(),
            category: BackendCategory::System,
            domain: PackageDomain::System,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: None,
            description: None,
            source: None,
            scope: None,
        }
    }
}
