use crate::{
    backends::BackendOperationCapability,
    cli::{confirm_execution, ConfirmationRequest},
    domain::{
        AllpResult, BackendOperationRecord, Capability, ExecutionPlan, MultiOperationReport,
        OperationKind, OperationStatus,
    },
    execution::{
        render_execution_plan_with_context, render_execution_plan_with_privilege_session,
        MaintenanceHookRunner, PrivilegeSession, PrivilegeStatus, ProcessExecutionOutcome,
    },
    operations::OperationContext,
    state,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const METADATA_REFRESH_FRESH_FOR: Duration = Duration::from_secs(6 * 60 * 60);
const METADATA_REFRESH_STATE_FILE: &str = "backend-metadata-refresh.json";

#[derive(Debug)]
struct PlannedOperation {
    id: usize,
    plan: ExecutionPlan,
    depends_on: Vec<usize>,
}

pub fn run(
    context: &OperationContext<'_>,
    capability: Capability,
    operation_name: &str,
) -> AllpResult<MultiOperationReport> {
    let mut records = Vec::new();
    let mut operations = Vec::new();
    let mut upgrades_pending_refresh = BTreeSet::new();

    for runtime in context.eligible_backends()? {
        let operation_capability = runtime.backend.operation_capability(capability);
        if should_skip_operation(capability, operation_capability) {
            records.push(BackendOperationRecord {
                backend_id: runtime.backend.id().to_owned(),
                backend_name: runtime.backend.display_name().to_owned(),
                action: None,
                command: None,
                status: OperationStatus::NotApplicable,
                message: Some(
                    runtime
                        .backend
                        .operation_not_applicable_message(capability, operation_capability),
                ),
                privilege_status: None,
            });
            continue;
        }

        if capability == Capability::Upgrade
            && operation_capability == BackendOperationCapability::InstalledPackageUpgrade
            && runtime.backend.requires_metadata_refresh_before_upgrade()
            && metadata_refresh_is_stale(context.state_dir, runtime.backend.id())
        {
            match runtime.backend.plan_update(
                &runtime.commands,
                context.runner,
                context.backend_filter,
                context.target,
            ) {
                Ok(mut refresh_plans) => {
                    let dependency_ids =
                        append_operations(&mut operations, refresh_plans.plans, &[]);
                    records.append(&mut refresh_plans.records);
                    if runtime.backend.plan_upgrade_after_metadata_refresh() {
                        upgrades_pending_refresh.insert(runtime.backend.id().to_owned());
                        continue;
                    }
                    let backend_plans = runtime.backend.plan_upgrade(
                        &runtime.commands,
                        context.runner,
                        context.backend_filter,
                        context.target,
                    );
                    match backend_plans {
                        Ok(mut backend_plans) => {
                            append_operations(
                                &mut operations,
                                backend_plans.plans,
                                &dependency_ids,
                            );
                            records.append(&mut backend_plans.records);
                        }
                        Err(error) => records.push(planning_failure(runtime, error)),
                    }
                    continue;
                }
                Err(error) => {
                    records.push(BackendOperationRecord {
                        backend_id: runtime.backend.id().to_owned(),
                        backend_name: runtime.backend.display_name().to_owned(),
                        action: None,
                        command: None,
                        status: OperationStatus::Failed,
                        message: Some(format!(
                            "could not plan required metadata refresh before upgrade: {error}"
                        )),
                        privilege_status: None,
                    });
                    continue;
                }
            }
        }

        let backend_plans = match capability {
            Capability::Update => runtime.backend.plan_update(
                &runtime.commands,
                context.runner,
                context.backend_filter,
                context.target,
            ),
            Capability::Upgrade => runtime.backend.plan_upgrade(
                &runtime.commands,
                context.runner,
                context.backend_filter,
                context.target,
            ),
            _ => unreachable!("maintenance only accepts update or upgrade"),
        };

        match backend_plans {
            Ok(mut backend_plans) => {
                append_operations(&mut operations, backend_plans.plans, &[]);
                records.append(&mut backend_plans.records);
            }
            Err(error) => {
                if matches!(error, crate::domain::AllpError::UnsupportedOperation { .. }) {
                    records.push(BackendOperationRecord {
                        backend_id: runtime.backend.id().to_owned(),
                        backend_name: runtime.backend.display_name().to_owned(),
                        action: None,
                        command: None,
                        status: OperationStatus::NotApplicable,
                        message: Some(error.to_string()),
                        privilege_status: None,
                    });
                } else {
                    records.push(BackendOperationRecord {
                        backend_id: runtime.backend.id().to_owned(),
                        backend_name: runtime.backend.display_name().to_owned(),
                        action: None,
                        command: None,
                        status: OperationStatus::Failed,
                        message: Some(error.to_string()),
                        privilege_status: None,
                    });
                }
            }
        }
    }

    if context.yes {
        for operation in &mut operations {
            if let Ok(runtime) = context.backend(&operation.plan.backend_id) {
                runtime
                    .backend
                    .authorize_noninteractive(&mut operation.plan);
            }
        }
    }
    let plans = operations
        .iter()
        .map(|operation| operation.plan.clone())
        .collect::<Vec<_>>();
    let mut selected = plans
        .iter()
        .map(|plan| plan.backend_name.clone())
        .collect::<Vec<_>>();
    selected.sort();
    selected.dedup();
    context
        .renderer
        .maintenance_title(operation_name, context.dry_run);
    context
        .renderer
        .environment_scan(context.discovery, operation_name, &selected);
    context
        .renderer
        .planned_operations(&plans, context.privilege_context);

    if plans.is_empty() {
        let report = MultiOperationReport {
            operation: operation_name.to_owned(),
            records,
        };
        update_phase(context, operation_name, "Phase 6: Summary");
        context
            .renderer
            .maintenance_summary(&report, context.verbose > 0, context.dry_run);
        return Ok(report);
    }

    if context.dry_run {
        for operation in operations {
            records.push(record_from_plan(
                operation.plan,
                OperationStatus::DryRun,
                None,
                context.privilege_context,
            ));
        }
        let report = MultiOperationReport {
            operation: operation_name.to_owned(),
            records,
        };
        update_phase(context, operation_name, "Phase 6: Summary");
        context
            .renderer
            .maintenance_summary(&report, context.verbose > 0, context.dry_run);
        return Ok(report);
    }

    update_phase(context, operation_name, "Phase 4: Confirmation");
    context.renderer.privilege_notice(
        &plans,
        context.no_interactive,
        context.privilege_context,
        context.root_context_notice_shown,
    );
    let prompt = if operation_name == "upgrade" {
        "Continue with upgrade?"
    } else {
        "Continue?"
    };
    let confirmed = confirm_execution(
        context.no_interactive,
        context.yes,
        ConfirmationRequest {
            prompt: prompt.to_owned(),
            default_yes: operation_name != "upgrade",
            non_interactive_hint: format!(
                "Review with:\n  allp {operation_name} --dry-run\n\nExecute explicitly with:\n  allp {operation_name} --yes"
            ),
        },
    )?;
    if !confirmed {
        context
            .renderer
            .info_message(&format!("{} cancelled by user", title_case(operation_name)));
        context.renderer.plain_message("0 commands executed");
        records.push(BackendOperationRecord {
            backend_id: operation_name.to_owned(),
            backend_name: title_case(operation_name),
            action: None,
            command: None,
            status: OperationStatus::Cancelled,
            message: Some("cancelled by user before execution".to_owned()),
            privilege_status: None,
        });
        let report = MultiOperationReport {
            operation: operation_name.to_owned(),
            records,
        };
        update_phase(context, operation_name, "Phase 6: Summary");
        context
            .renderer
            .maintenance_summary(&report, context.verbose > 0, context.dry_run);
        return Ok(report);
    }

    let mut privilege_session = PrivilegeSession::for_plans(&plans, context.privilege_context);
    let privilege_status = privilege_session.preflight(!context.no_interactive);
    if !privilege_status.permits_execution() {
        return finish_before_execution_for_privilege_failure(
            context,
            operation_name,
            records,
            operations,
            privilege_status,
        );
    }

    update_phase(context, operation_name, "Phase 5: Execution");
    let mut queue = VecDeque::from(operations);
    let mut total = queue.len();
    let mut live_tui = context.renderer.maintenance_tui(
        operation_name,
        total,
        context.no_interactive,
        context.no_tui,
    );
    let mut next_id = total;
    let mut index = 0;
    let mut failed = BTreeSet::new();
    while let Some(operation) = queue.pop_front() {
        index += 1;
        let operation_id = operation.id;
        let plan = operation.plan;
        let blocked_by = operation
            .depends_on
            .iter()
            .copied()
            .filter(|dependency| failed.contains(dependency))
            .collect::<Vec<_>>();
        if !blocked_by.is_empty() && !context.allow_stale_metadata {
            let record = record_from_plan(
                plan,
                OperationStatus::Deferred,
                Some("required metadata refresh failed; use --allow-stale-metadata to explicitly permit existing metadata".to_owned()),
                context.privilege_context,
            );
            if let Some(tui) = live_tui.as_mut() {
                tui.record_outcome(
                    index,
                    total,
                    &record.backend_name,
                    &record.status,
                    record.message.as_deref(),
                );
            }
            records.push(record);
            continue;
        }
        let mut privilege_status = privilege_session.validate_for(&plan);
        if privilege_status == PrivilegeStatus::CredentialExpired && !context.no_interactive {
            if let Some(tui) = live_tui.as_mut() {
                tui.prepare_for_prompt();
            }
            privilege_status = privilege_session.preflight(true);
            if privilege_status.permits_execution() {
                if let Some(tui) = live_tui.as_mut() {
                    tui.resume_after_prompt();
                }
            } else {
                // The progress line was deliberately cleared before sudo took
                // the terminal. Do not redraw it after a failed,
                // cancelled, or timed-out reauthentication: continue the
                // remaining report in the classic stream instead.
                live_tui = None;
            }
        }
        if !privilege_status.permits_execution() {
            failed.insert(operation_id);
            defer_pending_upgrade(&mut upgrades_pending_refresh, &plan, &mut records);
            let mut record = record_from_plan(
                plan,
                OperationStatus::Blocked,
                Some(privilege_status.message().to_owned()),
                context.privilege_context,
            );
            record.privilege_status = Some(privilege_status);
            if let Some(tui) = live_tui.as_mut() {
                tui.record_outcome(
                    index,
                    total,
                    &record.backend_name,
                    &record.status,
                    record.message.as_deref(),
                );
            } else {
                context.renderer.execution_finished(
                    index,
                    total,
                    &record.backend_name,
                    &record.status,
                    record.message.as_deref(),
                    Duration::ZERO,
                );
            }
            records.push(record);
            continue;
        }
        let runtime = context.backend(&plan.backend_id)?;
        let hook_runner = MaintenanceHookRunner::new(context.runner, &privilege_session);
        if let Err(error) = runtime.backend.validate_before_execution(
            &plan,
            &hook_runner,
            context.privilege_context,
        ) {
            failed.insert(operation_id);
            defer_pending_upgrade(&mut upgrades_pending_refresh, &plan, &mut records);
            let record = record_from_plan(
                plan,
                OperationStatus::Failed,
                Some(format!("pre-execution validation failed: {error}")),
                context.privilege_context,
            );
            if let Some(tui) = live_tui.as_mut() {
                tui.record_outcome(
                    index,
                    total,
                    &record.backend_name,
                    &record.status,
                    record.message.as_deref(),
                );
            }
            records.push(record);
            continue;
        }
        let command =
            render_execution_plan_with_privilege_session(&plan, context.privilege_context);
        if let Some(tui) = live_tui.as_mut() {
            tui.start_operation(index, total, &plan, context.privilege_context);
        } else {
            context.renderer.execution_started_with_privilege_session(
                index,
                total,
                &plan,
                context.privilege_context,
            );
        }
        let started = Instant::now();
        let execution = if let Some(tui) = live_tui.as_mut() {
            context.runner.execute_with_observer_and_privilege_session(
                &plan,
                &mut privilege_session,
                tui,
            )
        } else {
            context
                .runner
                .execute_with_privilege_session(&plan, &mut privilege_session)
        };
        match execution {
            Ok(ProcessExecutionOutcome::Process(status)) if status.success => {
                let hook_runner = MaintenanceHookRunner::new(context.runner, &privilege_session);
                let verification = context.backend(&plan.backend_id).and_then(|runtime| {
                    runtime
                        .backend
                        .post_execution_verification(&plan, &hook_runner)
                });
                let mut parsed = match verification {
                    Ok(Some(record)) => vec![record],
                    Err(error) => vec![BackendOperationRecord {
                        backend_id: plan.backend_id.clone(),
                        backend_name: plan.backend_name.clone(),
                        action: None,
                        command: None,
                        status: OperationStatus::Failed,
                        message: Some(format!("post-upgrade verification failed: {error}")),
                        privilege_status: None,
                    }],
                    Ok(None) => {
                        classify_success(context, &plan, &status, &command).unwrap_or_else(|| {
                            vec![BackendOperationRecord {
                                backend_id: plan.backend_id.clone(),
                                backend_name: plan.backend_name.clone(),
                                action: Some(plan.action.clone()),
                                command: Some(command.clone()),
                                status: OperationStatus::Completed,
                                message: None,
                                privilege_status: None,
                            }]
                        })
                    }
                };
                for record in &mut parsed {
                    if record.action.is_none() {
                        record.action = Some(plan.action.clone());
                    }
                    if record.command.is_none() {
                        record.command = Some(command.clone());
                    }
                }
                if let Some(first) = parsed.first() {
                    if let Some(tui) = live_tui.as_mut() {
                        tui.finish_operation(
                            index,
                            total,
                            &first.backend_name,
                            &first.status,
                            first.message.as_deref(),
                            started.elapsed(),
                        );
                    } else {
                        context.renderer.execution_finished(
                            index,
                            total,
                            &first.backend_name,
                            &first.status,
                            first.message.as_deref(),
                            started.elapsed(),
                        );
                    }
                }
                persist_metadata_refresh_success(context, &plan);
                records.append(&mut parsed);
                if plan.operation == OperationKind::Update
                    && upgrades_pending_refresh.remove(&plan.backend_id)
                {
                    let runtime = context.backend(&plan.backend_id)?;
                    let hook_runner =
                        MaintenanceHookRunner::new(context.runner, &privilege_session);
                    match runtime.backend.plan_upgrade(
                        &runtime.commands,
                        &hook_runner,
                        context.backend_filter,
                        context.target,
                    ) {
                        Ok(mut follow_up) => {
                            if context.yes {
                                for plan in &mut follow_up.plans {
                                    runtime.backend.authorize_noninteractive(plan);
                                }
                            }
                            let follow_up_confirmed = if follow_up.plans.is_empty() {
                                false
                            } else {
                                if let Some(tui) = live_tui.as_mut() {
                                    tui.prepare_for_prompt();
                                }
                                context.renderer.planned_operations(
                                    &follow_up.plans,
                                    context.privilege_context,
                                );
                                let confirmation = confirm_follow_up(context, operation_name);
                                if let Some(tui) = live_tui.as_mut() {
                                    tui.resume_after_prompt();
                                }
                                confirmation?
                            };
                            if follow_up_confirmed {
                                let added = follow_up.plans.len();
                                for plan in follow_up.plans {
                                    queue.push_back(PlannedOperation {
                                        id: next_id,
                                        plan,
                                        depends_on: Vec::new(),
                                    });
                                    next_id += 1;
                                    total += 1;
                                }
                                if let Some(tui) = live_tui.as_mut() {
                                    tui.queue_extended(
                                        total,
                                        runtime.backend.display_name(),
                                        added,
                                    );
                                }
                            } else if !follow_up.plans.is_empty() {
                                records.push(BackendOperationRecord {
                                    backend_id: runtime.backend.id().to_owned(),
                                    backend_name: runtime.backend.display_name().to_owned(),
                                    action: None,
                                    command: None,
                                    status: OperationStatus::Cancelled,
                                    message: Some(
                                        "cancelled before package upgrade execution".to_owned(),
                                    ),
                                    privilege_status: None,
                                });
                            }
                            records.append(&mut follow_up.records);
                        }
                        Err(error) => records.push(planning_failure(runtime, error)),
                    }
                }
            }
            Ok(ProcessExecutionOutcome::Process(status)) => {
                failed.insert(operation_id);
                let error = classify_failure(context, &plan, &status);
                let cancelled = status.code == Some(130);
                let record = BackendOperationRecord {
                    backend_id: plan.backend_id.clone(),
                    backend_name: plan.backend_name.clone(),
                    action: Some(plan.action.clone()),
                    command: Some(command),
                    status: if cancelled {
                        OperationStatus::Cancelled
                    } else if matches!(&error, Some(crate::domain::AllpError::BackendBusy { .. })) {
                        OperationStatus::Busy
                    } else {
                        OperationStatus::Failed
                    },
                    message: Some(if cancelled {
                        "interrupted by user".to_owned()
                    } else {
                        error.map(|error| error.to_string()).unwrap_or_else(|| {
                            format!(
                                "native command exited with status {}",
                                status
                                    .code
                                    .map(|code| code.to_string())
                                    .unwrap_or_else(|| "unknown".to_owned())
                            )
                        })
                    }),
                    privilege_status: None,
                };
                if let Some(tui) = live_tui.as_mut() {
                    tui.finish_operation(
                        index,
                        total,
                        &record.backend_name,
                        &record.status,
                        record.message.as_deref(),
                        started.elapsed(),
                    );
                } else {
                    context.renderer.execution_finished(
                        index,
                        total,
                        &record.backend_name,
                        &record.status,
                        record.message.as_deref(),
                        started.elapsed(),
                    );
                }
                records.push(record);
                defer_pending_upgrade(&mut upgrades_pending_refresh, &plan, &mut records);
            }
            Ok(ProcessExecutionOutcome::PrivilegeBlocked(privilege_status)) => {
                failed.insert(operation_id);
                defer_pending_upgrade(&mut upgrades_pending_refresh, &plan, &mut records);
                let record = BackendOperationRecord {
                    backend_id: plan.backend_id,
                    backend_name: plan.backend_name,
                    action: Some(plan.action),
                    command: Some(command),
                    status: OperationStatus::Blocked,
                    message: Some(privilege_status.message().to_owned()),
                    privilege_status: Some(privilege_status),
                };
                if let Some(tui) = live_tui.as_mut() {
                    tui.finish_operation(
                        index,
                        total,
                        &record.backend_name,
                        &record.status,
                        record.message.as_deref(),
                        started.elapsed(),
                    );
                } else {
                    context.renderer.execution_finished(
                        index,
                        total,
                        &record.backend_name,
                        &record.status,
                        record.message.as_deref(),
                        started.elapsed(),
                    );
                }
                records.push(record);
            }
            Err(error) => {
                failed.insert(operation_id);
                defer_pending_upgrade(&mut upgrades_pending_refresh, &plan, &mut records);
                let record = BackendOperationRecord {
                    backend_id: plan.backend_id,
                    backend_name: plan.backend_name,
                    action: Some(plan.action),
                    command: Some(command),
                    status: OperationStatus::Failed,
                    message: Some(error.to_string()),
                    privilege_status: None,
                };
                if let Some(tui) = live_tui.as_mut() {
                    tui.finish_operation(
                        index,
                        total,
                        &record.backend_name,
                        &record.status,
                        record.message.as_deref(),
                        started.elapsed(),
                    );
                } else {
                    context.renderer.execution_finished(
                        index,
                        total,
                        &record.backend_name,
                        &record.status,
                        record.message.as_deref(),
                        started.elapsed(),
                    );
                }
                records.push(record);
            }
        }
    }

    let report = MultiOperationReport {
        operation: operation_name.to_owned(),
        records,
    };
    if let Some(tui) = live_tui.as_mut() {
        tui.finish();
    }
    update_phase(context, operation_name, "Phase 6: Summary");
    context
        .renderer
        .maintenance_summary(&report, context.verbose > 0, context.dry_run);
    Ok(report)
}

fn finish_before_execution_for_privilege_failure(
    context: &OperationContext<'_>,
    operation_name: &str,
    mut records: Vec<BackendOperationRecord>,
    operations: Vec<PlannedOperation>,
    privilege_status: PrivilegeStatus,
) -> AllpResult<MultiOperationReport> {
    context
        .renderer
        .warn(&format!("{}.", title_case(privilege_status.message())));
    for operation in operations {
        let requires_administrator = operation
            .plan
            .privilege
            .requires_sudo(context.privilege_context);
        let (status, message) = if requires_administrator {
            (
                OperationStatus::Blocked,
                privilege_status.message().to_owned(),
            )
        } else {
            (
                OperationStatus::Deferred,
                format!(
                    "execution did not start because {}",
                    privilege_status.message()
                ),
            )
        };
        let mut record = record_from_plan(
            operation.plan,
            status,
            Some(message),
            context.privilege_context,
        );
        if requires_administrator {
            record.privilege_status = Some(privilege_status);
        }
        records.push(record);
    }

    let report = MultiOperationReport {
        operation: operation_name.to_owned(),
        records,
    };
    update_phase(context, operation_name, "Phase 6: Summary");
    context
        .renderer
        .maintenance_summary(&report, context.verbose > 0, context.dry_run);
    Ok(report)
}

fn confirm_follow_up(context: &OperationContext<'_>, operation_name: &str) -> AllpResult<bool> {
    confirm_execution(
        context.no_interactive,
        context.yes,
        ConfirmationRequest {
            prompt: format!("Continue with {operation_name}?"),
            default_yes: false,
            non_interactive_hint: format!(
                "Review with:\n  allp {operation_name} --dry-run\n\nExecute explicitly with:\n  allp {operation_name} --yes"
            ),
        },
    )
}

fn defer_pending_upgrade(
    pending: &mut BTreeSet<String>,
    plan: &ExecutionPlan,
    records: &mut Vec<BackendOperationRecord>,
) {
    if plan.operation == OperationKind::Update && pending.remove(&plan.backend_id) {
        records.push(BackendOperationRecord {
            backend_id: plan.backend_id.clone(),
            backend_name: plan.backend_name.clone(),
            action: Some("Upgrade installed packages".to_owned()),
            command: None,
            status: OperationStatus::Deferred,
            message: Some("required metadata refresh failed".to_owned()),
            privilege_status: None,
        });
    }
}

fn append_operations(
    operations: &mut Vec<PlannedOperation>,
    plans: Vec<ExecutionPlan>,
    depends_on: &[usize],
) -> Vec<usize> {
    let mut ids = Vec::with_capacity(plans.len());
    for plan in plans {
        let id = operations.len();
        operations.push(PlannedOperation {
            id,
            plan,
            depends_on: depends_on.to_vec(),
        });
        ids.push(id);
    }
    ids
}

fn planning_failure(
    runtime: &crate::discovery::DetectedBackend,
    error: crate::domain::AllpError,
) -> BackendOperationRecord {
    BackendOperationRecord {
        backend_id: runtime.backend.id().to_owned(),
        backend_name: runtime.backend.display_name().to_owned(),
        action: None,
        command: None,
        status: if matches!(
            &error,
            crate::domain::AllpError::UnsupportedOperation { .. }
        ) {
            OperationStatus::NotApplicable
        } else {
            OperationStatus::Failed
        },
        message: Some(error.to_string()),
        privilege_status: None,
    }
}

fn update_phase(context: &OperationContext<'_>, operation_name: &str, label: &str) {
    if operation_name == "update" {
        context.renderer.phase(label);
    }
}

fn classify_success(
    context: &OperationContext<'_>,
    plan: &ExecutionPlan,
    status: &crate::execution::ProcessStatus,
    command: &str,
) -> Option<Vec<BackendOperationRecord>> {
    let runtime = context.backend(&plan.backend_id).ok()?;
    runtime
        .backend
        .classify_execution_success(plan, status, command)
}

fn classify_failure(
    context: &OperationContext<'_>,
    plan: &ExecutionPlan,
    status: &crate::execution::ProcessStatus,
) -> Option<crate::domain::AllpError> {
    let runtime = context.backend(&plan.backend_id).ok()?;
    let command = render_execution_plan_with_context(plan, context.privilege_context);
    runtime
        .backend
        .classify_execution_failure(plan, status, &command)
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn should_skip_operation(
    requested: Capability,
    operation_capability: BackendOperationCapability,
) -> bool {
    match (requested, operation_capability) {
        (_, BackendOperationCapability::Unsupported | BackendOperationCapability::SelfUpdate) => {
            true
        }
        (Capability::Update, BackendOperationCapability::MetadataRefresh) => false,
        (Capability::Update, _) => true,
        (Capability::Upgrade, BackendOperationCapability::InstalledPackageUpgrade)
        | (Capability::Upgrade, BackendOperationCapability::CombinedRefreshAndUpgrade) => false,
        (Capability::Upgrade, _) => true,
        _ => true,
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MetadataRefreshState {
    refreshed_at_unix_seconds: BTreeMap<String, u64>,
}

fn metadata_refresh_is_stale(state_dir: &Path, backend_id: &str) -> bool {
    let path = state_dir.join(METADATA_REFRESH_STATE_FILE);
    let Ok(Some(state)) = state::read_json::<MetadataRefreshState>(&path) else {
        return true;
    };
    let Some(timestamp) = state.refreshed_at_unix_seconds.get(backend_id) else {
        return true;
    };
    let now = unix_timestamp();
    now.saturating_sub(*timestamp) >= METADATA_REFRESH_FRESH_FOR.as_secs()
}

fn persist_metadata_refresh_success(context: &OperationContext<'_>, plan: &ExecutionPlan) {
    if plan.operation != OperationKind::Update {
        return;
    }
    let Ok(runtime) = context.backend(&plan.backend_id) else {
        return;
    };
    if runtime.backend.operation_capability(Capability::Update)
        != BackendOperationCapability::MetadataRefresh
    {
        return;
    }

    let path = context.state_dir.join(METADATA_REFRESH_STATE_FILE);
    let mut persisted = state::read_json::<MetadataRefreshState>(&path)
        .ok()
        .flatten()
        .unwrap_or_default();
    persisted
        .refreshed_at_unix_seconds
        .insert(plan.backend_id.clone(), unix_timestamp());
    if let Err(error) = state::write_json_atomically(&path, &persisted) {
        context.renderer.warn(&format!(
            "Could not persist {} metadata refresh timestamp: {error}",
            plan.backend_name
        ));
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn record_from_plan(
    plan: ExecutionPlan,
    status: OperationStatus,
    message: Option<String>,
    context: &crate::domain::RuntimePrivilegeContext,
) -> BackendOperationRecord {
    let command = render_execution_plan_with_context(&plan, context);
    BackendOperationRecord {
        backend_id: plan.backend_id,
        backend_name: plan.backend_name,
        action: Some(plan.action),
        command: Some(command),
        status,
        message,
        privilege_status: None,
    }
}
