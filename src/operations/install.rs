use crate::{
    alternatives::AlternativeSearchRequest,
    backends::{InstallPreflight, InstallPreflightRecovery},
    cli::{
        confirm_conflicting_identity, confirm_execution, confirm_fuzzy_candidate, select_installer,
        select_no_alternative_action, select_package_candidate, should_page_candidate_selection,
        AlternativeNoMatchAction, ConfirmationRequest,
    },
    domain::{
        AllpError, AllpResult, Capability, MatchKind, PackageDomain, SearchBackendState,
        SearchScope,
    },
    execution::{render_execution_plan_with_context, ProcessStatus},
    operations::{
        search::{self, SearchPolicy},
        validate_package_id, OperationContext,
    },
};
use std::time::Instant;

pub fn run(context: &OperationContext<'_>, package: &str) -> AllpResult<()> {
    let mut active_backend_filter = context.backend_filter;
    let mut current_query = package.to_owned();
    let mut alternative_request: Option<AlternativeSearchRequest> = None;

    'search_again: loop {
        let search_context = OperationContext {
            backends: context.backends,
            discovery: context.discovery,
            runner: context.runner,
            renderer: context.renderer,
            privilege_context: context.privilege_context,
            dry_run: context.dry_run,
            no_interactive: context.no_interactive,
            yes: context.yes,
            allow_bootstrap: context.allow_bootstrap,
            verbose: context.verbose,
            state_dir: context.state_dir,
            backend_filter: active_backend_filter,
            search_scope: context.search_scope,
            target: context.target,
            root_context_notice_shown: context.root_context_notice_shown,
        };
        let policy = SearchPolicy {
            required_capability: Some(Capability::Install),
            scope: context.search_scope.unwrap_or(SearchScope::AllSources),
            ..SearchPolicy::default()
        };
        let report = if let Some(request) = &alternative_request {
            search::gather_with_policy_excluding(
                &search_context,
                &current_query,
                policy,
                &request.excluded_backends,
            )?
        } else {
            search::gather_with_policy(&search_context, &current_query, policy)?
        };

        for issue in &report.issues {
            context
                .renderer
                .warn(&format!("{}: {}", issue.backend_name, issue.message));
        }

        if report.candidates.is_empty() {
            if alternative_request.is_none() {
                return Err(AllpError::PackageNotFound(current_query.clone()));
            }
            context.renderer.search(&report);
            let can_configure_remote = report.backend_summaries.iter().any(|summary| {
                summary.state == SearchBackendState::NoConfiguredRemotes
                    && context.backends.get(&summary.backend_id).is_some()
            });
            loop {
                match select_no_alternative_action(can_configure_remote, context.no_interactive)? {
                    AlternativeNoMatchAction::ConfigureFlatpak => {
                        configure_search_prerequisite(context, &report)?;
                        continue 'search_again;
                    }
                    AlternativeNoMatchAction::SearchAnother(query) => {
                        current_query = query;
                        if let Some(request) = &mut alternative_request {
                            request.query = current_query.clone();
                        }
                        continue 'search_again;
                    }
                    AlternativeNoMatchAction::UnrestrictedSearch => {
                        active_backend_filter = context.backend_filter;
                        alternative_request = None;
                        current_query = package.to_owned();
                        continue 'search_again;
                    }
                    AlternativeNoMatchAction::ShowDiagnostics => {
                        context.renderer.search(&report);
                    }
                    AlternativeNoMatchAction::Cancelled => {
                        context.renderer.info_message("Installation cancelled");
                        context.renderer.plain_message("0 commands executed");
                        return Ok(());
                    }
                }
            }
        }

        let selectable = if active_backend_filter.is_some() {
            let exact: Vec<_> = report
                .candidates
                .iter()
                .filter(|candidate| candidate.package_id.eq_ignore_ascii_case(&current_query))
                .cloned()
                .collect();
            if exact.len() == 1 {
                exact
            } else {
                report.candidates.clone()
            }
        } else {
            report.candidates.clone()
        };

        if selectable.len() == 1 && !report.complete {
            return Err(AllpError::AmbiguousSelection(
                "Search completed with incomplete coverage.\n\nAllp will not auto-select a unique result because one eligible backend failed.\nUse --from to choose explicitly."
                    .to_owned(),
            ));
        }

        let scope = context.search_scope.unwrap_or(SearchScope::AllSources);
        if !context.renderer.json()
            && !should_page_candidate_selection(&selectable, context.no_interactive)
        {
            context
                .renderer
                .install_sources(&current_query, scope, &selectable);
        }
        let preferred_identity_index = preferred_official_identity_index(&selectable);
        if selectable.len() > 1 && context.no_interactive && preferred_identity_index.is_none() {
            return Err(AllpError::AmbiguousSelection(install_ambiguity_message(
                &current_query,
                &selectable,
                context.dry_run,
            )));
        }
        let selected_index = match preferred_identity_index {
            Some(index) => index,
            None => select_package_candidate(&selectable, context.no_interactive)?,
        };
        if selectable.len() == 1 && context.no_interactive && !context.renderer.json() {
            context.renderer.info_message(&format!(
                "Only one result found; selecting {}.",
                selectable[selected_index].package_id
            ));
        }
        let mut candidate = selectable[selected_index].clone();
        if matches!(
            candidate.domain,
            PackageDomain::Python | PackageDomain::Node
        ) && candidate.match_kind == MatchKind::Fuzzy
        {
            confirm_fuzzy_candidate(context.no_interactive)?;
        }
        if let Some(filter) = active_backend_filter {
            if candidate
                .installers
                .iter()
                .any(|installer| installer.eq_ignore_ascii_case(filter))
            {
                candidate.installers = vec![filter.to_ascii_lowercase()];
            }
        }
        if let Some(installer) = select_installer(&candidate, context.no_interactive)? {
            candidate.installers = vec![installer];
        }
        validate_package_id(&candidate.package_id)?;

        let mut selected_runtime = None;
        let plan = if let Some(plan) = crate::bootstrap::plan_install(&candidate)? {
            plan
        } else {
            let runtime = context.backend(&candidate.backend_id)?;
            loop {
                if let Some(status) = runtime
                    .backend
                    .install_preflight_status(&runtime.commands, &candidate)?
                {
                    context.renderer.preflight_stage(
                        &status.stage,
                        &status.command,
                        status.display_command.as_deref(),
                        context.verbose > 0,
                    );
                }
                let preflight = runtime.backend.preflight_plan_install(
                    &runtime.commands,
                    context.runner,
                    &candidate,
                );
                match preflight {
                    Ok(InstallPreflight::Continue) => break,
                    Ok(InstallPreflight::UseCandidate {
                        candidate: resolved,
                        warnings,
                    }) => {
                        if context.verbose > 0 {
                            for warning in warnings {
                                context
                                    .renderer
                                    .preflight_warning(&warning.title, &warning.message);
                            }
                        }
                        candidate = *resolved;
                        validate_package_id(&candidate.package_id)?;
                        break;
                    }
                    Ok(InstallPreflight::AlreadyInstalled {
                        package_id,
                        installed_version,
                        candidate_version,
                    }) => {
                        context.renderer.already_installed(
                            runtime.backend.display_name(),
                            &package_id,
                            installed_version.as_deref(),
                            candidate_version.as_deref(),
                        );
                        return Ok(());
                    }
                    Err(error) => {
                        match runtime.backend.recover_install_preflight_failure(
                            &runtime.commands,
                            context.runner,
                            &candidate,
                            error,
                            context.no_interactive,
                        )? {
                            InstallPreflightRecovery::RetryValidation => continue,
                            InstallPreflightRecovery::RetrySearch => {
                                active_backend_filter = context.backend_filter;
                                alternative_request = None;
                                current_query = package.to_owned();
                                continue 'search_again;
                            }
                            InstallPreflightRecovery::TryAlternativeInstallers => {
                                active_backend_filter = None;
                                let mut request = AlternativeSearchRequest::new(&current_query);
                                request.software_identity = Some(candidate.identity.clone());
                                request.exclude(candidate.backend_id.clone());
                                alternative_request = Some(request);
                                continue 'search_again;
                            }
                            InstallPreflightRecovery::Cancelled => {
                                context.renderer.info_message("Installation cancelled");
                                context.renderer.plain_message("0 commands executed");
                                return Ok(());
                            }
                        }
                    }
                }
            }
            selected_runtime = Some(runtime);
            runtime
                .backend
                .plan_install(&runtime.commands, &candidate)?
        };

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

        if candidate.identity.is_conflicting() {
            confirm_conflicting_identity(&candidate, context.no_interactive)?;
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
                prompt: "Install this package?".to_owned(),
                default_yes: true,
                non_interactive_hint: format!(
                    "Review with:\n  allp install {} --from {} --dry-run\n\nExecute explicitly with:\n  allp install {} --from {} --yes",
                    candidate.package_id, candidate.backend_id, candidate.package_id, candidate.backend_id
                ),
            },
        )?;
        if !confirmed {
            context.renderer.info_message("Installation cancelled");
            context.renderer.plain_message("0 commands executed");
            return Ok(());
        }
        if let Some(runtime) = selected_runtime {
            runtime.backend.preflight_install(
                &runtime.commands,
                context.runner,
                &candidate,
                context.privilege_context,
            )?;
        }
        return execute_install(context, plan, selected_runtime);
    }
}

