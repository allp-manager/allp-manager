use crate::domain::{
    BackendCategory, DistributionRelationship, IdentityMetadata, NameMatchKind, SoftwareType,
};
use serde::Serialize;
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Exact,
    Related,
    Fuzzy,
}

impl MatchKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Exact => "Exact",
            Self::Related => "Related",
            Self::Fuzzy => "Fuzzy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageDomain {
    System,
    Universal,
    Homebrew,
    Python,
    Node,
}

impl PackageDomain {
    pub fn result_label(&self) -> &'static str {
        match self {
            Self::System => "System Packages",
            Self::Universal => "Universal Applications",
            Self::Homebrew => "Homebrew",
            Self::Python => "Python Packages",
            Self::Node => "Node Packages",
        }
    }

    pub fn selection_warning(&self) -> Option<&'static str> {
        match self {
            Self::Python => Some(
                "Python registry matches may be unofficial, unrelated, abandoned, or malicious.",
            ),
            Self::Node => {
                Some("Node registry packages may run lifecycle scripts during installation.")
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    AppsAndTools,
    DeveloperEcosystems,
    AllSources,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeveloperTarget {
    Project,
    Workspace,
    Global,
    Environment,
    Tools,
    All,
}

impl DeveloperTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Workspace => "workspace",
            Self::Global => "global tools",
            Self::Environment => "active environment",
            Self::Tools => "isolated tools",
            Self::All => "all targets",
        }
    }

    pub fn cli_value(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Workspace => "workspace",
            Self::Global => "global",
            Self::Environment => "environment",
            Self::Tools => "tools",
            Self::All => "all",
        }
    }

    pub fn includes(self, target: Self) -> bool {
        self == Self::All || self == target
    }
}

impl FromStr for DeveloperTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "project" => Ok(Self::Project),
            "workspace" => Ok(Self::Workspace),
            "global" | "globals" | "global-tools" | "global_tools" => Ok(Self::Global),
            "environment" | "env" | "venv" => Ok(Self::Environment),
            "tools" | "tool" | "isolated-tools" | "isolated_tools" => Ok(Self::Tools),
            "all" => Ok(Self::All),
            _ => Err(format!(
                "invalid update target '{value}'; expected project, workspace, global, environment, tools, or all"
            )),
        }
    }
}

impl SearchScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::AppsAndTools => "Apps and tools",
            Self::DeveloperEcosystems => "Developer ecosystems",
            Self::AllSources => "All sources",
        }
    }

    pub fn cli_value(self) -> &'static str {
        match self {
            Self::AppsAndTools => "apps",
            Self::DeveloperEcosystems => "dev",
            Self::AllSources => "all",
        }
    }

    pub fn matches_backend_domains(self, domains: &[PackageDomain]) -> bool {
        domains
            .iter()
            .any(|domain| self.matches_backend_domain(*domain))
    }

    pub fn matches_candidate(self, candidate: &PackageCandidate) -> bool {
        match self {
            Self::AppsAndTools => matches!(
                candidate.result_section(),
                ResultSection::SystemPackages | ResultSection::UniversalApplications
            ),
            Self::DeveloperEcosystems => {
                candidate.result_section() == ResultSection::DeveloperEcosystems
            }
            Self::AllSources => true,
        }
    }

    fn matches_backend_domain(self, domain: PackageDomain) -> bool {
        match self {
            Self::AppsAndTools => matches!(
                domain,
                PackageDomain::System | PackageDomain::Universal | PackageDomain::Homebrew
            ),
            Self::DeveloperEcosystems => {
                matches!(domain, PackageDomain::Python | PackageDomain::Node)
            }
            Self::AllSources => true,
        }
    }
}

impl FromStr for SearchScope {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "apps" | "app" | "apps-and-tools" | "apps_and_tools" | "tools" => {
                Ok(Self::AppsAndTools)
            }
            "dev"
            | "developer"
            | "developers"
            | "developer-ecosystems"
            | "developer_ecosystems"
            | "ecosystems" => Ok(Self::DeveloperEcosystems),
            "all" | "all-sources" | "all_sources" => Ok(Self::AllSources),
            _ => Err(format!(
                "invalid search scope '{value}'; expected apps, dev, or all"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResultSection {
    SystemPackages,
    UniversalApplications,
    DeveloperEcosystems,
}

impl ResultSection {
    pub fn label(self) -> &'static str {
        match self {
            Self::SystemPackages => "System Packages",
            Self::UniversalApplications => "Universal Applications",
            Self::DeveloperEcosystems => "Developer Ecosystems",
        }
    }

    pub fn ordered_for_scope(scope: SearchScope) -> &'static [Self] {
        match scope {
            SearchScope::AppsAndTools => &[Self::SystemPackages, Self::UniversalApplications],
            SearchScope::DeveloperEcosystems => &[Self::DeveloperEcosystems],
            SearchScope::AllSources => &[
                Self::SystemPackages,
                Self::UniversalApplications,
                Self::DeveloperEcosystems,
            ],
        }
    }
}

impl fmt::Display for PackageDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::System => "system",
            Self::Universal => "universal",
            Self::Homebrew => "homebrew",
            Self::Python => "python",
            Self::Node => "node",
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PackageCandidate {
    pub backend_id: String,
    pub backend_name: String,
    pub category: BackendCategory,
    pub domain: PackageDomain,
    pub package_id: String,
    pub display_name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub source: Option<String>,
    pub installers: Vec<String>,
    pub artifact_kind: String,
    pub scope: Option<String>,
    pub match_kind: MatchKind,
    pub identity: IdentityMetadata,
}

impl PackageCandidate {
    pub fn infer_identity(
        match_kind: MatchKind,
        domain: PackageDomain,
        artifact_kind: &str,
    ) -> IdentityMetadata {
        let (name_match, distribution) = match match_kind {
            MatchKind::Exact => (
                NameMatchKind::Exact,
                DistributionRelationship::NameMatchOnly,
            ),
            MatchKind::Related => (NameMatchKind::Prefix, DistributionRelationship::Related),
            MatchKind::Fuzzy => (NameMatchKind::Fuzzy, DistributionRelationship::Fuzzy),
        };
        IdentityMetadata::baseline(
            name_match,
            distribution,
            software_type_for(domain, artifact_kind),
        )
    }

