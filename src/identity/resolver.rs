use crate::{
    domain::{
        DistributionRelationship, IdentityConfidence, IdentityMetadata, MatchKind, NameMatchKind,
        PackageCandidate, PackageDomain,
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
            warning: Some(
                "The npm package named \"homebrew\" is not the Homebrew package manager."
                    .to_owned(),
            ),
        };
        candidate.match_kind = MatchKind::Exact;
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
            warning: Some(format!(
                "Exact package-name match only; this has not been verified as {}.",
                identity.display_name
            )),
        };
    }
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
}
