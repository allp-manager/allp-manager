use super::{read_profile, Profile};
use crate::{
    cli::{confirm_execution, ConfirmationRequest},
    discovery::DetectedBackendSet,
    domain::{AllpError, AllpResult, Capability},
    operations::OperationContext,
};
use std::{collections::BTreeSet, path::Path};

pub(super) fn selected_profile(
    config: &Path,
    name: &str,
    without: &[String],
) -> AllpResult<Profile> {
    if without
        .iter()
        .any(|backend| !matches!(backend.as_str(), "snap" | "flatpak"))
    {
        return Err(AllpError::InvalidInput(
            "--without accepts only snap or flatpak".to_owned(),
        ));
    }
    let mut profile = read_profile(config, name)?;
    for package in &mut profile.packages {
        package.backend.make_ascii_lowercase();
        if let Some(installer) = &mut package.installer {
            installer.make_ascii_lowercase();
        }
    }
    profile.packages.retain(|p| !without.contains(&p.backend));
    Ok(profile)
}

/// Validate all owners before installing any prerequisites or applications.
pub fn missing_install_backends(
    config: &Path,
    name: &str,
    without: &[String],
    backends: &DetectedBackendSet,
) -> AllpResult<Vec<String>> {
    let profile = selected_profile(config, name, without)?;
    let mut missing = BTreeSet::new();
    for package in &profile.packages {
        match backends.get(&package.backend) {
            None if matches!(package.backend.as_str(), "snap" | "flatpak") => { missing.insert(package.backend.clone()); }
            None => return Err(AllpError::InvalidInput(format!("profile '{name}' requires unavailable backend '{}'; convert the profile for this system first; no packages were installed", package.backend))),
            Some(runtime) if !runtime.backend.has_capability(Capability::Install) => return Err(AllpError::InvalidInput(format!("{} does not support installation; no packages were installed", package.backend))),
            Some(runtime) => {
                if let Some(installer) = &package.installer {
                    if !runtime.backend.aliases().contains(&installer.as_str()) || (installer != "pip" && !runtime.commands.contains_key(installer)) {
                        return Err(AllpError::InvalidInput(format!("{} installer {installer} is unavailable; no packages were installed", package.backend)));
                    }
                }
            }
        }
    }
    Ok(missing.into_iter().collect())
}

pub(super) fn prepare_remotes(
    context: &OperationContext<'_>,
    profile: &Profile,
) -> AllpResult<bool> {
    let Some(runtime) = context.backends.get("flatpak") else {
        return Ok(true);
    };
    if !profile.packages.iter().any(|p| p.backend == "flatpak") {
        return Ok(true);
    }
    // The adapter's configured prerequisite is user Flathub. Never substitute
    // that remote for a custom or system-scoped source from an inventory.
    let unconfigured = context.discovery.entries.iter().any(|e| {
        e.backend_id == "flatpak"
            && e.state == crate::discovery::DetectionState::FoundButUnconfigured
    });
    if !unconfigured {
        return Ok(true);
    }
    if profile
        .packages
        .iter()
        .filter(|p| p.backend == "flatpak")
        .any(|p| {
            p.flatpak.as_ref().is_some_and(|f| {
                f.scope != super::FlatpakScope::User
                    || f.remote.as_deref().is_some_and(|r| r != "flathub")
            })
        })
    {
        return Err(AllpError::InvalidInput("saved Flatpak source needs manual remote configuration in its original scope; no applications were installed".to_owned()));
    }
    let plan = runtime
        .backend
        .plan_search_prerequisite(&runtime.commands)?
        .ok_or_else(|| {
            AllpError::InvalidInput("Flatpak remote configuration is unavailable".to_owned())
        })?;
    context
        .renderer
        .planned_operations(std::slice::from_ref(&plan), context.privilege_context);
    if context.dry_run {
        context.renderer.info_message("Dry run: configure this remote first, then rerun profile install to resolve applications. No command was executed.");
        return Ok(false);
    }
    if !confirm_execution(
        context.no_interactive,
        context.yes && context.allow_bootstrap,
        ConfirmationRequest {
            prompt: "This profile contains Flatpak apps. Configure Flathub before installing them?"
                .to_owned(),
            default_yes: false,
            non_interactive_hint:
                "Review the plan, then use --yes --allow-bootstrap, or --without flatpak."
                    .to_owned(),
        },
    )? {
        return Err(AllpError::InvalidInput(
            "Flatpak setup cancelled; no applications were installed".to_owned(),
        ));
    }
    let status = context.runner.execute(&plan)?;
    if !status.success {
        return Err(AllpError::CommandFailed {
            backend: plan.backend_name.clone(),
            command: crate::execution::render_execution_plan_with_context(
                &plan,
                context.privilege_context,
            ),
            code: status.code,
            stderr: status.stderr,
        });
    }
    if !runtime
        .backend
        .verify_search_prerequisite(&runtime.commands, context.runner)?
    {
        return Err(AllpError::InvalidInput(
            "Flatpak setup could not be verified; no applications were installed".to_owned(),
        ));
    }
    Ok(true)
}
