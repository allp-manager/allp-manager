use crate::{
    backends::{backend_matches_filter, Backend, CommandMap},
    discovery::{
        homebrew::{
            HomebrewDetectionState, HomebrewDiscovery, HomebrewLocator, SystemHomebrewLocator,
        },
        path::find_executable,
    },
    domain::{BackendCategory, Capability, PackageDomain, RuntimePrivilegeContext},
    execution::{privilege::runtime_context, ProcessRunner},
    platform::PlatformContext,
};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionState {
    Ready,
    NotFound,
    FoundButUnavailable,
    FoundButUnconfigured,
    UnsupportedVersion,
    ProbeFailed,
}

impl DetectionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::NotFound => "Not found",
            Self::FoundButUnavailable => "Unavailable",
            Self::FoundButUnconfigured => "Unconfigured",
            Self::UnsupportedVersion => "Unsupported version",
            Self::ProbeFailed => "Probe failed",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendDetection {
    pub backend_id: String,
    pub backend_name: String,
    pub category: BackendCategory,
    pub package_domains: Vec<PackageDomain>,
    pub state: DetectionState,
    pub capabilities: Vec<Capability>,
    pub aliases: Vec<String>,
    pub commands: BTreeMap<String, String>,
    pub missing: Vec<String>,
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homebrew: Option<HomebrewDiscovery>,
}

#[derive(Clone)]
pub struct DetectedBackend {
    pub backend: Arc<dyn Backend>,
    pub commands: CommandMap,
}

#[derive(Clone, Default)]
pub struct DetectedBackendSet {
    entries: Vec<DetectedBackend>,
}

impl DetectedBackendSet {
    pub fn new(entries: Vec<DetectedBackend>) -> Self {
        Self { entries }
    }

    pub fn iter(&self) -> impl Iterator<Item = &DetectedBackend> {
        self.entries.iter()
    }

