use crate::{
    bootstrap,
    cli::Spinner,
    discovery::{BackendDetection, DetectionState},
    domain::{
        AllpError, BackendIssue, Capability, MatchKind, PackageCandidate, SearchBackendState,
        SearchBackendSummary, SearchReport, SearchScope,
    },
    identity::resolver,
    operations::OperationContext,
};
use std::{
    collections::{BTreeMap, HashSet},
    sync::mpsc,
    thread,
};

const DEFAULT_LIMIT: usize = 25;
const RELATED_LIMIT_PER_BACKEND: usize = 5;
const MAX_SEARCH_CONCURRENCY: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct SearchPolicy {
    pub exact_only: bool,
    pub limit: usize,
    pub include_fuzzy: bool,
    pub required_capability: Option<Capability>,
    pub scope: SearchScope,
}

impl Default for SearchPolicy {
    fn default() -> Self {
        Self {
            exact_only: false,
            limit: DEFAULT_LIMIT,
            include_fuzzy: false,
            required_capability: None,
            scope: SearchScope::AllSources,
        }
    }
}

pub fn run(
    context: &OperationContext<'_>,
    query: &str,
    exact_only: bool,
    limit: usize,
    include_fuzzy: bool,
) -> crate::domain::AllpResult<SearchReport> {
    if limit == 0 {
        return Err(AllpError::InvalidInput(
            "--limit must be greater than zero".to_owned(),
        ));
    }

    let report = gather_with_policy(
        context,
        query,
        SearchPolicy {
            exact_only,
            limit,
            include_fuzzy,
            required_capability: None,
            scope: context.search_scope.unwrap_or(SearchScope::AllSources),
        },
    )?;
    context.renderer.search(&report);
    Ok(report)
}

pub fn gather(
    context: &OperationContext<'_>,
    query: &str,
) -> crate::domain::AllpResult<SearchReport> {
    gather_with_policy(context, query, SearchPolicy::default())
}

pub fn gather_with_policy(
    context: &OperationContext<'_>,
    query: &str,
    policy: SearchPolicy,
) -> crate::domain::AllpResult<SearchReport> {
    gather_with_policy_excluding(context, query, policy, &HashSet::new())
}

pub fn gather_with_policy_excluding(
    context: &OperationContext<'_>,
    query: &str,
    policy: SearchPolicy,
    excluded_backends: &HashSet<String>,
) -> crate::domain::AllpResult<SearchReport> {
    let mut candidates = bootstrap::candidates_for_query(
        query,
        policy.required_capability,
        policy.scope,
        context.backend_filter,
    );
    candidates.retain(|candidate| !backend_is_excluded(&candidate.backend_id, excluded_backends));
    let eligible = match context.eligible_backends() {
        Ok(backends) => backends,
        Err(_) if !candidates.is_empty() => Vec::new(),
        Err(error) => return Err(error),
    };
    let eligible: Vec<_> = eligible
        .into_iter()
        .filter(|runtime| {
            !backend_is_excluded(runtime.backend.id(), excluded_backends)
                && runtime.backend.has_capability(Capability::Search)
                && policy
                    .required_capability
                    .map(|capability| runtime.backend.has_capability(capability))
                    .unwrap_or(true)
                && policy
                    .scope
                    .matches_backend_domains(runtime.backend.package_domains())
        })
        .cloned()
        .collect();

    if eligible.is_empty() && candidates.is_empty() && excluded_backends.is_empty() {
        return Err(AllpError::UnsupportedOperation {
            backend: context
                .backend_filter
                .unwrap_or("detected package managers")
                .to_owned(),
            operation: "search".to_owned(),
        });
    }

    let mut issues = Vec::new();
    let mut backend_summaries = initial_backend_summaries(context, policy, excluded_backends);

    if !eligible.is_empty() {
        let spinner = Spinner::start(
            format!("Searching {} package manager(s)", eligible.len()),
            context.renderer.spinner_enabled(),
        );

        for chunk in eligible.chunks(MAX_SEARCH_CONCURRENCY) {
            let (sender, receiver) = mpsc::channel();
            thread::scope(|scope| {
                for runtime in chunk.iter().cloned() {
                    let sender = sender.clone();
                    let query = query.to_owned();
                    let runner = context.runner;
                    scope.spawn(move || {
                        let backend_id = runtime.backend.id().to_owned();
                        let backend_name = runtime.backend.display_name().to_owned();
                        let result = runtime.backend.search(&runtime.commands, runner, &query);
                        let _ = sender.send((backend_id, backend_name, result));
                    });
                }
                drop(sender);
            });

            for (backend_id, backend_name, result) in receiver {
                match result {
                    Ok(mut found) => {
                        update_backend_summary(
                            &mut backend_summaries,
                            &backend_id,
                            if found.is_empty() {
                                SearchBackendState::NoMatches
                            } else {
                                SearchBackendState::ParsedResults
                            },
                            found.len(),
                            None,
                        );
                        candidates.append(&mut found);
                    }
                    Err(error) => {
                        let (state, message) = classify_search_error(&error);
                        update_backend_summary(
                            &mut backend_summaries,
                            &backend_id,
                            state,
                            0,
                            Some(message.clone()),
                        );
                        if state == SearchBackendState::SearchFailed {
                            issues.push(BackendIssue {
                                backend_id,
                                backend_name,
                                message,
                            });
                        }
                    }
                }
            }
        }

        spinner.stop();
    }

    for candidate in &mut candidates {
        if !candidate.identity.is_official() {
            candidate.match_kind = classify_candidate(candidate, query);
        }
    }
    resolver::annotate_candidates(query, &mut candidates);
    candidates.retain(|candidate| policy.scope.matches_candidate(candidate));
    sort_candidates_for_query(&mut candidates, query);
    let candidates = apply_visibility(candidates, policy);

    Ok(SearchReport {
        query: query.to_owned(),
        complete: issues.is_empty(),
        candidates,
        issues,
        backend_summaries,
    })
}

