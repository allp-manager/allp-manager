use crate::{
    build_identity::{AllpBuildIdentity, BuildChannel},
    release::{ReleaseAsset, ReleaseManifest, Version},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CONTINUOUS_WORKFLOW_NAME: &str = "Continuous Build";
pub const CONTINUOUS_WORKFLOW_PATH: &str = ".github/workflows/continuous-build.yml";
pub const CONTINUOUS_MANIFEST_NAME: &str = "allp-continuous-manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuousBuildManifest {
    pub schema_version: u32,
    pub channel: String,
    pub base_version: Version,
    pub build_revision: u64,
    pub display_version: String,
    pub git_commit: String,
    pub build_id: String,
    pub workflow_run_id: String,
    pub workflow_run_number: u64,
    pub workflow_name: String,
    pub workflow_file: String,
    pub built_at: String,
    pub minimum_updater_version: Version,
    pub assets: Vec<ReleaseAsset>,
}

impl ContinuousBuildManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "unsupported continuous manifest schema {}",
                self.schema_version
            ));
        }
        if self.channel != "continuous" {
            return Err("continuous manifest has the wrong channel".to_owned());
        }
        if self.build_revision == 0 || self.build_revision != self.workflow_run_number {
            return Err(
                "continuous build revision must equal the positive workflow run number".to_owned(),
            );
        }
        if self.display_version != format!("{}.{}", self.base_version, self.build_revision) {
            return Err("continuous manifest display version is inconsistent".to_owned());
        }
        if self.workflow_name != CONTINUOUS_WORKFLOW_NAME
            || self.workflow_file != CONTINUOUS_WORKFLOW_PATH
        {
            return Err("continuous manifest identifies an unexpected workflow".to_owned());
        }
        if self.workflow_run_id.parse::<u64>().is_err() || self.build_id.trim().is_empty() {
            return Err("continuous manifest has an invalid workflow/build ID".to_owned());
        }
        if self.built_at.trim().is_empty() {
            return Err("continuous manifest has no build timestamp".to_owned());
        }
        self.identity().validate_published()?;

        let mut targets = HashSet::new();
        for asset in &self.assets {
            if !targets.insert(asset.target.as_str()) {
                return Err(format!("duplicate continuous target: {}", asset.target));
            }
        }
        self.as_release_manifest().validate()
    }

    pub fn identity(&self) -> AllpBuildIdentity {
        AllpBuildIdentity {
            base_version: self.base_version,
            build_revision: self.build_revision,
            git_commit: self.git_commit.clone(),
            build_id: self.build_id.clone(),
            built_at: Some(self.built_at.clone()),
            channel: BuildChannel::Continuous,
            target: "multi-target".to_owned(),
            official: true,
        }
    }

    pub fn identity_for_target(&self, target: &str) -> Result<AllpBuildIdentity, String> {
        if target.trim().is_empty() {
            return Err("continuous build identity requires a non-empty target".to_owned());
        }
        let mut identity = self.identity();
        identity.target = target.to_owned();
        Ok(identity)
    }

    pub fn expected_tag(&self) -> String {
        format!("continuous-v{}", self.display_version)
    }

    pub fn as_release_manifest(&self) -> ReleaseManifest {
        // Target selection and archive validation are shared with stable releases. The outer
        // descriptor retains the real continuous tag used to construct the trusted asset URL.
        ReleaseManifest {
            schema_version: 1,
            version: self.base_version,
            tag: format!("v{}", self.base_version),
            channel: "prerelease".to_owned(),
            published_at: self.built_at.clone(),
            minimum_updater_version: self.minimum_updater_version,
            assets: self.assets.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_workflow_is_rejected() {
        let mut manifest = fixture();
        manifest.workflow_name = "Other Workflow".to_owned();
        assert!(manifest
            .validate()
            .expect_err("unexpected workflow must fail")
            .contains("unexpected workflow"));
    }

    #[test]
    fn revision_and_display_must_be_consistent() {
        let mut manifest = fixture();
        manifest.display_version = "0.3.5.3".to_owned();
        assert!(manifest
            .validate()
            .expect_err("inconsistent display must fail")
            .contains("display version"));
    }

    #[test]
    fn requested_target_identity_is_separate_from_asset_compatibility() {
        let manifest = fixture();
        let target = "aarch64-apple-darwin";
        let identity = manifest
            .identity_for_target(target)
            .expect("a requested target should produce comparison identity");
        assert_eq!(identity.target, target);
        assert!(manifest
            .as_release_manifest()
            .assets
            .iter()
            .all(|asset| asset.target != target));
    }

    fn fixture() -> ContinuousBuildManifest {
        ContinuousBuildManifest {
            schema_version: 1,
            channel: "continuous".to_owned(),
            base_version: Version::new(0, 3, 5),
            build_revision: 2,
            display_version: "0.3.5.2".to_owned(),
            git_commit: "a".repeat(40),
            build_id: "123.1".to_owned(),
            workflow_run_id: "123".to_owned(),
            workflow_run_number: 2,
            workflow_name: CONTINUOUS_WORKFLOW_NAME.to_owned(),
            workflow_file: CONTINUOUS_WORKFLOW_PATH.to_owned(),
            built_at: "2026-08-08T00:00:00Z".to_owned(),
            minimum_updater_version: Version::new(0, 3, 5),
            assets: vec![ReleaseAsset {
                target: "x86_64-unknown-linux-gnu".to_owned(),
                os: "linux".to_owned(),
                architecture: "x86_64".to_owned(),
                libc: Some("glibc".to_owned()),
                archive: "allp-0.3.5.2-x86_64-unknown-linux-gnu.tar.gz".to_owned(),
                binary: "allp".to_owned(),
                sha256: "a".repeat(64),
                size: 42,
            }],
        }
    }
}
