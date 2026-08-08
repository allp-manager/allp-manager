use crate::{
    backends::universal::flatpak::{detect_flatpak_probe, FlatpakBackendState, FlatpakRemote},
    capabilities::{CapabilityAvailability, CapabilityRegistry},
    discovery::{DetectedBackendSet, DiscoveryReport, HomebrewDiscovery},
    execution::ProcessRunner,
    platform::PlatformContext,
    self_update::OFFICIAL_REPOSITORY,
};
use serde::Serialize;
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub platform: PlatformContext,
    pub allp_version: String,
    pub compatible_release_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snap_socket: Option<SocketDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flatpak: Option<FlatpakDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homebrew: Option<HomebrewDiscovery>,
    pub executables: Vec<ExecutableDiagnostic>,
    pub backends: Vec<crate::discovery::BackendDetection>,
    pub github_repository: String,
    pub github_update_source_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SocketDiagnostic {
    pub path: String,
    pub exists: bool,
    pub reachable: Option<bool>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutableDiagnostic {
    pub name: String,
    pub status: String,
    pub path: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlatpakDiagnostic {
    pub status: String,
    pub remotes: Vec<FlatpakRemote>,
    pub reason: Option<String>,
}

impl DoctorReport {
    pub fn collect(
        platform: PlatformContext,
        capabilities: &CapabilityRegistry,
        discovery: &DiscoveryReport,
        detected: &DetectedBackendSet,
        runner: &dyn ProcessRunner,
        snap_socket_path: &Path,
    ) -> Self {
        let snap_socket = discovery
            .entries
            .iter()
            .any(|entry| entry.backend_id == "snap")
            .then(|| socket_diagnostic(snap_socket_path));
        let homebrew = discovery
            .entries
            .iter()
            .find(|entry| entry.backend_id == "brew")
            .and_then(|entry| entry.homebrew.clone());
        let executables = capabilities
            .executables()
            .map(|capability| {
                if capability.name == "brew" {
                    if let Some(installation) = homebrew
                        .as_ref()
                        .and_then(|discovery| discovery.state.installation())
                    {
                        return ExecutableDiagnostic {
                            name: capability.name.clone(),
                            status: "available".to_owned(),
                            path: Some(installation.executable.display().to_string()),
                            reason: None,
                        };
                    }
                }
                ExecutableDiagnostic {
                    name: capability.name.clone(),
                    status: match capability.availability {
                        CapabilityAvailability::Available => "available",
                        CapabilityAvailability::Unavailable => "unavailable",
                        CapabilityAvailability::Unconfigured => "unconfigured",
                        CapabilityAvailability::Error => "error",
                    }
                    .to_owned(),
                    path: capability
                        .resolved_path
                        .as_ref()
                        .map(|path| path.display().to_string()),
                    reason: capability.failure_reason.clone(),
                }
            })
            .collect();
        let compatible_release_target = platform.target_triple();
        let flatpak = discovery
            .entries
            .iter()
            .any(|entry| entry.backend_id == "flatpak")
            .then(|| {
                detected
                    .get("flatpak")
                    .map(|runtime| {
                        let probe = detect_flatpak_probe(&runtime.commands, runner);
                        let state = probe.state.clone();
                        match state {
                            FlatpakBackendState::Missing => FlatpakDiagnostic {
                                status: "not_installed".to_owned(),
                                remotes: Vec::new(),
                                reason: Some("executable not found".to_owned()),
                            },
                            FlatpakBackendState::InstalledNoRemotes => FlatpakDiagnostic {
                                status: "installed_no_remotes".to_owned(),
                                remotes: probe.remotes,
                                reason: Some("no configured user or system remotes".to_owned()),
                            },
                            FlatpakBackendState::InstalledRefsWithoutUsableRemote => {
                                FlatpakDiagnostic {
                                    status: "installed_refs_without_usable_remote".to_owned(),
                                    remotes: probe.remotes,
                                    reason: Some(
                                        "installed refs exist without a configured usable remote"
                                            .to_owned(),
                                    ),
                                }
                            }
                            FlatpakBackendState::InstalledUserScopeReady
                            | FlatpakBackendState::InstalledSystemScopeReady
                            | FlatpakBackendState::InstalledBothScopesReady => {
                                let status = match state {
                                    FlatpakBackendState::InstalledUserScopeReady => {
                                        "installed_user_scope_ready"
                                    }
                                    FlatpakBackendState::InstalledSystemScopeReady => {
                                        "installed_system_scope_ready"
                                    }
                                    FlatpakBackendState::InstalledBothScopesReady => {
                                        "installed_both_scopes_ready"
                                    }
                                    _ => unreachable!("matched ready flatpak state"),
                                };
                                FlatpakDiagnostic {
                                    status: status.to_owned(),
                                    remotes: probe.remotes,
                                    reason: None,
                                }
                            }
                            FlatpakBackendState::BackendError(_) => FlatpakDiagnostic {
                                status: "backend_error".to_owned(),
                                remotes: probe.remotes,
                                reason: probe.reason,
                            },
                        }
                    })
                    .unwrap_or(FlatpakDiagnostic {
                        status: "not_installed".to_owned(),
                        remotes: Vec::new(),
                        reason: Some("executable not found".to_owned()),
                    })
            });
        Self {
            platform,
            allp_version: crate::build_identity::DISPLAY_VERSION.to_owned(),
            compatible_release_target,
            snap_socket,
            flatpak,
            homebrew,
            executables,
            backends: discovery.entries.clone(),
            github_repository: format!(
                "{}/{}",
                OFFICIAL_REPOSITORY.owner, OFFICIAL_REPOSITORY.name
            ),
            github_update_source_status:
                "trusted source configured; network not contacted by doctor".to_owned(),
        }
    }

    /// Restrict backend-specific diagnostics while retaining common platform/Allp context.
    pub fn retain_backend(&mut self, filter: &str) -> bool {
        let matching = self
            .backends
            .iter()
            .filter(|entry| {
                entry.backend_id.eq_ignore_ascii_case(filter)
                    || entry.backend_name.eq_ignore_ascii_case(filter)
                    || entry
                        .aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(filter))
            })
            .map(|entry| entry.backend_id.clone())
            .collect::<BTreeSet<_>>();
        if matching.is_empty() {
            return false;
        }

        let mut executable_names = self
            .backends
            .iter()
            .filter(|entry| matching.contains(&entry.backend_id))
            .flat_map(|entry| entry.commands.keys().cloned())
            .collect::<BTreeSet<_>>();
        for (backend, executable) in [("brew", "brew"), ("snap", "snap"), ("flatpak", "flatpak")] {
            if matching.contains(backend) {
                executable_names.insert(executable.to_owned());
            }
        }
        self.backends
            .retain(|entry| matching.contains(&entry.backend_id));
        self.executables
            .retain(|executable| executable_names.contains(&executable.name));
        if !matching.contains("brew") {
            self.homebrew = None;
        }
        if !matching.contains("snap") {
            self.snap_socket = None;
        }
        if !matching.contains("flatpak") {
            self.flatpak = None;
        }
        true
    }
}

fn socket_diagnostic(path: &Path) -> SocketDiagnostic {
    if !path.exists() {
        return SocketDiagnostic {
            path: path.display().to_string(),
            exists: false,
            reachable: None,
            reason: Some("socket does not exist".to_owned()),
        };
    }
    #[cfg(unix)]
    {
        match std::os::unix::net::UnixStream::connect(path) {
            Ok(_) => SocketDiagnostic {
                path: path.display().to_string(),
                exists: true,
                reachable: Some(true),
                reason: None,
            },
            Err(error) => SocketDiagnostic {
                path: path.display().to_string(),
                exists: true,
                reachable: Some(false),
                reason: Some(error.to_string()),
            },
        }
    }
    #[cfg(not(unix))]
    {
        SocketDiagnostic {
            path: path.display().to_string(),
            exists: true,
            reachable: None,
            reason: Some("Unix sockets are unsupported on this platform".to_owned()),
        }
    }
}
