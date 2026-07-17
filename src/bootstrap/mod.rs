use crate::{
    domain::{AllpResult, Capability, ExecutionPlan, PackageCandidate, SearchScope},
    identity::resolver,
};

pub mod homebrew;
pub mod providers;

pub fn has_bootstrap_candidate(query: &str) -> bool {
    resolver::is_known_bootstrap_query(query)
}

pub fn candidates_for_query(
    query: &str,
    required_capability: Option<Capability>,
    scope: SearchScope,
    backend_filter: Option<&str>,
) -> Vec<PackageCandidate> {
    if required_capability.is_some_and(|capability| capability != Capability::Install) {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    if homebrew::matches_query(query)
        && homebrew::matches_scope(scope)
        && homebrew::matches_filter(backend_filter)
    {
        candidates.push(homebrew::candidate());
    }
    candidates
}

pub fn plan_install(candidate: &PackageCandidate) -> AllpResult<Option<ExecutionPlan>> {
    if homebrew::is_candidate(candidate) {
        return homebrew::plan_install().map(Some);
    }
    Ok(None)
}