    pub fn get(&self, backend_id: &str) -> Option<&DetectedBackend> {
        self.entries
            .iter()
            .find(|entry| backend_matches_filter(entry.backend.as_ref(), backend_id))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryReport {
    pub entries: Vec<BackendDetection>,
}

pub struct DiscoveryResult {
    pub report: DiscoveryReport,
    pub detected: DetectedBackendSet,
}

pub struct BackendDiscovery {
    catalog: Vec<Arc<dyn Backend>>,
    homebrew_locator: Arc<dyn HomebrewLocator>,
}

impl BackendDiscovery {
    pub fn new(catalog: Vec<Arc<dyn Backend>>) -> Self {
        Self {
            catalog,
            homebrew_locator: Arc::new(SystemHomebrewLocator::new()),
        }
    }

    pub fn with_homebrew_locator(
        catalog: Vec<Arc<dyn Backend>>,
        homebrew_locator: Arc<dyn HomebrewLocator>,
    ) -> Self {
        Self {
            catalog,
            homebrew_locator,
        }
    }

    pub fn discover(&self, runner: &dyn ProcessRunner) -> DiscoveryResult {
        let privilege = runtime_context();
        let platform = PlatformContext::detect(&privilege);
        self.discover_with_context(runner, &platform, &privilege)
    }

    pub fn discover_with_context(
        &self,
        runner: &dyn ProcessRunner,
        platform: &PlatformContext,
        privilege: &RuntimePrivilegeContext,
    ) -> DiscoveryResult {
        self.discover_with_context_filtered(runner, platform, privilege, None)
    }

    pub fn discover_with_context_filtered(
        &self,
        runner: &dyn ProcessRunner,
        platform: &PlatformContext,
        privilege: &RuntimePrivilegeContext,
        backend_filter: Option<&str>,
    ) -> DiscoveryResult {
        let mut report_entries = Vec::new();
        let mut detected_entries = Vec::new();

        for backend in &self.catalog {
            if backend_filter
                .is_some_and(|filter| !backend_matches_filter(backend.as_ref(), filter))
            {
                continue;
            }
            if backend.id() == "brew" {
                let homebrew = self.homebrew_locator.locate(platform, privilege, runner);
                let mut commands = CommandMap::new();
                let mut printable_commands = BTreeMap::new();
                let (state, missing, message) = match &homebrew.state {
                    HomebrewDetectionState::Ready(installation) => {
                        commands.insert("brew".to_owned(), installation.executable.clone());
                        printable_commands.insert(
                            "brew".to_owned(),
                            installation.executable.display().to_string(),
                        );
                        (
                            DetectionState::Ready,
                            Vec::new(),
                            Some(format!(
                                "Homebrew {} · prefix {} · owner {}",
                                installation.version,
                                installation.prefix.display(),
                                installation.owner.name
                            )),
                        )
                    }
                    HomebrewDetectionState::NotInstalled => (
                        DetectionState::NotFound,
                        vec!["brew".to_owned()],
                        Some("no validated Homebrew installation was found".to_owned()),
                    ),
                    state => (
                        DetectionState::FoundButUnavailable,
                        Vec::new(),
                        state.problem().map(|problem| problem.message.clone()),
                    ),
                };

                report_entries.push(BackendDetection {
                    backend_id: backend.id().to_owned(),
                    backend_name: backend.display_name().to_owned(),
                    category: backend.category(),
                    package_domains: backend.package_domains().to_vec(),
                    state,
                    capabilities: backend.capabilities().to_vec(),
                    aliases: backend
                        .aliases()
                        .iter()
                        .map(|alias| (*alias).to_owned())
                        .collect(),
                    commands: printable_commands,
                    missing,
                    message,
                    homebrew: Some(homebrew),
                });
                if state == DetectionState::Ready {
                    detected_entries.push(DetectedBackend {
                        backend: Arc::clone(backend),
                        commands,
                    });
                }
                continue;
            }

            let mut commands = CommandMap::new();
            let mut printable_commands = BTreeMap::new();
            let mut missing = Vec::new();

            for requirement in backend.command_requirements() {
                let resolved = requirement
                    .alternatives
                    .iter()
                    .find_map(|candidate| find_executable(candidate));

                match resolved {
                    Some(path) => {
                        printable_commands.insert(
                            requirement.key.to_owned(),
                            path.to_string_lossy().into_owned(),
                        );
                        commands.insert(requirement.key.to_owned(), path);
                    }
                    None => missing.push(format!(
                        "{} ({})",
                        requirement.key,
                        requirement.alternatives.join(" | ")
                    )),
                }
            }

            for requirement in backend.optional_command_requirements() {
                let resolved = requirement
                    .alternatives
                    .iter()
                    .find_map(|candidate| find_executable(candidate));
                if let Some(path) = resolved {
                    printable_commands.insert(
                        requirement.key.to_owned(),
                        path.to_string_lossy().into_owned(),
                    );
                    commands.insert(requirement.key.to_owned(), path);
                }
            }

            let mut state = if missing.is_empty() {
                DetectionState::Ready
            } else if commands.is_empty() {
                DetectionState::NotFound
            } else {
                DetectionState::FoundButUnavailable
            };
            let mut message = if missing.is_empty() {
                None
            } else {
                Some(format!(
                    "missing required command(s): {}",
                    missing.join(", ")
                ))
            };

            if state == DetectionState::Ready {
                if let Err(error) = backend.probe(&commands, runner) {
                    state = if matches!(error, crate::domain::AllpError::NoConfiguredRemotes { .. })
                    {
                        DetectionState::FoundButUnconfigured
                    } else {
                        DetectionState::FoundButUnavailable
                    };
                    message = Some(error.to_string());
                }
            }

            report_entries.push(BackendDetection {
                backend_id: backend.id().to_owned(),
                backend_name: backend.display_name().to_owned(),
                category: backend.category(),
                package_domains: backend.package_domains().to_vec(),
                state,
                capabilities: backend.capabilities().to_vec(),
                aliases: backend
                    .aliases()
                    .iter()
                    .map(|alias| (*alias).to_owned())
                    .collect(),
                commands: printable_commands,
                missing,
                message,
                homebrew: None,
            });

            if matches!(
                state,
                DetectionState::Ready | DetectionState::FoundButUnconfigured
            ) {
                detected_entries.push(DetectedBackend {
                    backend: Arc::clone(backend),
                    commands,
                });
            }
        }

        DiscoveryResult {
            report: DiscoveryReport {
                entries: report_entries,
            },
            detected: DetectedBackendSet::new(detected_entries),
        }
    }
}

pub type Detector = BackendDiscovery;