fn initial_backend_summaries(
    context: &OperationContext<'_>,
    policy: SearchPolicy,
    excluded_backends: &HashSet<String>,
) -> Vec<SearchBackendSummary> {
    context
        .discovery
        .entries
        .iter()
        .filter(|entry| detection_entry_matches_policy(entry, context.backend_filter, policy))
        .map(|entry| {
            let (state, message) = if backend_is_excluded(&entry.backend_id, excluded_backends) {
                (
                    SearchBackendState::Skipped,
                    Some("excluded after failed exact resolution".to_owned()),
                )
            } else {
                match entry.state {
                    DetectionState::Ready => (SearchBackendState::Available, None),
                    DetectionState::NotFound => (
                        SearchBackendState::Unavailable,
                        Some("executable not found".to_owned()),
                    ),
                    _ => (
                        SearchBackendState::Unavailable,
                        Some(
                            entry
                                .message
                                .clone()
                                .unwrap_or_else(|| entry.state.label().to_ascii_lowercase()),
                        ),
                    ),
                }
            };
            SearchBackendSummary {
                backend_id: entry.backend_id.clone(),
                backend_name: entry.backend_name.clone(),
                state,
                result_count: 0,
                message,
            }
        })
        .collect()
}

fn backend_is_excluded(backend_id: &str, excluded_backends: &HashSet<String>) -> bool {
    excluded_backends
        .iter()
        .any(|excluded| excluded.eq_ignore_ascii_case(backend_id))
}

fn detection_entry_matches_policy(
    entry: &BackendDetection,
    backend_filter: Option<&str>,
    policy: SearchPolicy,
) -> bool {
    if let Some(filter) = backend_filter {
        let normalized = filter.to_ascii_lowercase();
        let matches_filter = entry.backend_id.eq_ignore_ascii_case(&normalized)
            || entry.backend_name.eq_ignore_ascii_case(filter)
            || entry
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(filter));
        if !matches_filter {
            return false;
        }
    }
    entry.capabilities.contains(&Capability::Search)
        && policy
            .required_capability
            .map(|capability| entry.capabilities.contains(&capability))
            .unwrap_or(true)
        && policy.scope.matches_backend_domains(&entry.package_domains)
}

