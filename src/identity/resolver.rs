use crate::{
    domain::{
        CandidateGroup, DistributionRelationship, IdentityConfidence, IdentityMetadata, MatchKind,
        NameMatchKind, PackageCandidate, PackageDomain,
    },
    identity::catalog::{self, CanonicalIdentity, HOMEBREW_ID},
};

pub fn resolve_query(query: &str) -> Option<&'static CanonicalIdentity> {
    let normalized = normalize_identity_text(query);
    if normalized.is_empty() {
        return None;
    }
    catalog::all().iter().find(|identity| {
        normalize_identity_text(identity.display_name) == normalized
            || identity
                .aliases
                .iter()
                .any(|alias| normalize_identity_text(alias) == normalized)
    })
}

pub fn annotate_candidates(query: &str, candidates: &mut [PackageCandidate]) {
    let identity = resolve_query(query);
    for candidate in candidates {
        if candidate.identity.is_official() {
            continue;
        }
        candidate.refresh_inferred_identity();
        let Some(identity) = identity else {
            continue;
        };
        annotate_candidate(identity, candidate);
    }
}

/// Build presentation-neutral identity groups after search ordering is final.
/// Only explicit canonical evidence may combine candidates. Name similarity by
/// itself never merges results.
pub fn group_candidates(candidates: &[PackageCandidate]) -> Vec<CandidateGroup> {
    let mut groups: Vec<CandidateGroup> = Vec::new();

    for (index, candidate) in candidates.iter().enumerate() {
        let selection_number = index + 1;
        let may_cluster = matches!(
            candidate.identity.confidence,
            IdentityConfidence::Official
                | IdentityConfidence::Verified
                | IdentityConfidence::Probable
        );
        let canonical_id = candidate.identity.canonical_id.as_deref();

        if may_cluster {
            if let Some(canonical_id) = canonical_id {
                if let Some(group) = groups.iter_mut().find(|group| {
                    group.canonical_id.as_deref() == Some(canonical_id)
                        && matches!(
                            group.confidence,
                            IdentityConfidence::Official
                                | IdentityConfidence::Verified
                                | IdentityConfidence::Probable
                        )
                }) {
                    group.selection_numbers.push(selection_number);
                    // A group is only as certain as its least-certain member.
                    group.confidence = group.confidence.max(candidate.identity.confidence);
                    continue;
                }
            }
        }

        groups.push(CandidateGroup {
            group_id: canonical_id
                .filter(|_| may_cluster)
                .map(|id| format!("canonical:{id}"))
                .unwrap_or_else(|| format!("candidate:{selection_number}")),
            canonical_id: canonical_id.map(str::to_owned),
            canonical_name: candidate.identity.canonical_name.clone(),
            confidence: candidate.identity.confidence,
            selection_numbers: vec![selection_number],
        });
    }
    groups
}

pub fn is_known_bootstrap_query(query: &str) -> bool {
    resolve_query(query).is_some_and(|identity| identity.id == HOMEBREW_ID)
}

pub fn normalize_identity_text(value: &str) -> String {
    value
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else {
                None
            }
        })
        .collect()
}