fn configure_search_prerequisite(
    context: &OperationContext<'_>,
    report: &crate::domain::SearchReport,
) -> AllpResult<()> {
    let summary = report
        .backend_summaries
        .iter()
        .find(|summary| summary.state == SearchBackendState::NoConfiguredRemotes)
        .ok_or_else(|| {
            AllpError::InvalidInput("no configurable search prerequisite is available".to_owned())
        })?;
    let runtime = context.backend(&summary.backend_id)?;
    let plan = runtime
        .backend
        .plan_search_prerequisite(&runtime.commands)?
        .ok_or_else(|| AllpError::UnsupportedOperation {
            backend: runtime.backend.display_name().to_owned(),
            operation: "configure search prerequisite".to_owned(),
        })?;
    context
        .renderer
        .planned_operations(std::slice::from_ref(&plan), context.privilege_context);
    if context.dry_run {
        return Err(AllpError::InvalidInput(
            "dry run stopped before changing Flatpak remote configuration".to_owned(),
        ));
    }
    let confirmed = confirm_execution(
        context.no_interactive,
        context.yes && context.allow_bootstrap,
        ConfirmationRequest {
            prompt: "Add this Flatpak remote?".to_owned(),
            default_yes: false,
            non_interactive_hint: "Remote configuration requires explicit approval. Use --yes --allow-bootstrap after reviewing the plan."
                .to_owned(),
        },
    )?;
    if !confirmed {
        return Err(AllpError::InvalidInput(
            "Flatpak remote configuration cancelled".to_owned(),
        ));
    }
    let status = context.runner.execute(&plan)?;
    ensure_configuration_succeeded(context, &plan, status)?;
    if !runtime
        .backend
        .verify_search_prerequisite(&runtime.commands, context.runner)?
    {
        return Err(AllpError::InvalidInput(format!(
            "{} configuration command succeeded, but capability refresh could not verify the required remote",
            runtime.backend.display_name()
        )));
    }
    context.renderer.success_message(&format!(
        "{} search prerequisite configured and verified.",
        runtime.backend.display_name()
    ));
    Ok(())
}