    pub fn refresh_inferred_identity(&mut self) {
        self.identity = Self::infer_identity(self.match_kind, self.domain, &self.artifact_kind);
    }

    pub fn result_section(&self) -> ResultSection {
        match self.domain {
            PackageDomain::System => ResultSection::SystemPackages,
            PackageDomain::Universal => ResultSection::UniversalApplications,
            PackageDomain::Homebrew => {
                if self.artifact_kind.to_ascii_lowercase().contains("cask") {
                    ResultSection::UniversalApplications
                } else {
                    ResultSection::SystemPackages
                }
            }
            PackageDomain::Python | PackageDomain::Node => ResultSection::DeveloperEcosystems,
        }
    }
}

fn software_type_for(domain: PackageDomain, artifact_kind: &str) -> SoftwareType {
    match domain {
        PackageDomain::System => SoftwareType::SystemPackage,
        PackageDomain::Universal => SoftwareType::UniversalApplication,
        PackageDomain::Homebrew => {
            if artifact_kind.to_ascii_lowercase().contains("cask") {
                SoftwareType::UniversalApplication
            } else {
                SoftwareType::SystemPackage
            }
        }
        PackageDomain::Python => SoftwareType::PythonPackage,
        PackageDomain::Node => SoftwareType::NodePackage,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledPackage {
    pub backend_id: String,
    pub backend_name: String,
    pub category: BackendCategory,
    pub domain: PackageDomain,
    pub package_id: String,
    pub display_name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub source: Option<String>,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackageInfo {
    pub backend_id: String,
    pub backend_name: String,
    pub category: BackendCategory,
    pub domain: PackageDomain,
    pub package_id: String,
    pub display_name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub source: Option<String>,
    pub scope: Option<String>,
    pub artifact_kind: Option<String>,
    pub installed: Option<bool>,
    pub extra: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::{MatchKind, PackageCandidate, PackageDomain, ResultSection, SearchScope};
    use crate::domain::BackendCategory;

    #[test]
    fn result_sections_follow_required_order_and_homebrew_artifact_type() {
        assert_eq!(
            ResultSection::ordered_for_scope(SearchScope::AllSources),
            &[
                ResultSection::SystemPackages,
                ResultSection::UniversalApplications,
                ResultSection::DeveloperEcosystems,
            ]
        );
        assert_eq!(
            candidate(PackageDomain::Homebrew, "Homebrew formula").result_section(),
            ResultSection::SystemPackages
        );
        assert_eq!(
            candidate(PackageDomain::Homebrew, "Homebrew cask").result_section(),
            ResultSection::UniversalApplications
        );
        assert_eq!(
            candidate(PackageDomain::Python, "Python package").result_section(),
            ResultSection::DeveloperEcosystems
        );
    }

    #[test]
    fn search_scope_filters_candidate_sections() {
        let system = candidate(PackageDomain::System, "system package");
        let universal = candidate(PackageDomain::Universal, "application");
        let python = candidate(PackageDomain::Python, "Python package");

        assert!(SearchScope::AppsAndTools.matches_candidate(&system));
        assert!(SearchScope::AppsAndTools.matches_candidate(&universal));
        assert!(!SearchScope::AppsAndTools.matches_candidate(&python));
        assert!(SearchScope::DeveloperEcosystems.matches_candidate(&python));
        assert!(!SearchScope::DeveloperEcosystems.matches_candidate(&system));
    }

    fn candidate(domain: PackageDomain, artifact_kind: &str) -> PackageCandidate {
        PackageCandidate {
            backend_id: "example".to_owned(),
            backend_name: "Example".to_owned(),
            category: BackendCategory::Development,
            domain,
            package_id: "demo".to_owned(),
            display_name: "demo".to_owned(),
            version: None,
            description: None,
            source: None,
            installers: Vec::new(),
            artifact_kind: artifact_kind.to_owned(),
            scope: None,
            match_kind: MatchKind::Exact,
            identity: PackageCandidate::infer_identity(MatchKind::Exact, domain, artifact_kind),
        }
    }
}
