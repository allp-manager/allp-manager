use crate::{
    discovery::{DetectionState, DiscoveryReport},
    domain::{
        BackendCategory, BackendIssue, ExecutionPlan, InstalledPackage, MatchKind,
        MultiOperationReport, NativeCommand, PackageCandidate, PackageInfo, ResultSection,
        RuntimePrivilegeContext, SearchReport, SearchScope,
    },
    execution::{render_execution_plan_with_context, render_native_argv, render_native_command},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal, Write},
};

#[derive(Debug, Clone)]
pub struct Renderer {
    color: bool,
    json: bool,
}

impl Renderer {
    pub fn new(no_color: bool, json: bool) -> Self {
        Self {
            color: !no_color
                && !json
                && io::stdout().is_terminal()
                && std::env::var_os("NO_COLOR").is_none()
                && std::env::var("TERM")
                    .map(|term| term != "dumb")
                    .unwrap_or(true),
            json,
        }
    }

    pub fn json(&self) -> bool {
        self.json
    }
    pub fn spinner_enabled(&self) -> bool {
        self.color && !self.json
    }

    #[cfg(test)]
    fn with_color_for_test(color: bool, json: bool) -> Self {
        Self { color, json }
    }

    pub fn render_json<T: Serialize + ?Sized>(&self, value: &T) {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_owned())
        );
    }

    pub fn render_json_envelope<T, I>(&self, command: &str, complete: bool, results: &T, issues: &I)
    where
        T: Serialize + ?Sized,
        I: Serialize + ?Sized,
    {
        #[derive(Serialize)]
        struct Envelope<'a, T: Serialize + ?Sized, I: Serialize + ?Sized> {
            schema_version: u8,
            command: &'a str,
            complete: bool,
            results: &'a T,
            issues: &'a I,
        }

        self.render_json(&Envelope {
            schema_version: 1,
            command,
            complete,
            results,
            issues,
        });
    }

    pub fn detection(&self, report: &DiscoveryReport, verbose: bool) {
        if self.json {
            let issues: &[BackendIssue] = &[];
            self.render_json_envelope("detect", true, &report.entries, &issues);
            return;
        }

        if verbose {
            self.detection_verbose(report);
            return;
        }

        println!("{}", self.heading("Package Managers"));
        for category in [
            BackendCategory::System,
            BackendCategory::Universal,
            BackendCategory::Development,
        ] {
            let entries: Vec<_> = report
                .entries
                .iter()
                .filter(|entry| entry.category == category)
                .collect();
            if entries.is_empty() {
                continue;
            }
            println!("\n{}", self.subheading(category.manager_label()));
            for entry in entries {
                let (symbol, state) = match entry.state {
                    DetectionState::Ready => (self.success("✔"), entry.state.label()),
                    DetectionState::NotFound => (self.muted("✖"), "Not installed"),
                    DetectionState::FoundButUnavailable
                    | DetectionState::FoundButUnconfigured
                    | DetectionState::UnsupportedVersion
                    | DetectionState::ProbeFailed => (self.warning("⚠"), entry.state.label()),
                };
                println!("{symbol} {:<10} {state}", entry.backend_name);
                if entry.state != DetectionState::NotFound {
                    if let Some(message) = &entry.message {
                        println!("  {message}");
                    }
                }
            }
        }
    }

    fn detection_verbose(&self, report: &DiscoveryReport) {
        println!("{}", self.heading("Package Managers"));
        for entry in &report.entries {
            println!("\n{}", self.subheading(&entry.backend_name));
            println!("Status: {}", entry.state.label());
            let capabilities = entry
                .capabilities
                .iter()
                .map(|capability| capability.label())
                .collect::<Vec<_>>()
                .join(", ");
            println!("Capabilities: {capabilities}");
            if entry.commands.is_empty() {
                println!("Commands: none resolved");
            } else {
                println!("Commands:");
                for (key, path) in &entry.commands {
                    println!("  {key:<11} {path}");
                }
            }
            println!(
                "Probe: {}",
                if entry.state == DetectionState::Ready {
                    "Passed"
                } else {
                    "Not ready"
                }
            );
            if let Some(message) = &entry.message {
                println!("Message: {message}");
            }
            if !entry.missing.is_empty() {
                println!("Missing: {}", entry.missing.join(", "));
            }
        }
    }

    pub fn detected_summary(&self, report: &DiscoveryReport) {
        if self.json {
            return;
        }
        let ready: Vec<_> = report
            .entries
            .iter()
            .filter(|entry| entry.state == DetectionState::Ready)
            .map(|entry| entry.backend_name.as_str())
            .collect();
        if ready.is_empty() {
            println!(
                "{} No supported package managers detected.",
                self.warning("⚠")
            );
        } else {
            println!("{} Detected: {}", self.info_style("ℹ"), ready.join(", "));
        }
    }

    pub fn environment_scan(
        &self,
        report: &DiscoveryReport,
        _operation: &str,
        selected: &[String],
    ) {
        if self.json {
            return;
        }
        let ready: Vec<_> = report
            .entries
            .iter()
            .filter(|entry| entry.state == DetectionState::Ready)
            .map(|entry| entry.backend_name.as_str())
            .collect();
        println!("{}", self.heading("Environment Scan"));
        if ready.is_empty() {
            println!("Detected and ready: none");
        } else {
            println!("Detected and ready: {}", ready.join(", "));
        }
        if selected.is_empty() {
            println!("Selected for execution: none");
        } else {
            println!("Selected for execution: {}", selected.join(", "));
        }
    }

    pub fn maintenance_title(&self, operation: &str, dry_run: bool) {
        if self.json {
            return;
        }
        let suffix = if dry_run { " · Dry Run" } else { "" };
        println!(
            "{}",
            self.heading(&format!("Allp {}{suffix}", title_case(operation)))
        );
    }

    pub fn planned_operations(&self, plans: &[ExecutionPlan], context: &RuntimePrivilegeContext) {
        if self.json {
            return;
        }
        let heading = if plans.len() == 1 {
            "Planned Operation"
        } else {
            "Planned Operations"
        };
        println!("\n{}", self.heading(heading));
        for (index, plan) in plans.iter().enumerate() {
            if plans.len() > 1 {
                println!("\n{}. {}", index + 1, self.bold(&plan.backend_name));
            } else {
                println!("\n{}", self.bold(&plan.backend_name));
            }
            println!("   Action: {}", plan.action);
            if let Some(package_id) = &plan.package_id {
                println!("   Package: {package_id}");
            }
            if let Some(source) = &plan.source {
                println!("   Source: {source}");
            }
            if let Some(scope) = &plan.scope {
                println!("   Target: {scope}");
            }
            for (key, value) in &plan.details {
                println!("   {key}: {value}");
            }
            println!(
                "   Command: {}",
                render_execution_plan_with_context(plan, context)
            );
            println!("   Privilege: {}", plan.privilege.label(context));
        }
    }

    pub fn privilege_notice(
        &self,
        plans: &[ExecutionPlan],
        no_interactive: bool,
        context: &RuntimePrivilegeContext,
        root_context_notice_shown: bool,
    ) {
        if self.json {
            return;
        }
        let root_required = plans
            .iter()
            .any(|plan| plan.privilege.requires_sudo(context));
        let direct_root = plans.iter().any(|plan| {
            plan.privilege == crate::domain::PrivilegeRequirement::RootRequired && context.is_root()
        });
        let original_user = plans
            .iter()
            .any(|plan| plan.privilege.requires_original_user(context));
        if !root_required && !direct_root && !original_user {
            return;
        }
        let _ = io::stdout().flush();
        if root_required && no_interactive {
            eprintln!(
                "{} Allp is running as your normal user. Root-required native child commands would be elevated after confirmation.",
                self.warning("⚠")
            );
        } else if root_required {
            eprintln!(
                "\n{} Administrator access is required for selected operations.\nAllp itself is running as your normal user.\nOnly the native child commands listed above will be elevated.\nYou may be asked for your sudo password.",
                self.warning("⚠")
            );
        } else if direct_root && !root_context_notice_shown {
            eprintln!(
                "{} Allp is running with administrator privileges. Root-required system operations will run directly.",
                self.warning("⚠")
            );
        }
        if original_user {
            eprintln!(
                "{} User-scoped operations will run as the original sudo user.",
                self.info_style("ℹ")
            );
        }
    }

    pub fn runtime_context_notice(&self, context: &RuntimePrivilegeContext) {
        if self.json || !context.is_root() {
            return;
        }
        eprintln!(
            "{} Allp is running with administrator privileges.\n  System operations may run directly as root.\n  User-scoped operations will run as the original user when possible.",
            self.warning("⚠")
        );
    }

    pub fn search(&self, report: &SearchReport) {
        if self.json {
            self.render_json_envelope(
                "search",
                report.complete,
                &report.candidates,
                &report.issues,
            );
            return;
        }

        if report.candidates.is_empty() {
            println!("No packages found for '{}'.", report.query);
        } else {
            println!("{}", self.heading("Search Results"));
            self.candidates(&report.candidates, SearchScope::AllSources);
        }

        for issue in &report.issues {
            eprintln!(
                "{} {}: {}",
                self.warning("⚠"),
                issue.backend_name,
                issue.message
            );
        }
        self.search_summary(report);
    }

    pub fn candidates(&self, candidates: &[PackageCandidate], scope: SearchScope) {
        if candidates
            .iter()
            .any(|candidate| matches!(candidate.match_kind, crate::domain::MatchKind::Related))
        {
            println!("Related matches may not represent the same software.");
        }

        self.grouped_candidates(candidates, scope);
    }

    pub fn install_sources(
        &self,
        query: &str,
        scope: SearchScope,
        candidates: &[PackageCandidate],
    ) {
        if self.json {
            return;
        }
        let title = match scope {
            SearchScope::AppsAndTools => "Apps and Tools Results",
            SearchScope::DeveloperEcosystems => "Developer Ecosystem Results",
            SearchScope::AllSources => "Search Results",
        };
        println!("{}", self.heading(&format!("{title} for \"{query}\"")));
        self.result_counts(candidates, scope);
        self.selection_warnings(query, scope, candidates);
        self.grouped_candidates(candidates, scope);
    }

    pub fn preflight_stage(&self, stage: &str, command: &NativeCommand, verbose: bool) {
        if self.json {
            return;
        }
        println!("\n{} {stage}", self.info_style("●"));
        println!("  Command: {}", render_native_command(command));
        if verbose {
            println!("  Argv: {}", render_native_argv(command));
        }
    }

    pub fn preflight_warning(&self, title: &str, message: &str) {
        if self.json || message.trim().is_empty() {
            return;
        }
        println!("{title}:");
        for line in message.trim().lines() {
            println!("  {line}");
        }
    }

    fn grouped_candidates(&self, candidates: &[PackageCandidate], scope: SearchScope) {
        for section in ResultSection::ordered_for_scope(scope) {
            let entries = candidates
                .iter()
                .enumerate()
                .filter(|(_, candidate)| candidate.result_section() == *section)
                .collect::<Vec<_>>();
            if entries.is_empty() {
                continue;
            }
            println!("\n{}", self.subheading(section.label()));
            for (index, candidate) in entries {
                let version = candidate.version.as_deref().unwrap_or("unknown");
                let source = candidate.source.as_deref().unwrap_or("unknown source");
                println!(
                    "[{}] {:<12} {:<32} {:<18} {}",
                    index + 1,
                    self.bold(&candidate.backend_name),
                    candidate.package_id,
                    candidate.identity.label(),
                    version
                );
                println!(
                    "    source: {source} · type: {} · scope: {}",
                    candidate.artifact_kind,
                    candidate.scope.as_deref().unwrap_or("unknown")
                );
                if let Some(canonical) = &candidate.identity.canonical_name {
                    println!(
                        "    identity: {canonical} · confidence: {:?} · relationship: {:?}",
                        candidate.identity.confidence, candidate.identity.distribution
                    );
                }
                if !candidate.installers.is_empty() {
                    println!("    installers: {}", candidate.installers.join(", "));
                }
                if let Some(warning) = &candidate.identity.warning {
                    println!("    warning: {warning}");
                }
                if candidate.display_name != candidate.package_id {
                    println!("    Name: {}", candidate.display_name);
                }
                if let Some(remote) = candidate.metadata.get("flatpak.remote") {
                    println!("    Remote: {remote}");
                }
                println!("    Type: {}", candidate.artifact_kind);
                if let Some(description) = &candidate.description {
                    println!("    {}", description);
                }
            }
        }
    }

    fn search_summary(&self, report: &SearchReport) {
        if report.backend_summaries.is_empty() {
            return;
        }

        println!("\n{}", self.heading("Search Summary"));
        for summary in &report.backend_summaries {
            match summary.state {
                crate::domain::SearchBackendState::ParsedResults => {
                    println!(
                        "{} {:<8} {} {}",
                        self.success("✔"),
                        summary.backend_name,
                        summary.result_count,
                        plural(summary.result_count, "result", "results")
                    );
                }
                crate::domain::SearchBackendState::NoMatches => {
                    println!("{} {} · no matches", self.muted("○"), summary.backend_name);
                }
                crate::domain::SearchBackendState::NoConfiguredRemotes => {
                    println!(
                        "{} {} skipped · no configured remotes",
                        self.muted("○"),
                        summary.backend_name
                    );
                }
                crate::domain::SearchBackendState::SearchFailed => {
                    println!(
                        "{} {} search failed · {}",
                        self.error("✖"),
                        summary.backend_name,
                        summary.message.as_deref().unwrap_or("search failed")
                    );
                }
                crate::domain::SearchBackendState::Unavailable => {
                    if let Some(message) = &summary.message {
                        println!(
                            "{} {} unavailable · {message}",
                            self.muted("○"),
                            summary.backend_name
                        );
                    } else {
                        println!("{} {} unavailable", self.muted("○"), summary.backend_name);
                    }
                }
                crate::domain::SearchBackendState::Available => {
                    println!("{} {} available", self.muted("○"), summary.backend_name);
                }
            }
        }
    }

    fn result_counts(&self, candidates: &[PackageCandidate], scope: SearchScope) {
        println!(
            "\n{} {} relevant result(s) found",
            self.success("✔"),
            candidates.len()
        );
        for section in ResultSection::ordered_for_scope(scope) {
            let count = candidates
                .iter()
                .filter(|candidate| candidate.result_section() == *section)
                .count();
            if count > 0 {
                println!("{:<24} {count}", format!("{}:", section.label()));
            }
        }
        println!("{:<24} {}", "Total:", candidates.len());
    }

    fn selection_warnings(&self, query: &str, scope: SearchScope, candidates: &[PackageCandidate]) {
        if !candidates
            .iter()
            .any(|candidate| candidate.match_kind == MatchKind::Exact)
        {
            println!(
                "\n{} No exact match found for \"{query}\".",
                self.warning("⚠")
            );
        }
        if candidates
            .iter()
            .any(|candidate| matches!(candidate.match_kind, MatchKind::Related | MatchKind::Fuzzy))
        {
            println!(
                "{} Related or fuzzy matches may not represent the same software.",
                self.warning("⚠")
            );
        }
        if candidates
            .iter()
            .any(|candidate| candidate.identity.is_conflicting())
        {
            println!(
                "{} Some exact package names conflict with the requested software identity.",
                self.warning("⚠")
            );
        }
        let has_registry_candidates = candidates
            .iter()
            .any(|candidate| candidate.result_section() == ResultSection::DeveloperEcosystems);
        if has_registry_candidates
            && !candidates
                .iter()
                .any(|candidate| candidate.match_kind == MatchKind::Exact)
        {
            println!(
                "{} Related or fuzzy registry packages may be unofficial, unrelated, abandoned, typosquatted, or malicious.",
                self.warning("⚠")
            );
        }
        if scope == SearchScope::AllSources {
            let sections = candidates
                .iter()
                .map(PackageCandidate::result_section)
                .collect::<std::collections::BTreeSet<_>>();
            if sections.len() > 1 {
                println!(
                    "{} Similar names across package sources and programming ecosystems do not imply the same software.",
                    self.warning("⚠")
                );
            }
        }
    }

    pub fn installed(&self, packages: &[InstalledPackage]) {
        if self.json {
            let issues: &[BackendIssue] = &[];
            self.render_json_envelope("list", true, packages, &issues);
            return;
        }

        let mut by_backend: BTreeMap<&str, Vec<&InstalledPackage>> = BTreeMap::new();
        for package in packages {
            by_backend
                .entry(&package.backend_name)
                .or_default()
                .push(package);
        }

        for (backend, packages) in by_backend {
            println!("\n{}", self.subheading(backend));
            for (index, package) in packages.iter().enumerate() {
                println!(
                    "[{}] {:<36} {}",
                    index + 1,
                    package.package_id,
                    package.version.as_deref().unwrap_or("unknown")
                );
            }
        }
    }

    pub fn installed_choices(&self, packages: &[InstalledPackage]) {
        if self.json {
            let issues: &[BackendIssue] = &[];
            self.render_json_envelope("installed_choices", true, packages, &issues);
            return;
        }
        println!("{}", self.heading("Installed copies found"));
        for (index, package) in packages.iter().enumerate() {
            println!(
                "[{}] {} · {} · {} · {}",
                index + 1,
                package.backend_name,
                package.package_id,
                package.version.as_deref().unwrap_or("unknown"),
                package.scope.as_deref().unwrap_or("unknown scope")
            );
        }
    }

    pub fn info(&self, info: &PackageInfo, full: bool) {
        if self.json {
            let issues: &[BackendIssue] = &[];
            self.render_json_envelope("info", true, info, &issues);
            return;
        }

        println!("{}", self.heading("Package Information"));
        println!("\n{}", self.subheading(&info.display_name));
        println!("Backend:       {}", info.backend_name);
        println!("Package ID:    {}", info.package_id);
        println!(
            "Version:       {}",
            info.version.as_deref().unwrap_or("unknown")
        );
        println!(
            "Installed:     {}",
            match info.installed {
                Some(true) => "Yes",
                Some(false) => "No",
                None => "Unknown",
            }
        );
        if let Some(architecture) = extra_value(info, "Architecture") {
            println!("Architecture:  {architecture}");
        }
        println!(
            "Source:        {}",
            info.source.as_deref().unwrap_or("unknown")
        );
        if let Some(homepage) = extra_value(info, "Homepage") {
            println!("Homepage:      {homepage}");
        }
        println!(
            "Type:          {}",
            info.artifact_kind.as_deref().unwrap_or("unknown")
        );
        println!(
            "Scope:         {}",
            info.scope.as_deref().unwrap_or("unknown")
        );
        if let Some(description) = &info.description {
            println!("\n{}", self.subheading("Description"));
            println!("{}", wrap_text(description, 88));
        }

        if full && !info.extra.is_empty() {
            println!("\n{}", self.subheading("Extended Metadata"));
            for (key, value) in &info.extra {
                println!("{key}: {value}");
            }
        }
    }

    pub fn raw_info(&self, raw: &str) {
        if !self.json {
            print!("{raw}");
            if !raw.ends_with('\n') {
                println!();
            }
        }
    }

    pub fn plan(&self, plan: &ExecutionPlan, context: &RuntimePrivilegeContext) {
        let command = render_execution_plan_with_context(plan, context);
        if self.json {
            #[derive(Serialize)]
            struct PlanView<'a> {
                backend_id: &'a str,
                backend_name: &'a str,
                operation: &'a str,
                action: &'a str,
                package_id: &'a Option<String>,
                source: &'a Option<String>,
                scope: &'a Option<String>,
                details: &'a [(String, String)],
                native_command: String,
                privilege: &'a str,
                interactive: bool,
            }
            self.render_json(&PlanView {
                backend_id: &plan.backend_id,
                backend_name: &plan.backend_name,
                operation: plan.operation.as_str(),
                action: &plan.action,
                package_id: &plan.package_id,
                source: &plan.source,
                scope: &plan.scope,
                details: &plan.details,
                native_command: command,
                privilege: plan.privilege.label(context),
                interactive: plan.interactive,
            });
            return;
        }

        println!(
            "\n{} {}",
            self.info_style("ℹ"),
            self.bold(&plan.backend_name)
        );
        println!("Action: {}", plan.action);
        if let Some(package_id) = &plan.package_id {
            println!("Package: {package_id}");
        }
        println!("Native command:\n  {command}");
    }

    pub fn already_installed(
        &self,
        backend: &str,
        package_id: &str,
        installed_version: Option<&str>,
        candidate_version: Option<&str>,
    ) {
        if self.json {
            #[derive(Serialize)]
            struct AlreadyInstalledView<'a> {
                status: &'a str,
                backend: &'a str,
                package_id: &'a str,
                installed_version: Option<&'a str>,
                candidate_version: Option<&'a str>,
            }
            self.render_json(&AlreadyInstalledView {
                status: "already_installed",
                backend,
                package_id,
                installed_version,
                candidate_version,
            });
            return;
        }
        if candidate_version.is_some()
            && installed_version.is_some()
            && candidate_version != installed_version
        {
            println!(
                "{} {package_id} is already installed, but a different candidate version is available.",
                self.warning("⚠")
            );
        } else {
            println!("{} {package_id} is already installed", self.success("✔"));
        }
        println!("\nBackend: {backend}");
        if let Some(version) = installed_version {
            println!("Installed version: {version}");
        }
        if let Some(version) = candidate_version {
            println!("Candidate version: {version}");
        }
        println!("\nNothing to install.");
    }

    pub fn multi_operation(&self, report: &MultiOperationReport) {
        if self.json {
            self.render_json_envelope(
                &report.operation,
                !report.has_failures(),
                &report.records,
                &[] as &[BackendIssue],
            );
            return;
        }

        println!(
            "\n{}",
            self.heading(&format!("{} summary", report.operation))
        );
        for record in &report.records {
            let marker = self.status_marker(&record.status);
            println!(
                "{marker} {:<10} {}",
                record.backend_name,
                record.status.label()
            );
            if let Some(action) = &record.action {
                println!("  Action: {action}");
            }
            if let Some(command) = &record.command {
                println!("  Command: {command}");
            }
            if let Some(message) = &record.message {
                println!("  {message}");
            }
        }

        let planned = report
            .records
            .iter()
            .filter(|record| record.command.is_some())
            .count();
        let executed = report
            .records
            .iter()
            .filter(|record| record.status.counts_as_executed() && record.command.is_some())
            .count();
        let failed = report
            .records
            .iter()
            .filter(|record| record.status.is_failure())
            .count();
        if report
            .records
            .iter()
            .any(|record| matches!(record.status, crate::domain::OperationStatus::DryRun))
        {
            println!("\nDry run completed");
        }
        println!("{planned} operation(s) planned");
        println!("{executed} command(s) executed");
        if failed > 0 {
            println!("{failed} backend operation(s) failed");
        }
    }

    pub fn maintenance_summary(&self, report: &MultiOperationReport, verbose: bool, dry_run: bool) {
        if self.json {
            #[derive(Serialize)]
            struct MaintenanceEnvelope<'a> {
                schema_version: u8,
                command: &'a str,
                complete: bool,
                requires_confirmation: bool,
                confirmation_bypassed: bool,
                targets: Vec<&'a str>,
                plans: Vec<&'a crate::domain::BackendOperationRecord>,
                results: &'a [crate::domain::BackendOperationRecord],
                skips: Vec<&'a crate::domain::BackendOperationRecord>,
                issues: &'a [BackendIssue],
            }
            let targets = report
                .records
                .iter()
                .map(|record| record.backend_name.as_str())
                .collect::<Vec<_>>();
            let plans = report
                .records
                .iter()
                .filter(|record| record.command.is_some())
                .collect::<Vec<_>>();
            let skips = report
                .records
                .iter()
                .filter(|record| {
                    matches!(
                        record.status,
                        crate::domain::OperationStatus::Skipped
                            | crate::domain::OperationStatus::NotApplicable
                            | crate::domain::OperationStatus::Unavailable
                            | crate::domain::OperationStatus::Protected
                    )
                })
                .collect::<Vec<_>>();
            self.render_json(&MaintenanceEnvelope {
                schema_version: 1,
                command: &report.operation,
                complete: !report.has_failures(),
                requires_confirmation: !dry_run,
                confirmation_bypassed: false,
                targets,
                plans,
                results: &report.records,
                skips,
                issues: &[],
            });
            return;
        }

        println!(
            "\n{}",
            self.heading(&format!("{} Summary", title_case(&report.operation)))
        );
        let hidden_unavailable = report
            .records
            .iter()
            .filter(|record| record.status.is_optional_unavailable())
            .count();
        for record in &report.records {
            if record.status.is_optional_unavailable() && !verbose {
                continue;
            }
            let marker = self.status_marker(&record.status);
            let label = record.status.label();
            if let Some(message) = &record.message {
                if matches!(record.status, crate::domain::OperationStatus::DryRun) {
                    println!("{marker} {:<10} {label}", record.backend_name);
                    if verbose {
                        println!("  {message}");
                    }
                } else {
                    println!(
                        "{marker} {:<15} {label} · {}",
                        record.backend_name,
                        strip_status_prefix(message)
                    );
                }
            } else {
                println!("{marker} {:<15} {label}", record.backend_name);
            }
            if verbose {
                if let Some(action) = &record.action {
                    println!("  Action: {action}");
                }
                if let Some(command) = &record.command {
                    println!("  Command: {command}");
                }
            }
        }
        if hidden_unavailable > 0 && !verbose {
            println!("\nOptional unavailable targets hidden; use --verbose to show details.");
        }

        let completed = count_status(report, |status| {
            matches!(
                status,
                crate::domain::OperationStatus::Completed | crate::domain::OperationStatus::Success
            )
        });
        let updated = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::Updated)
        });
        let up_to_date = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::UpToDate)
        });
        let deferred = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::Deferred)
        });
        let not_applicable = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::NotApplicable)
        });
        let unavailable = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::Unavailable)
        });
        let protected = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::Protected)
        });
        let busy = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::Busy)
        });
        let cancelled = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::Cancelled)
        });
        let failed = count_status(report, |status| {
            matches!(status, crate::domain::OperationStatus::Failed)
        });
        let planned = report
            .records
            .iter()
            .filter(|record| record.command.is_some())
            .count();
        if dry_run {
            println!("\nDry run completed");
            println!("{planned} operation(s) planned");
            println!("0 commands executed");
        } else {
            println!("\n{completed} completed");
            println!("{updated} updated");
            println!("{up_to_date} up to date");
            println!("{deferred} deferred");
            println!("{not_applicable} not applicable");
            println!("{protected} protected");
            println!("{busy} busy");
            println!("{cancelled} cancelled");
            println!("{failed} failed");
            if verbose {
                println!("{unavailable} unavailable");
            }
        }
    }

    pub fn warn(&self, message: &str) {
        if !self.json {
            eprintln!("{} {message}", self.warning("⚠"));
        }
    }

    pub fn success_message(&self, message: &str) {
        if !self.json {
            println!("{} {message}", self.success("✔"));
        }
    }

    pub fn info_message(&self, message: &str) {
        if !self.json {
            println!("{} {message}", self.info_style("ℹ"));
        }
    }

    pub fn plain_message(&self, message: &str) {
        if !self.json {
            println!("{message}");
        }
    }

    pub fn execution_started(
        &self,
        index: usize,
        total: usize,
        plan: &ExecutionPlan,
        context: &RuntimePrivilegeContext,
    ) {
        if self.json {
            return;
        }
        eprintln!(
            "{} [{index}/{total}] {} {} started",
            self.info_style("●"),
            plan.backend_name,
            plan.operation.as_str()
        );
        eprintln!("  Action: {}", plan.action);
        eprintln!(
            "  Command: {}",
            render_execution_plan_with_context(plan, context)
        );
    }

    pub fn execution_finished(
        &self,
        index: usize,
        total: usize,
        backend_name: &str,
        status: &crate::domain::OperationStatus,
        message: Option<&str>,
        elapsed: std::time::Duration,
    ) {
        if self.json {
            return;
        }
        let marker = self.status_marker(status);
        eprintln!(
            "{marker} [{index}/{total}] {backend_name} finished in {}",
            format_duration(elapsed)
        );
        eprintln!(
            "  Result: {}{}",
            status.label(),
            message
                .map(strip_status_prefix)
                .map(|value| format!(" · {value}"))
                .unwrap_or_default()
        );
    }

    fn heading(&self, value: &str) -> String {
        self.style(value, "1;36")
    }
    fn subheading(&self, value: &str) -> String {
        self.style(value, "1;34")
    }
    fn bold(&self, value: &str) -> String {
        self.style(value, "1")
    }
    fn success(&self, value: &str) -> String {
        self.style(value, "32")
    }
    fn warning(&self, value: &str) -> String {
        self.style(value, "33")
    }
    fn error(&self, value: &str) -> String {
        self.style(value, "31")
    }
    fn info_style(&self, value: &str) -> String {
        self.style(value, "36")
    }
    fn muted(&self, value: &str) -> String {
        self.style(value, "2")
    }

    fn status_marker(&self, status: &crate::domain::OperationStatus) -> String {
        match status {
            crate::domain::OperationStatus::Updated
            | crate::domain::OperationStatus::UpToDate
            | crate::domain::OperationStatus::Completed
            | crate::domain::OperationStatus::AlreadyInstalled
            | crate::domain::OperationStatus::Success => self.success("✔"),
            crate::domain::OperationStatus::Failed => self.error("✖"),
            crate::domain::OperationStatus::Protected
            | crate::domain::OperationStatus::Busy
            | crate::domain::OperationStatus::Deferred => self.warning("⚠"),
            crate::domain::OperationStatus::DryRun
            | crate::domain::OperationStatus::Available
            | crate::domain::OperationStatus::Selected => self.info_style("ℹ"),
            crate::domain::OperationStatus::NotApplicable
            | crate::domain::OperationStatus::NotSelected
            | crate::domain::OperationStatus::Unavailable
            | crate::domain::OperationStatus::Cancelled
            | crate::domain::OperationStatus::Skipped => self.muted("○"),
        }
    }

    fn style(&self, value: &str, code: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{value}\x1b[0m")
        } else {
            value.to_owned()
        }
    }
}

