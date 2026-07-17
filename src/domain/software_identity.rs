use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NameMatchKind {
    Exact,
    NormalizedExact,
    Alias,
    Prefix,
    Token,
    Fuzzy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityConfidence {
    Official,
    Verified,
    Probable,
    Unverified,
    Conflicting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionRelationship {
    OfficialInstaller,
    OfficialPackage,
    VerifiedThirdPartyPackage,
    NameMatchOnly,
    Related,
    Fuzzy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareType {
    PackageManager,
    SystemPackage,
    UniversalApplication,
    DesktopApplication,
    CliTool,
    LanguageRuntime,
    RegistryClient,
    LanguageLibrary,
    NodePackage,
    PythonPackage,
    OfficialInstaller,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IdentityMetadata {
    pub name_match: NameMatchKind,
    pub confidence: IdentityConfidence,
    pub distribution: DistributionRelationship,
    pub software_type: SoftwareType,
    pub canonical_id: Option<String>,
    pub canonical_name: Option<String>,
    pub official_source: bool,
    pub warning: Option<String>,
}

impl IdentityMetadata {
    pub fn baseline(
        name_match: NameMatchKind,
        distribution: DistributionRelationship,
        software_type: SoftwareType,
    ) -> Self {
        Self {
            name_match,
            confidence: IdentityConfidence::Unverified,
            distribution,
            software_type,
            canonical_id: None,
            canonical_name: None,
            official_source: false,
            warning: None,
        }
    }

    pub fn official(
        canonical_id: &str,
        canonical_name: &str,
        software_type: SoftwareType,
        distribution: DistributionRelationship,
    ) -> Self {
        Self {
            name_match: NameMatchKind::Exact,
            confidence: IdentityConfidence::Official,
            distribution,
            software_type,
            canonical_id: Some(canonical_id.to_owned()),
            canonical_name: Some(canonical_name.to_owned()),
            official_source: true,
            warning: None,
        }
    }

    pub fn label(&self) -> &'static str {
        match (self.confidence, self.distribution) {
            (IdentityConfidence::Official, DistributionRelationship::OfficialInstaller) => {
                "Official installer"
            }
            (IdentityConfidence::Official, DistributionRelationship::OfficialPackage) => {
                "Official package"
            }
            (IdentityConfidence::Verified, DistributionRelationship::VerifiedThirdPartyPackage) => {
                "Verified third-party"
            }
            (IdentityConfidence::Conflicting, DistributionRelationship::NameMatchOnly) => {
                "Conflicting name"
            }
            (_, DistributionRelationship::NameMatchOnly) => "Exact package name",
            (_, DistributionRelationship::Related) => "Related",
            (_, DistributionRelationship::Fuzzy) => "Fuzzy",
            _ => "Probable identity",
        }
    }

    pub fn rank(&self) -> u8 {
        match (self.confidence, self.distribution) {
            (IdentityConfidence::Official, DistributionRelationship::OfficialInstaller) => 0,
            (IdentityConfidence::Official, _) => 1,
            (IdentityConfidence::Verified, _) => 2,
            (IdentityConfidence::Probable, _) => 3,
            (IdentityConfidence::Unverified, _) => 4,
            (IdentityConfidence::Conflicting, _) => 5,
        }
    }

    pub fn is_official(&self) -> bool {
        self.confidence == IdentityConfidence::Official && self.official_source
    }

    pub fn is_conflicting(&self) -> bool {
        self.confidence == IdentityConfidence::Conflicting
    }
}

impl Default for IdentityMetadata {
    fn default() -> Self {
        Self::baseline(
            NameMatchKind::Fuzzy,
            DistributionRelationship::Fuzzy,
            SoftwareType::Unknown,
        )
    }
}
