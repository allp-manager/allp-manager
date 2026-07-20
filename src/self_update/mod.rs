mod checksum;
mod github;
mod replacement;

pub use github::{CurlHttpClient, GitHubReleaseSource, HttpClient, HttpResponse};
pub use replacement::{
    apply_replacement, replace_binary_atomically, run_deferred_replacement,
    schedule_deferred_replacement, stage_release, ReplacementOutcome, StagedRelease,
};

use crate::{
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
    Stable,
    Prerelease,
}

impl UpdateChannel {
    pub fn as_str(self) -> &'static str {
        match self {
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelfUpdateState {
    pub last_checked_at: Option<u64>,
    pub last_seen_version: Option<Version>,
    pub last_attempted_version: Option<Version>,
    pub last_successful_version: Option<Version>,
    pub etag: Option<String>,
    pub update_channel: UpdateChannel,
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
    pub release: Option<ReleaseDescriptor>,
    pub asset: Option<ReleaseAsset>,
    pub target: Option<String>,
    pub install_path: PathBuf,
    pub message: Option<String>,
}

pub struct SelfUpdater<'a> {
    source: &'a dyn ReleaseSource,
    platform: &'a PlatformContext,
    state_path: PathBuf,
}

impl<'a> SelfUpdater<'a> {
    pub fn new(
        source: &'a dyn ReleaseSource,
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
        let current_version = env!("CARGO_PKG_VERSION")
            .parse::<Version>()
            .map_err(AllpError::InvalidInput)?;
        let mut persisted =
            state::read_json::<SelfUpdateState>(&self.state_path)?.unwrap_or_default();
        persisted.update_channel = channel;

        if offline {
            state::write_json_atomically(&self.state_path, &persisted)?;
            return Ok(SelfUpdateCheck {
                availability: UpdateAvailability::Offline,
                current_version,
                release: None,
                asset: None,
                target: self.platform.target_triple(),
                install_path: self.platform.current_executable.clone(),
                message: Some("offline mode disabled the GitHub release check".to_owned()),
            });
        }

        let release = self.source.latest_release(channel, &current_version)?;
        persisted.last_checked_at = Some(unix_timestamp());
        if let Some(etag) = self.source.response_etag() {
            persisted.etag = Some(etag);
        }
        let Some(release) = release else {
            state::write_json_atomically(&self.state_path, &persisted)?;
            return Ok(SelfUpdateCheck {
                availability: UpdateAvailability::UpToDate,
                current_version,
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
        persisted.etag = release.etag.clone();

        let (availability, asset, message) = if release.version <= current_version {
            (UpdateAvailability::UpToDate, None, None)
        } else if release.manifest.minimum_updater_version > current_version {
            (
                UpdateAvailability::UpdaterTooOld,
                None,
                Some(format!(
                    "release {} requires updater {} or newer",
                    release.version, release.manifest.minimum_updater_version
                )),
            )
        } else if let Some(asset) = release.manifest.asset_for(self.platform).cloned() {
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