fn strip_status_prefix(message: &str) -> &str {
    message.strip_prefix("Skipped: ").unwrap_or(message)
}

fn count_status(
    report: &MultiOperationReport,
    predicate: impl Fn(&crate::domain::OperationStatus) -> bool,
) -> usize {
    report
        .records
        .iter()
        .filter(|record| predicate(&record.status))
        .count()
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        return format!("{seconds}s");
    }
    format!("{}m {}s", seconds / 60, seconds % 60)
}

fn extra_value<'a>(info: &'a PackageInfo, key: &str) -> Option<&'a str> {
    info.extra
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

fn wrap_text(value: &str, width: usize) -> String {
    let mut output = String::new();
    let mut line_len = 0usize;
    for word in value.split_whitespace() {
        let word_len = word.len();
        if line_len > 0 && line_len + 1 + word_len > width {
            output.push('\n');
            output.push_str(word);
            line_len = word_len;
        } else {
            if line_len > 0 {
                output.push(' ');
                line_len += 1;
            }
            output.push_str(word);
            line_len += word_len;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::Renderer;

    #[test]
    fn status_styles_use_expected_colors_when_enabled() {
        let renderer = Renderer::with_color_for_test(true, false);

        assert_eq!(renderer.success("✔"), "\x1b[32m✔\x1b[0m");
        assert_eq!(renderer.error("✖"), "\x1b[31m✖\x1b[0m");
        assert_eq!(renderer.warning("⚠"), "\x1b[33m⚠\x1b[0m");
        assert_eq!(renderer.info_style("ℹ"), "\x1b[36mℹ\x1b[0m");
    }

    #[test]
    fn color_can_be_disabled_without_losing_icons() {
        let renderer = Renderer::with_color_for_test(false, false);

        assert_eq!(renderer.success("✔"), "✔");
        assert_eq!(renderer.error("✖"), "✖");
        assert_eq!(renderer.warning("⚠"), "⚠");
        assert_eq!(renderer.info_style("ℹ"), "ℹ");
    }

    #[test]
    fn json_renderer_test_fixture_does_not_color() {
        let renderer = Renderer::with_color_for_test(false, true);

        assert_eq!(renderer.success("✔"), "✔");
        assert!(renderer.json());
    }
}
