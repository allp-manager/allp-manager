use crate::release::Version;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, fmt, str::FromStr};

pub const BASE_VERSION: &str = env!("ALLP_BASE_VERSION");
pub const BUILD_REVISION: &str = env!("ALLP_BUILD_REVISION");
pub const DISPLAY_VERSION: &str = env!("ALLP_DISPLAY_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildChannel {
    Stable,
    Continuous,
    Development,
}

impl BuildChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Continuous => "continuous",
            Self::Development => "development",
        }
    }
}

impl fmt::Display for BuildChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for BuildChannel {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "stable" => Ok(Self::Stable),
            "continuous" => Ok(Self::Continuous),
            "development" | "dev" | "local" => Ok(Self::Development),
            _ => Err(format!("invalid Allp build channel: {raw}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllpBuildIdentity {
    pub base_version: Version,
    pub build_revision: u64,
    pub git_commit: String,
    pub build_id: String,
    pub built_at: Option<String>,
    pub channel: BuildChannel,
    pub target: String,
    pub official: bool,
}

impl AllpBuildIdentity {
    pub fn current() -> Self {
        Self {
            base_version: BASE_VERSION
                .parse()
                .expect("build.rs emitted a valid Cargo base version"),
            build_revision: BUILD_REVISION
                .parse()
                .expect("build.rs emitted a numeric build revision"),
            git_commit: env!("ALLP_GIT_SHA").to_owned(),
            build_id: env!("ALLP_BUILD_ID").to_owned(),
            built_at: nonempty(env!("ALLP_BUILD_TIMESTAMP")),
            channel: env!("ALLP_BUILD_CHANNEL")
                .parse()
                .expect("build.rs emitted a valid build channel"),
            target: env!("ALLP_BUILD_TARGET").to_owned(),
            official: env!("ALLP_BUILD_OFFICIAL") == "1",
        }
    }

    pub fn display_version(&self) -> String {
        if self.channel == BuildChannel::Stable && self.build_revision == 0 {
            self.base_version.to_string()
        } else {
            format!("{}.{}", self.base_version, self.build_revision)
        }
    }

    pub fn validate_published(&self) -> Result<(), String> {
        if !self.official {
            return Err("published build identity is not marked official".to_owned());
        }
        if self.channel == BuildChannel::Development {
            return Err("published build identity uses the development channel".to_owned());
        }
        if self.channel == BuildChannel::Continuous && self.build_revision == 0 {
            return Err("continuous build revision must be positive".to_owned());
        }
        if !matches!(self.git_commit.len(), 40 | 64)
            || !self.git_commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("published build identity has an invalid Git commit".to_owned());
        }
        if self.build_id.trim().is_empty() {
            return Err("published build identity has an empty build ID".to_owned());
        }
        if self.target.trim().is_empty() {
            return Err("published build identity has an empty target".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildComparison {
    LocalAhead,
    SameBuild,
    SameSource,
    UpdateAvailable,
}

pub fn compare_builds(
    installed: &AllpBuildIdentity,
    remote: &AllpBuildIdentity,
) -> Result<BuildComparison, String> {
    match remote.base_version.cmp(&installed.base_version) {
        Ordering::Greater => return Ok(BuildComparison::UpdateAvailable),
        Ordering::Less => return Ok(BuildComparison::LocalAhead),
        Ordering::Equal => {}
    }

    let same_known_commit = known_commit(&installed.git_commit)
        && known_commit(&remote.git_commit)
        && installed
            .git_commit
            .eq_ignore_ascii_case(&remote.git_commit);
    if installed.build_revision == remote.build_revision {
        if installed
            .git_commit
            .eq_ignore_ascii_case(&remote.git_commit)
        {
            return Ok(BuildComparison::SameBuild);
        }
        // `make reinstall` embeds revision 1 for a local development build. That value is
        // deliberately not a GitHub Actions run number, so it can collide with a verified
        // continuous build even when the sources differ. The default update channel is the
        // trusted continuous channel; let that official candidate replace the local build
        // after the normal user confirmation instead of treating the collision as a release
        // integrity failure.
        if is_local_development_build(installed) && is_verified_continuous_build(remote) {
            return Ok(BuildComparison::UpdateAvailable);
        }
        return Err(format!(
            "build identity conflict: {}.{} maps to commits {} and {}",
            installed.base_version,
            installed.build_revision,
            installed.git_commit,
            remote.git_commit
        ));
    }
    if same_known_commit {
        return Ok(BuildComparison::SameSource);
    }
    Ok(if remote.build_revision > installed.build_revision {
        BuildComparison::UpdateAvailable
    } else {
        BuildComparison::LocalAhead
    })
}

fn is_local_development_build(identity: &AllpBuildIdentity) -> bool {
    identity.channel == BuildChannel::Development && !identity.official
}

fn is_verified_continuous_build(identity: &AllpBuildIdentity) -> bool {
    identity.channel == BuildChannel::Continuous && identity.official
}

pub fn short_version_output() -> String {
    format!("allp {DISPLAY_VERSION}")
}

pub fn verbose_version_output() -> String {
    let identity = AllpBuildIdentity::current();
    format!(
        "Allp {}\n\nBase version:\n  {}\n\nBuild revision:\n  {}\n\nChannel:\n  {}\n\nCommit:\n  {}\n\nBuild ID:\n  {}\n\nTarget:\n  {}\n\nBuilt at:\n  {}\n\nOfficial build:\n  {}",
        identity.display_version(),
        identity.base_version,
        identity.build_revision,
        identity.channel,
        identity.git_commit,
        identity.build_id,
        identity.target,
        identity.built_at.as_deref().unwrap_or("not recorded"),
        if identity.official { "yes" } else { "no (local/development provenance)" },
    )
}

fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn known_commit(commit: &str) -> bool {
    !commit.is_empty() && commit != "unknown"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(base: Version, revision: u64, commit: &str) -> AllpBuildIdentity {
        AllpBuildIdentity {
            base_version: base,
            build_revision: revision,
            git_commit: commit.to_owned(),
            build_id: revision.to_string(),
            built_at: None,
            channel: BuildChannel::Continuous,
            target: "x86_64-unknown-linux-gnu".to_owned(),
            official: true,
        }
    }

    #[test]
    fn same_base_version_uses_build_revision() {
        let installed = identity(Version::new(0, 3, 5), 1, &"a".repeat(40));
        let remote = identity(Version::new(0, 3, 5), 2, &"b".repeat(40));
        assert_eq!(
            compare_builds(&installed, &remote),
            Ok(BuildComparison::UpdateAvailable)
        );
        assert_eq!(
            compare_builds(&remote, &installed),
            Ok(BuildComparison::LocalAhead)
        );
    }

    #[test]
    fn base_semver_has_priority_over_revision() {
        let old_base = identity(Version::new(0, 3, 5), 99, &"a".repeat(40));
        let new_base = identity(Version::new(0, 3, 6), 1, &"b".repeat(40));
        assert_eq!(
            compare_builds(&old_base, &new_base),
            Ok(BuildComparison::UpdateAvailable)
        );
        assert_eq!(
            compare_builds(&new_base, &old_base),
            Ok(BuildComparison::LocalAhead)
        );
    }

    #[test]
    fn identical_build_is_current() {
        let installed = identity(Version::new(0, 3, 5), 2, &"a".repeat(40));
        assert_eq!(
            compare_builds(&installed, &installed),
            Ok(BuildComparison::SameBuild)
        );
    }

    #[test]
    fn same_commit_rebuild_is_not_forced() {
        let installed = identity(Version::new(0, 3, 5), 2, &"a".repeat(40));
        let rebuilt = identity(Version::new(0, 3, 5), 3, &"a".repeat(40));
        assert_eq!(
            compare_builds(&installed, &rebuilt),
            Ok(BuildComparison::SameSource)
        );
    }

    #[test]
    fn newer_verified_continuous_build_replaces_different_local_development_source() {
        let mut installed = identity(Version::new(0, 3, 5), 1, &"a".repeat(40));
        installed.channel = BuildChannel::Development;
        installed.official = false;
        let remote = identity(Version::new(0, 3, 5), 4, &"b".repeat(40));

        assert_eq!(
            compare_builds(&installed, &remote),
            Ok(BuildComparison::UpdateAvailable)
        );
    }

    #[test]
    fn verified_continuous_build_replaces_local_reinstall_on_revision_collision() {
        let mut installed = identity(Version::new(0, 3, 5), 1, &"a".repeat(40));
        installed.channel = BuildChannel::Development;
        installed.official = false;
        let remote = identity(Version::new(0, 3, 5), 1, &"b".repeat(40));

        assert_eq!(
            compare_builds(&installed, &remote),
            Ok(BuildComparison::UpdateAvailable)
        );
    }

    #[test]
    fn unverified_revision_collision_cannot_replace_local_development_build() {
        let mut installed = identity(Version::new(0, 3, 5), 1, &"a".repeat(40));
        installed.channel = BuildChannel::Development;
        installed.official = false;
        let mut remote = identity(Version::new(0, 3, 5), 1, &"b".repeat(40));
        remote.official = false;

        assert!(compare_builds(&installed, &remote)
            .expect_err("unverified revision collision must remain an integrity error")
            .contains("identity conflict"));
    }

    #[test]
    fn older_continuous_build_does_not_downgrade_local_development_source() {
        let mut installed = identity(Version::new(0, 3, 5), 4, &"a".repeat(40));
        installed.channel = BuildChannel::Development;
        installed.official = false;
        let remote = identity(Version::new(0, 3, 5), 3, &"b".repeat(40));

        assert_eq!(
            compare_builds(&installed, &remote),
            Ok(BuildComparison::LocalAhead)
        );
    }

    #[test]
    fn same_revision_with_different_commits_is_an_integrity_error() {
        let installed = identity(Version::new(0, 3, 5), 2, &"a".repeat(40));
        let remote = identity(Version::new(0, 3, 5), 2, &"b".repeat(40));
        assert!(compare_builds(&installed, &remote)
            .expect_err("ambiguous identity must fail")
            .contains("identity conflict"));
    }

    #[test]
    fn same_revision_with_unknown_and_known_commits_is_an_integrity_error() {
        let installed = identity(Version::new(0, 3, 5), 2, "unknown");
        let remote = identity(Version::new(0, 3, 5), 2, &"b".repeat(40));
        assert!(compare_builds(&installed, &remote)
            .expect_err("an unknown commit must not alias a published build")
            .contains("identity conflict"));
    }

    #[test]
    fn published_commit_length_is_exact() {
        let mut build = identity(Version::new(0, 3, 5), 2, &"a".repeat(41));
        assert!(build.validate_published().is_err());
        build.git_commit = "b".repeat(64);
        assert!(build.validate_published().is_ok());
    }

    #[test]
    fn requested_checkout_has_four_component_display_without_changing_cargo_semver() {
        assert_eq!(BASE_VERSION, env!("CARGO_PKG_VERSION"));
        let identity = AllpBuildIdentity::current();
        if identity.official {
            assert_eq!(DISPLAY_VERSION, identity.display_version());
        } else {
            assert_eq!(DISPLAY_VERSION, format!("{}.1", env!("CARGO_PKG_VERSION")));
            assert_eq!(identity.channel, BuildChannel::Development);
        }
    }
}