fn annotate_candidate(identity: &CanonicalIdentity, candidate: &mut PackageCandidate) {
    let name_match = identity_name_match(identity, candidate);
    if identity.id == HOMEBREW_ID
        && candidate.domain == PackageDomain::Node
        && normalize_identity_text(&candidate.package_id) == "homebrew"
    {
        candidate.identity = IdentityMetadata {
            name_match,
            confidence: IdentityConfidence::Conflicting,
            distribution: DistributionRelationship::NameMatchOnly,
            software_type: candidate.identity.software_type,
            canonical_id: Some(identity.id.to_owned()),
            canonical_name: Some(identity.display_name.to_owned()),
            official_source: false,
            confidence_source: Some("built-in conflict rule: npm/homebrew".to_owned()),
            warning: Some(
                "The npm package named \"homebrew\" is not the Homebrew package manager."
                    .to_owned(),
            ),
        };
        candidate.match_kind = MatchKind::Exact;
        return;
    }

    if let Some(mapping) = identity.verified_packages.iter().find(|mapping| {
        candidate
            .backend_id
            .eq_ignore_ascii_case(mapping.backend_id)
            && package_id_matches_mapping(
                &candidate.backend_id,
                &candidate.package_id,
                mapping.package_id,
            )
    }) {
        candidate.identity = IdentityMetadata {
            name_match,
            confidence: IdentityConfidence::Verified,
            distribution: DistributionRelationship::VerifiedThirdPartyPackage,
            software_type: identity.software_type,
            canonical_id: Some(identity.id.to_owned()),
            canonical_name: Some(identity.display_name.to_owned()),
            official_source: false,
            confidence_source: Some(mapping.evidence.to_owned()),
            warning: None,
        };
        return;
    }

    if candidate.domain == PackageDomain::Homebrew {
        candidate.identity = IdentityMetadata {
            name_match,
            confidence: IdentityConfidence::Verified,
            distribution: DistributionRelationship::VerifiedThirdPartyPackage,
            software_type: candidate.identity.software_type,
            canonical_id: Some(identity.id.to_owned()),
            canonical_name: Some(identity.display_name.to_owned()),
            official_source: false,
            confidence_source: Some("built-in Homebrew identity mapping".to_owned()),
            warning: None,
        };
        return;
    }

    if matches!(
        name_match,
        NameMatchKind::Exact | NameMatchKind::NormalizedExact | NameMatchKind::Alias
    ) {
        candidate.identity = IdentityMetadata {
            name_match,
            confidence: IdentityConfidence::Unverified,
            distribution: DistributionRelationship::NameMatchOnly,
            software_type: candidate.identity.software_type,
            canonical_id: Some(identity.id.to_owned()),
            canonical_name: Some(identity.display_name.to_owned()),
            official_source: false,
            confidence_source: Some("package-name match only".to_owned()),
            warning: Some(format!(
                "Exact package-name match only; this has not been verified as {}.",
                identity.display_name
            )),
        };
    }
}

fn package_id_matches_mapping(backend_id: &str, candidate_id: &str, mapped_id: &str) -> bool {
    if candidate_id.eq_ignore_ascii_case(mapped_id) {
        return true;
    }
    if !backend_id.eq_ignore_ascii_case("dnf") {
        return false;
    }
    candidate_id
        .strip_prefix(mapped_id)
        .and_then(|suffix| suffix.strip_prefix('.'))
        .is_some_and(|architecture| {
            matches!(
                architecture,
                "x86_64" | "aarch64" | "noarch" | "i686" | "ppc64le" | "s390x"
            )
        })
}

