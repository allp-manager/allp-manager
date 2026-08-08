use crate::{
    discovery::{
        path::find_executable, DiscoveryReport, HomebrewDetectionState, HomebrewDiscovery,
    },
    platform::PlatformContext,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAvailability {
    Available,
    Unavailable,
    Unconfigured,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutableCapability {
    pub name: String,
    pub availability: CapabilityAvailability,
    pub resolved_path: Option<PathBuf>,
    pub version: Option<String>,
    pub owner_uid: Option<u32>,
    pub configuration_state: Option<String>,
    pub failure_reason: Option<String>,
    pub diagnostics: Vec<String>,
    pub can_bootstrap: bool,
    pub bootstrap_methods: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CapabilityRegistry {
    executables: BTreeMap<String, ExecutableCapability>,
}

impl CapabilityRegistry {
    pub fn probe<'a>(_context: &PlatformContext, names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut registry = Self::default();
        for name in names {
            registry.probe_executable(name);
        }
        registry
    }

    pub fn probe_defaults(context: &PlatformContext) -> Self {
        Self::probe(
            context,
            [
                "apt-get", "dnf", "pacman", "zypper", "apk", "flatpak", "snap", "brew", "curl",
                "tar", "sudo",
            ],
        )
    }

    pub fn probe_executable(&mut self, name: &str) -> &ExecutableCapability {
        self.executables.entry(name.to_owned()).or_insert_with(|| {
            let resolved_path = find_executable(name);
            let (availability, failure_reason, diagnostics) = match &resolved_path {
                Some(path) => (
                    CapabilityAvailability::Available,
                    None,
                    vec![format!("resolved executable: {}", path.display())],
                ),
                None => (
                    CapabilityAvailability::Unavailable,
                    Some("executable not found on PATH or a standard platform path".to_owned()),
                    Vec::new(),
                ),
            };
            ExecutableCapability {
                name: name.to_owned(),
                availability,
                owner_uid: resolved_path.as_deref().and_then(file_owner_uid),
                resolved_path,
                version: None,
                configuration_state: None,
                failure_reason,
                diagnostics,
                can_bootstrap: matches!(name, "flatpak" | "snap"),
                bootstrap_methods: Vec::new(),
            }
        })
    }

    pub fn executable(&self, name: &str) -> Option<&ExecutableCapability> {
        self.executables.get(name)
    }

    pub fn resolved_executable(&self, name: &str) -> Option<&Path> {
        self.executable(name)?.resolved_path.as_deref()
    }

    pub fn refresh_executable(&mut self, name: &str) -> &ExecutableCapability {
        self.executables.remove(name);
        self.probe_executable(name)
    }

    pub fn executables(&self) -> impl Iterator<Item = &ExecutableCapability> {
        self.executables.values()
    }

    /// Synchronize capabilities that use backend-specific validated discovery rather than PATH.
    pub fn apply_discovery(&mut self, discovery: &DiscoveryReport) {
        if let Some(homebrew) = discovery
            .entries
            .iter()
            .find(|entry| entry.backend_id == "brew")
            .and_then(|entry| entry.homebrew.as_ref())
        {
            self.apply_homebrew_discovery(homebrew);
        }
    }

    fn apply_homebrew_discovery(&mut self, discovery: &HomebrewDiscovery) {
        let capability = self
            .executables
            .entry("brew".to_owned())
            .or_insert_with(|| ExecutableCapability {
                name: "brew".to_owned(),
                availability: CapabilityAvailability::Unavailable,
                resolved_path: None,
                version: None,
                owner_uid: None,
                configuration_state: None,
                failure_reason: None,
                diagnostics: Vec::new(),
                can_bootstrap: false,
                bootstrap_methods: Vec::new(),
            });
        capability.diagnostics = discovery
            .attempts
            .iter()
            .map(|attempt| {
                let path = attempt
                    .executable
                    .as_ref()
                    .map(|path| format!(" · {}", path.display()))
                    .unwrap_or_default();
                format!(
                    "{}: {}{path}",
                    attempt.source.label(),
                    attempt.status.label()
                )
            })
            .collect();
        match &discovery.state {
            HomebrewDetectionState::Ready(installation) => {
                capability.availability = CapabilityAvailability::Available;
                capability.resolved_path = Some(installation.executable.clone());
                capability.version = Some(installation.version.clone());
                capability.owner_uid = installation.owner.uid;
                capability.configuration_state = Some("validated_ready".to_owned());
                capability.failure_reason = None;
            }
            HomebrewDetectionState::NotInstalled => {
                capability.availability = CapabilityAvailability::Unavailable;
                capability.resolved_path = None;
                capability.version = None;
                capability.owner_uid = None;
                capability.configuration_state = Some("not_installed".to_owned());
                capability.failure_reason =
                    Some("no validated Homebrew installation was found".to_owned());
            }
            state => {
                capability.availability = CapabilityAvailability::Error;
                capability.resolved_path =
                    state.problem().map(|problem| problem.executable.clone());
                capability.version = None;
                capability.owner_uid = None;
                capability.configuration_state = Some("installed_unusable".to_owned());
                capability.failure_reason = state.problem().map(|problem| problem.message.clone());
            }
        }
    }
}

fn file_owner_uid(path: &Path) -> Option<u32> {
    let metadata = fs::metadata(path).ok()?;
    #[cfg(unix)]
    {
        Some(metadata.uid())
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::RuntimePrivilegeContext, platform::PlatformContext};

    #[test]
    fn unavailable_executable_has_a_reason() {
        let context = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        let registry = CapabilityRegistry::probe(&context, ["allp-definitely-missing-command"]);
        let capability = registry
            .executable("allp-definitely-missing-command")
            .expect("capability should be recorded");
        assert_eq!(capability.availability, CapabilityAvailability::Unavailable);
        assert!(capability.failure_reason.is_some());
    }
}