fn update_backend_summary(
    summaries: &mut [SearchBackendSummary],
    backend_id: &str,
    state: SearchBackendState,
    result_count: usize,
    message: Option<String>,
) {
    if let Some(summary) = summaries
        .iter_mut()
        .find(|summary| summary.backend_id == backend_id)
    {
        summary.state = state;
        summary.result_count = result_count;
        summary.message = message;
    }
}

fn classify_search_error(error: &AllpError) -> (SearchBackendState, String) {
    match error {
        AllpError::NoConfiguredRemotes { .. } => (
            SearchBackendState::NoConfiguredRemotes,
            "no configured remotes".to_owned(),
        ),
        _ => (
            SearchBackendState::SearchFailed,
            normalized_search_error(error),
        ),
    }
}

fn normalized_search_error(error: &AllpError) -> String {
    match error {
        AllpError::CommandFailed { code, stderr, .. } => {
            let reason = stderr.trim();
            if reason.is_empty() {
                code.map(|code| format!("native command exited with {code}"))
                    .unwrap_or_else(|| "native command failed".to_owned())
            } else {
                reason.lines().next().unwrap_or(reason).to_owned()
            }
        }
        _ => error
            .to_string()
            .lines()
            .next()
            .unwrap_or("search failed")
            .to_owned(),
    }
}

pub fn classify_candidate(candidate: &PackageCandidate, query: &str) -> MatchKind {
    classify_match(
        &candidate.package_id,
        &candidate.display_name,
        candidate.description.as_deref(),
        query,
    )
}

pub fn classify_match(
    package_id: &str,
    display_name: &str,
    _description: Option<&str>,
    query: &str,
) -> MatchKind {
    let query = query.trim();
    if query.is_empty() {
        return MatchKind::Fuzzy;
    }

    let package_norm = normalize(package_id);
    let display_norm = normalize(display_name);
    let query_norm = normalize(query);

    if package_norm == query_norm || display_norm == query_norm {
        return MatchKind::Exact;
    }

    let package_has_penalty = has_development_library_penalty(package_id);
    let display_is_package_id = display_norm == package_norm;

    if ((!display_is_package_id || !package_has_penalty)
        && display_has_token(&display_norm, &query_norm))
        || is_strong_package_name(package_id, &package_norm, &query_norm)
    {
        return MatchKind::Related;
    }

    MatchKind::Fuzzy
}

pub fn sort_candidates(candidates: &mut [PackageCandidate]) {
    sort_candidates_for_query(candidates, "");
}

pub fn sort_candidates_for_query(candidates: &mut [PackageCandidate], query: &str) {
    candidates.sort_by(|left, right| {
        left.identity
            .rank()
            .cmp(&right.identity.rank())
            .then(left.match_kind.cmp(&right.match_kind))
            .then(rank_score(left, query).cmp(&rank_score(right, query)))
            .then(left.category.cmp(&right.category))
            .then_with(|| {
                left.backend_name
                    .to_ascii_lowercase()
                    .cmp(&right.backend_name.to_ascii_lowercase())
            })
            .then_with(|| {
                left.package_id
                    .to_ascii_lowercase()
                    .cmp(&right.package_id.to_ascii_lowercase())
            })
    });
}

pub fn apply_visibility(
    candidates: Vec<PackageCandidate>,
    policy: SearchPolicy,
) -> Vec<PackageCandidate> {
    if policy.exact_only {
        return candidates
            .into_iter()
            .filter(|candidate| candidate.match_kind == MatchKind::Exact)
            .collect();
    }

    let mut exact = Vec::new();
    let mut related_by_backend: BTreeMap<String, Vec<PackageCandidate>> = BTreeMap::new();
    let mut fuzzy_by_backend: BTreeMap<String, Vec<PackageCandidate>> = BTreeMap::new();

    for candidate in candidates {
        match candidate.match_kind {
            MatchKind::Exact => exact.push(candidate),
            MatchKind::Related => {
                let bucket = related_by_backend
                    .entry(candidate.backend_id.clone())
                    .or_default();
                if bucket.len() < RELATED_LIMIT_PER_BACKEND {
                    bucket.push(candidate);
                }
            }
            MatchKind::Fuzzy => {
                if policy.include_fuzzy {
                    fuzzy_by_backend
                        .entry(candidate.backend_id.clone())
                        .or_default()
                        .push(candidate);
                }
            }
        }
    }

    let mut visible = exact;
    round_robin_extend(&mut visible, related_by_backend, policy.limit);
    if policy.include_fuzzy {
        round_robin_extend(&mut visible, fuzzy_by_backend, policy.limit);
    }
    visible
}

