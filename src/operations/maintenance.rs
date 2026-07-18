use crate::{
    backends::BackendOperationCapability,
    cli::{confirm_execution, ConfirmationRequest},
    domain::{
        AllpResult, BackendOperationRecord, Capability, ExecutionPlan, MultiOperationReport,
        OperationKind, OperationStatus,
    },
    execution::render_execution_plan_with_context,
    operations::OperationContext,
    state,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const METADATA_REFRESH_FRESH_FOR: Duration = Duration::from_secs(6 * 60 * 60);
const METADATA_REFRESH_STATE_FILE: &str = "backend-metadata-refresh.json";

pub fn run(
    context: &OperationContext<'_>,
    capability: Capability,
    operation_name: &str,
) -> AllpResult<MultiOperationReport> {
    let mut records = Vec::new();
    let mut plans = Vec::new();

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
                    plans.append(&mut refresh_plans.plans);
                    records.append(&mut refresh_plans.records);
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
                plans.append(&mut backend_plans.plans);
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
                    });
                } else {
                    records.push(BackendOperationRecord {
                        backend_id: runtime.backend.id().to_owned(),
                        backend_name: runtime.backend.display_name().to_owned(),
                        action: None,
                        command: None,
                        status: OperationStatus::Failed,
                        message: Some(error.to_string()),
                    });
                }
            }
        }
    }

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
        for plan in plans {
            records.push(record_from_plan(
                plan,
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

    update_phase(context, operation_name, "Phase 5: Execution");
    let total = plans.len();
    for (offset, plan) in plans.into_iter().enumerate() {
        let index = offset + 1;
        let command = render_execution_plan_with_context(&plan, context.privilege_context);
        context
            .renderer
            .execution_started(index, total, &plan, context.privilege_context);
        let started = Instant::now();
        match context.runner.execute(&plan) {
            Ok(status) if status.success => {
                let mut parsed = classify_success(context, &plan, &status, &command)
                    .unwrap_or_else(|| {
                        vec![BackendOperationRecord {
                            backend_id: plan.backend_id.clone(),
                            backend_name: plan.backend_name.clone(),
                            action: Some(plan.action.clone()),
                            command: Some(command.clone()),
                            status: OperationStatus::Completed,
                            message: None,
                        }]
                    });
                for record in &mut parsed {
                    if record.action.is_none() {
                        record.action = Some(plan.action.clone());
                    }
                    if record.command.is_none() {
                        record.command = Some(command.clone());
                    }
                }
                if let Some(first) = parsed.first() {
                    context.renderer.execution_finished(
                        index,
                        total,
                        &first.backend_name,
                        &first.status,
                        first.message.as_deref(),
                        started.elapsed(),
                    );
                }
                persist_metadata_refresh_success(context, &plan);
                records.append(&mut parsed);
            }
            Ok(status) => {
                let error = classify_failure(context, &plan, &status);
                let record = BackendOperationRecord {
                    backend_id: plan.backend_id.clone(),
                    backend_name: plan.backend_name.clone(),
                    action: Some(plan.action.clone()),
                    command: Some(command),
                    status: if error.is_some() {
                        OperationStatus::Busy
                    } else {
                        OperationStatus::Failed
                    },
                    message: Some(error.map(|error| error.to_string()).unwrap_or_else(|| {
                        format!(
                            "native command exited with status {}",
                            status
                                .code
                                .map(|code| code.to_string())
                                .unwrap_or_else(|| "unknown".to_owned())
                        )
                    })),
                };
                context.renderer.execution_finished(
                    index,
                    total,
                    &record.backend_name,
                    &record.status,
                    record.message.as_deref(),
                    started.elapsed(),
                );
                records.push(record);
            }
            Err(error) => {
                let record = BackendOperationRecord {
                    backend_id: plan.backend_id,
                    backend_name: plan.backend_name,
                    action: Some(plan.action),
                    command: Some(command),
                    status: OperationStatus::Failed,
                    message: Some(error.to_string()),
                };
                context.renderer.execution_finished(
                    index,
                    total,
                    &record.backend_name,
                    &record.status,
                    record.message.as_deref(),
                    started.elapsed(),
                );
                records.push(record);
            }
        }
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
    }
}
