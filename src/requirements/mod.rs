use crate::platform::{OperatingSystem, PlatformContext};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    Required,
    Optional,
    OneOf,
    Service,
    Socket,
    Remote,
    Permission,
    Network,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Requirement {
    pub id: String,
    pub kind: RequirementKind,
    pub description: String,
    pub alternatives: Vec<String>,
    pub mutating_action: Option<RequirementMutation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementMutation {
    InstallExecutable,
    EnableService,
    AddRemote,
    ChangeConfiguration,
    ElevatePrivilege,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RequirementSet {
    pub supported: bool,
    pub unsupported_reason: Option<String>,
    pub requirements: Vec<Requirement>,
}

impl RequirementSet {
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            supported: false,
            unsupported_reason: Some(reason.into()),
            requirements: Vec::new(),
        }
    }

    pub fn supported(requirements: Vec<Requirement>) -> Self {
        Self {
            supported: true,
            unsupported_reason: None,
            requirements,
        }
    }
}

pub trait BackendRequirements {
    fn requirements(&self, context: &PlatformContext) -> RequirementSet;
}

pub fn snap_requirements(context: &PlatformContext) -> RequirementSet {
    if context.os != OperatingSystem::Linux {
        return RequirementSet::unsupported("Snap is supported only on Linux");
    }
    RequirementSet::supported(vec![
        Requirement {
            id: "snapd.socket".to_owned(),
            kind: RequirementKind::OneOf,
            description: "reachable snapd Unix socket (preferred transport)".to_owned(),
            alternatives: vec!["snap".to_owned()],
            mutating_action: Some(RequirementMutation::EnableService),
        },
        Requirement {
            id: "snap-store".to_owned(),
            kind: RequirementKind::Network,
            description: "Snap Store access".to_owned(),
            alternatives: Vec::new(),
            mutating_action: None,
        },
    ])
}

pub fn flatpak_requirements(context: &PlatformContext) -> RequirementSet {
    if context.os != OperatingSystem::Linux {
        return RequirementSet::unsupported("Flatpak is supported only on Linux");
    }
    RequirementSet::supported(vec![
        Requirement {
            id: "flatpak".to_owned(),
            kind: RequirementKind::Required,
            description: "Flatpak executable".to_owned(),
            alternatives: Vec::new(),
            mutating_action: Some(RequirementMutation::InstallExecutable),
        },
        Requirement {
            id: "flatpak.remote".to_owned(),
            kind: RequirementKind::Remote,
            description: "at least one configured Flatpak remote".to_owned(),
            alternatives: vec!["flathub".to_owned()],
            mutating_action: Some(RequirementMutation::AddRemote),
        },
    ])
}

pub fn self_update_requirements(_context: &PlatformContext) -> RequirementSet {
    RequirementSet::supported(vec![
        Requirement {
            id: "https-client".to_owned(),
            kind: RequirementKind::Required,
            description: "HTTPS client with bounded redirects and timeouts".to_owned(),
            alternatives: vec!["curl".to_owned()],
            mutating_action: None,
        },
        Requirement {
            id: "staging-directory".to_owned(),
            kind: RequirementKind::Permission,
            description: "writable update staging directory".to_owned(),
            alternatives: Vec::new(),
            mutating_action: None,
        },
    ])
}

pub fn bootstrap_requirement_for_backend(backend_id: &str) -> Option<Requirement> {
    let (id, description) = match backend_id {
        "flatpak" => ("flatpak", "Flatpak executable"),
        "snap" => ("snap", "Snap executable and snapd service package"),
        _ => return None,
    };
    Some(Requirement {
        id: id.to_owned(),
        kind: RequirementKind::Required,
        description: description.to_owned(),
        alternatives: Vec::new(),
        mutating_action: Some(RequirementMutation::InstallExecutable),
    })
}
