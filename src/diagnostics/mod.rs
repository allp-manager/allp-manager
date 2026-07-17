use crate::{
    backends::universal::flatpak::{detect_flatpak_state, FlatpakBackendState, FlatpakRemote},
    capabilities::{CapabilityAvailability, CapabilityRegistry},
    discovery::{DetectedBackendSet, DiscoveryReport},
    execution::ProcessRunner,
    platform::PlatformContext,
    self_update::OFFICIAL_REPOSITORY,
};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub platform: PlatformContext,
    pub allp_version: String,
    pub compatible_release_target: Option<String>,
    pub snap_socket: SocketDiagnostic,
    pub flatpak: FlatpakDiagnostic,
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
        let snap_socket = socket_diagnostic(snap_socket_path);
        let executables = capabilities
            .executables()
            .map(|capability| ExecutableDiagnostic {
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
            })
            .collect();
        let compatible_release_target = platform.target_triple();
        let flatpak = detected
            .get("flatpak")
            .map(
                |runtime| match detect_flatpak_state(&runtime.commands, runner) {
                    FlatpakBackendState::NotInstalled => FlatpakDiagnostic {
                        status: "not_installed".to_owned(),
                        remotes: Vec::new(),
                        reason: Some("executable not found".to_owned()),
                    },
                    FlatpakBackendState::InstalledWithoutRemotes => FlatpakDiagnostic {
                        status: "installed_without_remotes".to_owned(),
                        remotes: Vec::new(),
                        reason: Some("no configured remotes".to_owned()),
                    },
                    FlatpakBackendState::InstalledWithRemotes(remotes) => FlatpakDiagnostic {
                        status: "installed_with_remotes".to_owned(),
                        remotes,
                        reason: None,
                    },
                    FlatpakBackendState::BackendError(reason) => FlatpakDiagnostic {
                        status: "backend_error".to_owned(),
                        remotes: Vec::new(),
                        reason: Some(reason),
                    },
                },
            )
            .unwrap_or(FlatpakDiagnostic {
                status: "not_installed".to_owned(),
                remotes: Vec::new(),
                reason: Some("executable not found".to_owned()),
            });
        Self {
            platform,
            allp_version: env!("CARGO_PKG_VERSION").to_owned(),
            compatible_release_target,
            snap_socket,
            flatpak,
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