fn identity_name_match(
    identity: &CanonicalIdentity,
    candidate: &PackageCandidate,
) -> NameMatchKind {
    if candidate.package_id == identity.display_name
        || candidate.display_name == identity.display_name
    {
        return NameMatchKind::Exact;
    }

    let package = normalize_identity_text(&candidate.package_id);
    let display = normalize_identity_text(&candidate.display_name);
    let canonical = normalize_identity_text(identity.display_name);
    if package == canonical || display == canonical {
        return NameMatchKind::NormalizedExact;
    }
    if identity.aliases.iter().any(|alias| {
        let alias = normalize_identity_text(alias);
        package == alias || display == alias
    }) {
        return NameMatchKind::Alias;
    }
    if package.starts_with(&canonical) || display.starts_with(&canonical) {
        return NameMatchKind::Prefix;
    }
    NameMatchKind::Fuzzy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{BackendCategory, MatchKind, SoftwareType};

    #[test]
    fn resolves_homebrew_aliases() {
        let identity = resolve_query("Home Brew").expect("homebrew alias should resolve");

        assert_eq!(identity.id, HOMEBREW_ID);
        assert_eq!(identity.software_type, SoftwareType::PackageManager);
    }

    #[test]
    fn npm_homebrew_is_marked_conflicting() {
        let mut candidates = vec![PackageCandidate {
            backend_id: "node".to_owned(),
            backend_name: "Node.js".to_owned(),
            category: BackendCategory::Development,
            domain: PackageDomain::Node,
            package_id: "homebrew".to_owned(),
            display_name: "homebrew".to_owned(),
            version: None,
            description: None,
            source: Some("npm registry".to_owned()),
            installers: vec!["npm".to_owned()],
            artifact_kind: "Node package".to_owned(),
            scope: Some("global user tool".to_owned()),
            match_kind: MatchKind::Exact,
            identity: PackageCandidate::infer_identity(
                MatchKind::Exact,
                PackageDomain::Node,
                "Node package",
            ),
            metadata: Default::default(),
        }];

        annotate_candidates("Homebrew", &mut candidates);

        assert!(candidates[0].identity.is_conflicting());
        assert_eq!(
            candidates[0].identity.distribution,
            DistributionRelationship::NameMatchOnly
        );
    }

    #[test]
    fn verified_canonical_mappings_group_without_changing_selection_numbers() {
        let mut candidates = vec![
            test_candidate("apt", "firefox"),
            test_candidate("flatpak", "org.mozilla.firefox"),
        ];
        for candidate in &mut candidates {
            candidate.identity.confidence = IdentityConfidence::Verified;
            candidate.identity.canonical_id = Some("org.mozilla.firefox".to_owned());
            candidate.identity.canonical_name = Some("Firefox".to_owned());
        }

        let groups = group_candidates(&candidates);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].selection_numbers, vec![1, 2]);
    }

    #[test]
    fn same_name_without_verified_identity_never_groups() {
        let candidates = vec![
            test_candidate("apt", "code"),
            test_candidate("node", "code"),
        ];

        let groups = group_candidates(&candidates);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].selection_numbers, vec![1]);
        assert_eq!(groups[1].selection_numbers, vec![2]);
    }

    #[test]
    fn firefox_backend_ids_form_one_verified_group() {
        let mut candidates = vec![
            test_candidate("apt", "firefox"),
            test_candidate("flatpak", "org.mozilla.firefox"),
        ];
        annotate_candidates("Firefox", &mut candidates);

        assert!(candidates
            .iter()
            .all(|candidate| candidate.identity.confidence == IdentityConfidence::Verified));
        assert!(candidates
            .iter()
            .all(|candidate| candidate.identity.confidence_source.is_some()));
        let groups = group_candidates(&candidates);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].canonical_id.as_deref(), Some("firefox"));
        assert_eq!(groups[0].selection_numbers, vec![1, 2]);
    }

    #[test]
    fn verified_mapping_does_not_accept_arbitrary_identifier_suffixes() {
        assert!(package_id_matches_mapping(
            "dnf",
            "firefox.x86_64",
            "firefox"
        ));
        assert!(!package_id_matches_mapping(
            "flatpak",
            "org.mozilla.firefox.beta",
            "org.mozilla.firefox"
        ));
    }

    fn test_candidate(backend_id: &str, package_id: &str) -> PackageCandidate {
        PackageCandidate {
            backend_id: backend_id.to_owned(),
            backend_name: backend_id.to_owned(),
            category: BackendCategory::System,
            domain: PackageDomain::System,
            package_id: package_id.to_owned(),
            display_name: package_id.to_owned(),
            version: None,
            description: None,
            source: None,
            installers: Vec::new(),
            artifact_kind: "system package".to_owned(),
            scope: Some("system".to_owned()),
            match_kind: MatchKind::Exact,
            identity: PackageCandidate::infer_identity(
                MatchKind::Exact,
                PackageDomain::System,
                "system package",
            ),
            metadata: Default::default(),
        }
    }
}