fn round_robin_extend(
    visible: &mut Vec<PackageCandidate>,
    mut buckets: BTreeMap<String, Vec<PackageCandidate>>,
    limit: usize,
) {
    let backend_ids: Vec<_> = buckets.keys().cloned().collect();
    let mut offsets: BTreeMap<String, usize> = BTreeMap::new();

    loop {
        let mut added = false;
        for backend_id in &backend_ids {
            if visible.len() >= limit {
                return;
            }
            let offset = offsets.entry(backend_id.clone()).or_default();
            let Some(bucket) = buckets.get_mut(backend_id) else {
                continue;
            };
            if *offset < bucket.len() {
                visible.push(bucket[*offset].clone());
                *offset += 1;
                added = true;
            }
        }
        if !added {
            break;
        }
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn display_has_token(display_norm: &str, query_norm: &str) -> bool {
    display_norm
        .split('-')
        .any(|token| token == query_norm || token.starts_with(&format!("{query_norm}-")))
}

fn is_strong_package_name(original: &str, package_norm: &str, query_norm: &str) -> bool {
    if has_development_library_penalty(original) {
        return false;
    }

    package_norm == query_norm
        || package_norm
            .strip_prefix(query_norm)
            .is_some_and(|remaining| remaining.starts_with('-'))
}

fn rank_score(candidate: &PackageCandidate, query: &str) -> usize {
    let package_id = candidate.package_id.to_ascii_lowercase();
    let display_name = candidate.display_name.to_ascii_lowercase();
    let query = normalize(query);
    if candidate.match_kind == MatchKind::Exact {
        return 0;
    }

    let mut score = 100usize;
    if !query.is_empty() {
        if let Some(remaining) = package_id.strip_prefix(&format!("{query}-")) {
            score = score.min(if known_product_suffix(remaining) {
                20
            } else {
                30
            });
        } else if package_id.starts_with(&query) {
            score = score.min(35);
        } else if display_name.starts_with(&query) {
            score = score.min(40);
        } else if let Some(position) = package_id.split('-').position(|token| token == query) {
            score = score.min(60 + position * 10);
        }
    }
    if has_development_library_penalty(&package_id) {
        score += 100;
    }
    if package_id.matches('-').count() > 3 {
        score += 30;
    }
    if is_transitional(&candidate.description) {
        score += 50;
    }
    score += package_id.len().saturating_sub(16);
    score
}

fn known_product_suffix(value: &str) -> bool {
    let suffix = value.split('-').next().unwrap_or(value);
    matches!(
        suffix,
        "scm" | "cli" | "gui" | "lfs" | "core" | "desktop" | "client" | "server"
    )
}

fn is_transitional(description: &Option<String>) -> bool {
    description
        .as_deref()
        .map(|value| value.to_ascii_lowercase().contains("transitional package"))
        .unwrap_or(false)
}

fn has_development_library_penalty(package_id: &str) -> bool {
    let value = package_id.to_ascii_lowercase();
    value.starts_with("golang-")
        || value.starts_with("lib")
        || value.ends_with("-dev")
        || value.ends_with("-devel")
        || value.ends_with("-doc")
        || value.ends_with("-dbg")
        || value.ends_with("-perl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{BackendCategory, PackageDomain};

    fn candidate(
        backend_id: &str,
        category: BackendCategory,
        package_id: &str,
    ) -> PackageCandidate {
        PackageCandidate {
            backend_id: backend_id.to_owned(),
            backend_name: backend_id.to_ascii_uppercase(),
            category,
            domain: match category {
                BackendCategory::System => PackageDomain::System,
                BackendCategory::Universal => PackageDomain::Universal,
                BackendCategory::Development => PackageDomain::Python,
            },
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: None,
            description: Some("test package".to_owned()),
            source: None,
            installers: Vec::new(),
            artifact_kind: "test".to_owned(),
            scope: None,
            match_kind: MatchKind::Fuzzy,
            identity: PackageCandidate::infer_identity(
                MatchKind::Fuzzy,
                match category {
                    BackendCategory::System => PackageDomain::System,
                    BackendCategory::Universal => PackageDomain::Universal,
                    BackendCategory::Development => PackageDomain::Python,
                },
                "test",
            ),
            metadata: Default::default(),
        }
    }

    #[test]
    fn classifies_git_matches_into_three_levels() {
        assert_eq!(classify_match("git", "git", None, "git"), MatchKind::Exact);
        assert_eq!(
            classify_match("git-scm", "git-scm", None, "git"),
            MatchKind::Related
        );
        assert_eq!(
            classify_match("git-cola", "git-cola", None, "git"),
            MatchKind::Related
        );
        assert_eq!(
            classify_match(
                "golang-github-git-lfs-dev",
                "golang-github-git-lfs-dev",
                None,
                "git"
            ),
            MatchKind::Fuzzy
        );
        assert_eq!(
            classify_match(
                "libtest-requires-git-perl",
                "libtest-requires-git-perl",
                None,
                "git"
            ),
            MatchKind::Fuzzy
        );
    }

    #[test]
    fn classifies_ambiguous_code_names_without_claiming_equivalence() {
        assert_eq!(
            classify_match("code", "code", None, "code"),
            MatchKind::Exact
        );
        assert_eq!(
            classify_match("com.visualstudio.code", "Visual Studio Code", None, "code"),
            MatchKind::Related
        );
    }

    #[test]
    fn applies_per_backend_related_limits_and_total_limits() {
        let mut candidates = vec![candidate("first", BackendCategory::System, "git")];
        for index in 0..10 {
            let mut item = candidate(
                "second",
                BackendCategory::Universal,
                &format!("git-tool-{index}"),
            );
            item.match_kind = MatchKind::Related;
            candidates.push(item);
        }
        candidates[0].match_kind = MatchKind::Exact;

        let visible = apply_visibility(
            candidates,
            SearchPolicy {
                exact_only: false,
                limit: 25,
                include_fuzzy: false,
                required_capability: None,
                scope: SearchScope::AllSources,
            },
        );

        assert_eq!(visible.len(), 6);
        assert_eq!(
            visible
                .iter()
                .filter(|candidate| candidate.match_kind == MatchKind::Related)
                .count(),
            RELATED_LIMIT_PER_BACKEND
        );
    }

    #[test]
    fn sorts_deterministically_by_match_category_backend_and_package_id() {
        let mut candidates = vec![
            candidate("second", BackendCategory::Universal, "git-cola"),
            candidate("first", BackendCategory::System, "git"),
            candidate("first", BackendCategory::System, "git-cola"),
        ];
        candidates[0].match_kind = MatchKind::Related;
        candidates[1].match_kind = MatchKind::Exact;
        candidates[2].match_kind = MatchKind::Related;

        sort_candidates(&mut candidates);

        let ids: Vec<_> = candidates
            .iter()
            .map(|candidate| candidate.package_id.as_str())
            .collect();
        assert_eq!(ids, vec!["git", "git-cola", "git-cola"]);
        assert_eq!(candidates[1].backend_id, "first");
        assert_eq!(candidates[2].backend_id, "second");
    }

    #[test]
    fn exact_policy_hides_non_exact_results() {
        let mut exact = candidate("first", BackendCategory::System, "git");
        exact.match_kind = MatchKind::Exact;
        let mut related = candidate("second", BackendCategory::Universal, "git-scm");
        related.match_kind = MatchKind::Related;

        let visible = apply_visibility(
            vec![exact, related],
            SearchPolicy {
                exact_only: true,
                limit: 25,
                include_fuzzy: true,
                required_capability: None,
                scope: SearchScope::AllSources,
            },
        );

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].package_id, "git");
    }
}
