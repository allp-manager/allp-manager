use crate::{
    cli::{select_candidate, select_installed},
    domain::{AllpError, AllpResult, Capability, PackageInfo, SearchScope},
    operations::{
        list,
        search::{self, SearchPolicy},
        OperationContext,
    },
};

pub fn run(context: &OperationContext<'_>, package: &str, full: bool, raw: bool) -> AllpResult<()> {
    let installed_report = list::gather(context)?;
    for issue in &installed_report.issues {
        context
            .renderer
            .warn(&format!("{}: {}", issue.backend_name, issue.message));
    }
    let installed: Vec<_> = installed_report
        .packages
        .iter()
        .filter(|item| {
            item.package_id.eq_ignore_ascii_case(package)
                || item.display_name.eq_ignore_ascii_case(package)
        })
        .cloned()
        .collect();

    if !installed.is_empty() {
        if installed.len() > 1 && !context.renderer.json() {
            context.renderer.installed_choices(&installed);
        }
        if installed.len() > 1 && context.no_interactive {
            return Err(AllpError::AmbiguousSelection(
                "Multiple installed packages matched.\n\nUse --from with a backend ID or provide an exact package ID."
                    .to_owned(),
            ));
        }
        let index = select_installed(&installed, context.no_interactive)?;
        let selected = &installed[index];
        let runtime = context.backend(&selected.backend_id)?;
        if raw && !context.renderer.json() {
            let raw = runtime.backend.raw_info(
                &runtime.commands,
                context.runner,
                &selected.package_id,
            )?;
            context.renderer.raw_info(&raw);
            return Ok(());
        }
        let mut info =
            runtime
                .backend
                .info(&runtime.commands, context.runner, &selected.package_id)?;
        info.installed = Some(true);
        context.renderer.info(&info, full);
        return Ok(());
    }

    let search_report = search::gather_with_policy(
        context,
        package,
        SearchPolicy {
            required_capability: Some(Capability::Info),
            scope: SearchScope::AllSources,
            ..SearchPolicy::default()
        },
    )?;
    for issue in &search_report.issues {
        context
            .renderer
            .warn(&format!("{}: {}", issue.backend_name, issue.message));
    }
    if search_report.candidates.is_empty() {
        return Err(AllpError::PackageNotFound(package.to_owned()));
    }

    let selectable = search_report.candidates;

    if !context.renderer.json() {
        context
            .renderer
            .candidates(&selectable, SearchScope::AllSources);
    }
    if selectable.len() > 1 && context.no_interactive {
        return Err(AllpError::AmbiguousSelection(
            "Multiple package information sources were found.\n\nUse --from with a backend ID or provide an exact package ID."
                .to_owned(),
        ));
    }
    let index = select_candidate(&selectable, context.no_interactive)?;
    let candidate = &selectable[index];
    let runtime = context.backend(&candidate.backend_id)?;

    if raw && !context.renderer.json() {
        let raw =
            runtime
                .backend
                .raw_info(&runtime.commands, context.runner, &candidate.package_id)?;
        context.renderer.raw_info(&raw);
        return Ok(());
    }

    let info = runtime
        .backend
        .info(&runtime.commands, context.runner, &candidate.package_id)
        .unwrap_or_else(|_| PackageInfo {
            backend_id: candidate.backend_id.clone(),
            backend_name: candidate.backend_name.clone(),
            category: candidate.category,
            domain: candidate.domain,
            package_id: candidate.package_id.clone(),
            display_name: candidate.display_name.clone(),
            version: candidate.version.clone(),
            description: candidate.description.clone(),
            source: candidate.source.clone(),
            scope: candidate.scope.clone(),
            artifact_kind: Some(candidate.artifact_kind.clone()),
            installed: Some(false),
            extra: vec![("artifact_kind".to_owned(), candidate.artifact_kind.clone())],
        });

    let mut info = info;
    if info.installed.is_none() {
        info.installed = Some(false);
    }
    context.renderer.info(&info, full);
    Ok(())
}
