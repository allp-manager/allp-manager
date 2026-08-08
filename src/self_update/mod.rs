mod checksum;
mod continuous;
mod github;
mod replacement;
mod trusted_helper;

pub use continuous::{
    ContinuousBuildManifest, CONTINUOUS_MANIFEST_NAME, CONTINUOUS_WORKFLOW_NAME,
    CONTINUOUS_WORKFLOW_PATH,
};
pub use github::{
    CurlHttpClient, GitHubActionsBuildSource, GitHubReleaseSource, HttpClient, HttpResponse,
};
pub use replacement::{
    apply_replacement, replace_binary_atomically, replace_binary_atomically_verified,
    run_deferred_replacement, run_deferred_replacement_verified, schedule_deferred_replacement,
    stage_release, ExpectedBinary, ExpectedBuildIdentity, ReplacementOutcome, StagedRelease,
};

use crate::{
    build_identity::{compare_builds, AllpBuildIdentity, BuildComparison},
    domain::{AllpError, AllpResult},
    platform::PlatformContext,
    release::{ReleaseAsset, ReleaseManifest, Version},
    state,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub const SELF_UPDATE_COMPLETED_ENV: &str = "ALLP_SELF_UPDATE_COMPLETED";
pub const SELF_UPDATE_VERSION_ENV: &str = "ALLP_SELF_UPDATE_VERSION";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitHubRepository {
    pub owner: &'static str,
    pub name: &'static str,
}

pub const OFFICIAL_REPOSITORY: GitHubRepository = GitHubRepository {
    owner: "allp-manager",
    name: "allp-manager",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UpdateChannel {
    #[default]
    Continuous,
    Stable,
    Prerelease,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Continuous => "continuous",
            Self::Stable => "stable",
            Self::Prerelease => "prerelease",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReleaseDescriptor {
    pub version: Version,
    pub tag: String,
    pub channel: UpdateChannel,
    pub published_at: Option<String>,
    pub manifest: ReleaseManifest,
    pub build_identity: Option<AllpBuildIdentity>,
    pub etag: Option<String>,
}

pub trait ReleaseSource {
    fn latest_release(
        &self,
        channel: UpdateChannel,
        current: &Version,
    ) -> AllpResult<Option<ReleaseDescriptor>>;

    fn response_etag(&self) -> Option<String> {
        None
    }
}

/// Channel-independent source of a verified distributable build.
///
/// `ReleaseSource` remains as a compatibility boundary for the tagged-release provider; the
/// updater itself depends on this build-oriented abstraction so equal Cargo versions can still
/// produce distinct update candidates.
pub trait BuildSource {
    fn latest_build(
        &self,
        channel: UpdateChannel,
        current: &Version,
        target: Option<&str>,
    ) -> AllpResult<Option<ReleaseDescriptor>>;

    fn response_etag(&self) -> Option<String> {
        None
    }
}

impl<T: ReleaseSource + ?Sized> BuildSource for T {
    fn latest_build(
        &self,
        channel: UpdateChannel,
        current: &Version,
        _target: Option<&str>,
    ) -> AllpResult<Option<ReleaseDescriptor>> {
        self.latest_release(channel, current)
    }

    fn response_etag(&self) -> Option<String> {
        ReleaseSource::response_etag(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelfUpdateState {
    pub last_checked_at: Option<u64>,
    pub last_seen_version: Option<Version>,
    pub last_attempted_version: Option<Version>,
    pub last_successful_version: Option<Version>,
    #[serde(default)]
    pub last_seen_build: Option<AllpBuildIdentity>,
    #[serde(default)]
    pub last_attempted_build: Option<AllpBuildIdentity>,
    #[serde(default)]
    pub last_successful_build: Option<AllpBuildIdentity>,
    pub etag: Option<String>,
    pub update_channel: UpdateChannel,
    /// Distinguishes an explicit user choice from the pre-continuous legacy default.
    #[serde(default)]
    pub channel_configured: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAvailability {
    Offline,
    UpToDate,
    Available,
    UnsupportedTarget,
    UpdaterTooOld,
}

#[derive(Debug, Clone)]
pub struct SelfUpdateCheck {
    pub availability: UpdateAvailability,
    pub current_version: Version,
    pub current_build: AllpBuildIdentity,
    pub release: Option<ReleaseDescriptor>,
    pub asset: Option<ReleaseAsset>,
    pub target: Option<String>,
    pub install_path: PathBuf,
    pub message: Option<String>,
}

pub struct SelfUpdater<'a> {
    source: &'a dyn BuildSource,
    platform: &'a PlatformContext,
    state_path: PathBuf,
}

impl<'a> SelfUpdater<'a> {
    pub fn new(
        source: &'a dyn BuildSource,
        platform: &'a PlatformContext,
        state_path: PathBuf,
    ) -> Self {
        Self {
            source,
            platform,
            state_path,
        }
    }

    pub fn check(&self, channel: UpdateChannel, offline: bool) -> AllpResult<SelfUpdateCheck> {
        let current_build = AllpBuildIdentity::current();
        let current_version = current_build.base_version;
        let mut persisted =
            state::read_json::<SelfUpdateState>(&self.state_path)?.unwrap_or_default();
        persisted.update_channel = channel;

        if offline {
            state::write_json_atomically(&self.state_path, &persisted)?;
            return Ok(SelfUpdateCheck {
                availability: UpdateAvailability::Offline,
                current_version,
                current_build,
                release: None,
                asset: None,
                target: self.platform.target_triple(),
                install_path: self.platform.current_executable.clone(),
                message: Some("offline mode disabled the GitHub release check".to_owned()),
            });
        }

        let target = self.platform.target_triple();
        if channel == UpdateChannel::Continuous && target.is_none() {
            persisted.last_checked_at = Some(unix_timestamp());
            state::write_json_atomically(&self.state_path, &persisted)?;
            return Ok(SelfUpdateCheck {
                availability: UpdateAvailability::UnsupportedTarget,
                current_version,
                current_build,
                release: None,
                asset: None,
                target,
                install_path: self.platform.current_executable.clone(),
                message: Some(
                    "continuous self-update has no compatible target for this platform".to_owned(),
                ),
            });
        }
        let release = self
            .source
            .latest_build(channel, &current_version, target.as_deref())?;
        persisted.last_checked_at = Some(unix_timestamp());
        if let Some(etag) = self.source.response_etag() {
            persisted.etag = Some(etag);
        }
        let Some(release) = release else {
            state::write_json_atomically(&self.state_path, &persisted)?;
            return Ok(SelfUpdateCheck {
                availability: UpdateAvailability::UpToDate,
                current_version,
                current_build,
                release: None,
                asset: None,
                target: self.platform.target_triple(),
                install_path: self.platform.current_executable.clone(),
                message: None,
            });
        };
        release
            .manifest
            .validate()
            .map_err(|message| AllpError::Parse {
                backend: "Allp self-update".to_owned(),
                message,
            })?;
        persisted.last_seen_version = Some(release.version);
        persisted.last_seen_build = release.build_identity.clone();
        persisted.etag = release.etag.clone();

        let compatible_asset = release.manifest.asset_for(self.platform).cloned();
        if channel == UpdateChannel::Continuous && compatible_asset.is_none() {
            let selected_target = target.as_deref().unwrap_or("this platform").to_owned();
            state::write_json_atomically(&self.state_path, &persisted)?;
            return Ok(SelfUpdateCheck {
                availability: UpdateAvailability::UnsupportedTarget,
                current_version,
                current_build,
                release: Some(release),
                asset: None,
                target,
                install_path: self.platform.current_executable.clone(),
                message: Some(format!(
                    "the latest continuous build has no compatible asset for {selected_target}"
                )),
            });
        }

        let comparison = if channel == UpdateChannel::Continuous {
            let remote = release.build_identity.as_ref().ok_or_else(|| {
                AllpError::InvalidInput(
                    "continuous update candidate has no compiled build identity".to_owned(),
                )
            })?;
            compare_builds(&current_build, remote).map_err(AllpError::InvalidInput)?
        } else if release.version > current_version {
            BuildComparison::UpdateAvailable
        } else if release.version < current_version {
            BuildComparison::LocalAhead
        } else {
            BuildComparison::SameBuild
        };

        let (availability, asset, message) = if matches!(
            comparison,
            BuildComparison::SameBuild | BuildComparison::SameSource | BuildComparison::LocalAhead
        ) {
            let message = match comparison {
                BuildComparison::SameSource => Some(
                    "a newer workflow rebuild has the same source commit; reinstall was not forced"
                        .to_owned(),
                ),
                BuildComparison::LocalAhead => {
                    Some("the installed Allp build is newer than the selected channel".to_owned())
                }
                _ => None,
            };
            (UpdateAvailability::UpToDate, None, message)
        } else if release.manifest.minimum_updater_version > current_version {
            (
                UpdateAvailability::UpdaterTooOld,
                None,
                Some(format!(
                    "release {} requires updater {} or newer",
                    release.version, release.manifest.minimum_updater_version
                )),
            )
        } else if let Some(asset) = compatible_asset {
            (UpdateAvailability::Available, Some(asset), None)
        } else {
            (
                UpdateAvailability::UnsupportedTarget,
                None,
                Some(format!(
                    "release {} has no compatible asset for {}",
                    release.version,
                    self.platform
                        .target_triple()
                        .unwrap_or_else(|| "this platform".to_owned())
                )),
            )
        };
        state::write_json_atomically(&self.state_path, &persisted)?;

        Ok(SelfUpdateCheck {
            availability,
            current_version,
            current_build,
            release: Some(release),
            asset,
            target: self.platform.target_triple(),
            install_path: self.platform.current_executable.clone(),
            message,
        })
    }

    pub fn mark_attempted(&self, version: Version) -> AllpResult<()> {
        let mut persisted =
            state::read_json::<SelfUpdateState>(&self.state_path)?.unwrap_or_default();
        persisted.last_attempted_version = Some(version);
        state::write_json_atomically(&self.state_path, &persisted)
    }

    pub fn mark_successful(&self, version: Version) -> AllpResult<()> {
        let mut persisted =
            state::read_json::<SelfUpdateState>(&self.state_path)?.unwrap_or_default();
        persisted.last_successful_version = Some(version);
        state::write_json_atomically(&self.state_path, &persisted)
    }

    pub fn mark_attempted_build(&self, build: &AllpBuildIdentity) -> AllpResult<()> {
        let mut persisted =
            state::read_json::<SelfUpdateState>(&self.state_path)?.unwrap_or_default();
        persisted.last_attempted_version = Some(build.base_version);
        persisted.last_attempted_build = Some(build.clone());
        state::write_json_atomically(&self.state_path, &persisted)
    }

    pub fn mark_successful_build(&self, build: &AllpBuildIdentity) -> AllpResult<()> {
        let mut persisted =
            state::read_json::<SelfUpdateState>(&self.state_path)?.unwrap_or_default();
        persisted.last_successful_version = Some(build.base_version);
        persisted.last_successful_build = Some(build.clone());
        state::write_json_atomically(&self.state_path, &persisted)
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::RuntimePrivilegeContext,
        platform::{Architecture, LibcFamily, OperatingSystem, PlatformContext},
        release::{ReleaseAsset, ReleaseManifest},
    };
    use std::sync::Mutex;

    struct StaticSource {
        calls: Mutex<usize>,
        release: Option<ReleaseDescriptor>,
        etag: Option<String>,
    }

    impl ReleaseSource for StaticSource {
        fn latest_release(
            &self,
            _channel: UpdateChannel,
            _current: &Version,
        ) -> AllpResult<Option<ReleaseDescriptor>> {
            *self.calls.lock().unwrap() += 1;
            Ok(self.release.clone())
        }

        fn response_etag(&self) -> Option<String> {
            self.etag.clone()
        }
    }

    #[test]
    fn offline_check_never_calls_release_source() {
        let source = StaticSource {
            calls: Mutex::new(0),
            release: None,
            etag: None,
        };
        let platform = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        let state = std::env::temp_dir().join(format!(
            "allp-self-update-offline-{}.json",
            std::process::id()
        ));
        let updater = SelfUpdater::new(&source, &platform, state);
        let check = updater
            .check(UpdateChannel::Stable, true)
            .expect("offline check should succeed");
        assert_eq!(check.availability, UpdateAvailability::Offline);
        assert_eq!(*source.calls.lock().unwrap(), 0);
        let persisted = state::read_json::<SelfUpdateState>(&updater.state_path)
            .expect("offline state should read")
            .expect("offline state should exist");
        assert_eq!(persisted.update_channel, UpdateChannel::Stable);
        let _ = std::fs::remove_file(&updater.state_path);
    }

    #[test]
    fn newer_release_selects_the_exact_platform_asset() {
        let release = release_descriptor(true);
        let source = StaticSource {
            calls: Mutex::new(0),
            release: Some(release),
            etag: Some("etag-new".to_owned()),
        };
        let platform = linux_x86_platform();
        let state_path = temporary_state("available");
        let updater = SelfUpdater::new(&source, &platform, state_path.clone());
        let check = updater
            .check(UpdateChannel::Stable, false)
            .expect("newer release should check");
        assert_eq!(check.availability, UpdateAvailability::Available);
        assert_eq!(
            check.asset.as_ref().map(|asset| asset.target.as_str()),
            Some("x86_64-unknown-linux-gnu")
        );
        let persisted = state::read_json::<SelfUpdateState>(&state_path)
            .expect("state should read")
            .expect("state should exist");
        assert_eq!(persisted.etag.as_deref(), Some("etag-new"));
        assert_eq!(persisted.last_seen_version, Some(next_patch_version()));
        let _ = std::fs::remove_file(state_path);
    }

    #[test]
    fn missing_target_is_structured_and_does_not_stage_an_update() {
        let source = StaticSource {
            calls: Mutex::new(0),
            release: Some(release_descriptor(false)),
            etag: None,
        };
        let platform = linux_x86_platform();
        let state_path = temporary_state("unsupported");
        let check = SelfUpdater::new(&source, &platform, state_path.clone())
            .check(UpdateChannel::Stable, false)
            .expect("unsupported target should be structured");
        assert_eq!(check.availability, UpdateAvailability::UnsupportedTarget);
        assert!(check.asset.is_none());
        let _ = std::fs::remove_file(state_path);
    }

    #[test]
    fn unsupported_continuous_platform_does_not_call_build_source() {
        let source = StaticSource {
            calls: Mutex::new(0),
            release: None,
            etag: None,
        };
        let mut platform = linux_x86_platform();
        platform.os = OperatingSystem::Other;
        platform.libc = None;
        let state_path = temporary_state("unsupported-continuous-platform");

        let check = SelfUpdater::new(&source, &platform, state_path.clone())
            .check(UpdateChannel::Continuous, false)
            .expect("unsupported continuous platform should be a structured result");

        assert_eq!(check.availability, UpdateAvailability::UnsupportedTarget);
        assert!(check.release.is_none());
        assert!(check.asset.is_none());
        assert!(check.target.is_none());
        assert_eq!(*source.calls.lock().unwrap(), 0);
        let _ = std::fs::remove_file(state_path);
    }

    #[test]
    fn continuous_manifest_without_requested_asset_is_structured() {
        let mut release = release_descriptor(false);
        release.channel = UpdateChannel::Continuous;
        release.manifest.channel = "prerelease".to_owned();
        release.build_identity = Some(AllpBuildIdentity {
            base_version: release.version,
            build_revision: 2,
            git_commit: "a".repeat(40),
            build_id: "123.1".to_owned(),
            built_at: Some("2026-08-08T00:00:00Z".to_owned()),
            channel: crate::build_identity::BuildChannel::Continuous,
            target: "x86_64-unknown-linux-gnu".to_owned(),
            official: true,
        });
        let source = StaticSource {
            calls: Mutex::new(0),
            release: Some(release),
            etag: None,
        };
        let platform = linux_x86_platform();
        let state_path = temporary_state("continuous-missing-target");

        let check = SelfUpdater::new(&source, &platform, state_path.clone())
            .check(UpdateChannel::Continuous, false)
            .expect("missing continuous target should be structured");

        assert_eq!(check.availability, UpdateAvailability::UnsupportedTarget);
        assert!(check.release.is_some());
        assert!(check.asset.is_none());
        assert_eq!(*source.calls.lock().unwrap(), 1);
        assert!(check
            .message
            .as_deref()
            .is_some_and(|message| message.contains("x86_64-unknown-linux-gnu")));
        let _ = std::fs::remove_file(state_path);
    }

    #[test]
    fn no_newer_release_persists_response_etag() {
        let source = StaticSource {
            calls: Mutex::new(0),
            release: None,
            etag: Some("etag-current".to_owned()),
        };
        let platform = linux_x86_platform();
        let state_path = temporary_state("current");
        let check = SelfUpdater::new(&source, &platform, state_path.clone())
            .check(UpdateChannel::Stable, false)
            .expect("up-to-date check should succeed");
        assert_eq!(check.availability, UpdateAvailability::UpToDate);
        let persisted = state::read_json::<SelfUpdateState>(&state_path)
            .expect("state should read")
            .expect("state should exist");
        assert_eq!(persisted.etag.as_deref(), Some("etag-current"));
        let _ = std::fs::remove_file(state_path);
    }

    #[test]
    fn malformed_manifest_is_rejected_before_asset_selection() {
        let mut release = release_descriptor(true);
        release.manifest.schema_version = 99;
        let source = StaticSource {
            calls: Mutex::new(0),
            release: Some(release),
            etag: None,
        };
        let platform = linux_x86_platform();
        let state_path = temporary_state("malformed");
        let error = SelfUpdater::new(&source, &platform, state_path.clone())
            .check(UpdateChannel::Stable, false)
            .expect_err("malformed manifest must fail");
        assert!(error
            .to_string()
            .contains("unsupported release manifest schema"));
        assert!(!state_path.exists());
    }

    fn linux_x86_platform() -> PlatformContext {
        let mut platform = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        platform.os = OperatingSystem::Linux;
        platform.architecture = Architecture::X86_64;
        platform.libc = Some(LibcFamily::Glibc);
        platform
    }

    fn release_descriptor(with_matching_asset: bool) -> ReleaseDescriptor {
        let version = next_patch_version();
        let tag = format!("v{version}");
        let target = if with_matching_asset {
            "x86_64-unknown-linux-gnu"
        } else {
            "aarch64-unknown-linux-gnu"
        };
        let architecture = if with_matching_asset {
            "x86_64"
        } else {
            "aarch64"
        };
        ReleaseDescriptor {
            version,
            tag: tag.clone(),
            channel: UpdateChannel::Stable,
            published_at: Some("2026-07-17T00:00:00Z".to_owned()),
            manifest: ReleaseManifest {
                schema_version: 1,
                version,
                tag,
                channel: "stable".to_owned(),
                published_at: "2026-07-17T00:00:00Z".to_owned(),
                minimum_updater_version: Version::new(0, 3, 3),
                assets: vec![ReleaseAsset {
                    target: target.to_owned(),
                    os: "linux".to_owned(),
                    architecture: architecture.to_owned(),
                    libc: Some("glibc".to_owned()),
                    archive: format!("allp-v{version}-{target}.tar.gz"),
                    binary: "allp".to_owned(),
                    sha256: "a".repeat(64),
                    size: 42,
                }],
            },
            build_identity: None,
            etag: Some("etag-new".to_owned()),
        }
    }

    fn next_patch_version() -> Version {
        let current = env!("CARGO_PKG_VERSION")
            .parse::<Version>()
            .expect("package version should be valid semantic versioning");
        Version::new(current.major, current.minor, current.patch + 1)
    }

    fn temporary_state(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "allp-self-update-{label}-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ))
    }
}