fn ensure_configuration_succeeded(
    context: &OperationContext<'_>,
    plan: &crate::domain::ExecutionPlan,
    status: ProcessStatus,
) -> AllpResult<()> {
    if status.success {
        return Ok(());
    }
    Err(AllpError::CommandFailed {
        backend: plan.backend_name.clone(),
        command: render_execution_plan_with_context(plan, context.privilege_context),
        code: status.code,
        stderr: if status.stderr.trim().is_empty() {
            status.stdout
        } else {
            status.stderr
        },
    })
}

fn execute_install(
    context: &OperationContext<'_>,
    plan: crate::domain::ExecutionPlan,
    selected_runtime: Option<&crate::discovery::DetectedBackend>,
) -> AllpResult<()> {
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
            &crate::domain::OperationStatus::Completed,
            None,
            started.elapsed(),
        );
        context
            .renderer
            .success_message("Installation command completed successfully.");
        Ok(())
    } else {
        let command = render_execution_plan_with_context(&plan, context.privilege_context);
        if let Some(runtime) = selected_runtime {
            if let Some(error) = runtime
                .backend
                .classify_execution_failure(&plan, &status, &command)
            {
                let message = error.to_string();
                context.renderer.execution_finished(
                    1,
                    1,
                    &plan.backend_name,
                    &crate::domain::OperationStatus::Busy,
                    Some(&message),
                    started.elapsed(),
                );
                return Err(error);
            }
        }
        context.renderer.execution_finished(
            1,
            1,
            &plan.backend_name,
            &crate::domain::OperationStatus::Failed,
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

fn preferred_official_identity_index(
    candidates: &[crate::domain::PackageCandidate],
) -> Option<usize> {
    let official = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.identity.is_official())
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if official.len() == 1
        && candidates
            .iter()
            .any(|candidate| candidate.identity.is_conflicting())
    {
        return official.first().copied();
    }
    None
}

fn install_ambiguity_message(
    package: &str,
    candidates: &[crate::domain::PackageCandidate],
    dry_run: bool,
) -> String {
    let mut message =
        format!("Multiple install candidates were found for \"{package}\".\n\nUse one of:");
    for candidate in candidates.iter().take(8) {
        let dry_run_flag = if dry_run { " --dry-run" } else { "" };
        message.push_str(&format!(
            "\n  allp install {} --from {}{dry_run_flag}",
            candidate.package_id, candidate.backend_id,
        ));
    }
    message.push_str(
        "\n\nYou can also narrow the broad search first with --scope apps, --scope dev, or --scope all.",
    );
    message
}

#[cfg(test)]
mod tests {
    use super::install_ambiguity_message;
    use crate::domain::{BackendCategory, MatchKind, PackageCandidate, PackageDomain};

    #[test]
    fn non_interactive_ambiguity_includes_recovery_commands() {
        let candidates = vec![
            candidate("first", "git", MatchKind::Exact),
            candidate("second", "git-scm", MatchKind::Related),
        ];

        let message = install_ambiguity_message("git", &candidates, true);

        assert!(message.contains("allp install git --from first --dry-run"));
        assert!(message.contains("allp install git-scm --from second --dry-run"));
    }

    fn candidate(backend_id: &str, package_id: &str, match_kind: MatchKind) -> PackageCandidate {
        PackageCandidate {
            backend_id: backend_id.to_owned(),
            backend_name: backend_id.to_ascii_uppercase(),
            category: BackendCategory::System,
            domain: PackageDomain::System,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: None,
            description: None,
            source: None,
            installers: Vec::new(),
            artifact_kind: "test".to_owned(),
            scope: None,
            match_kind,
            identity: PackageCandidate::infer_identity(match_kind, PackageDomain::System, "test"),
            metadata: Default::default(),
        }
    }
}
