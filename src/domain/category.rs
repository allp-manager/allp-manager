use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendCategory {
    System,
    Universal,
    Development,
}

impl BackendCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System packages",
            Self::Universal => "Universal applications",
            Self::Development => "Development packages & tools",
        }
    }

    pub fn manager_label(self) -> &'static str {
        match self {
            Self::System => "System Package Managers",
            Self::Universal => "Universal Package Managers",
            Self::Development => "Development Package Managers",
        }
    }

    pub fn result_label(self) -> &'static str {
        match self {
            Self::System => "System Packages",
            Self::Universal => "Universal Applications",
            Self::Development => "Development Packages and Tools",
        }
    }
}

impl fmt::Display for BackendCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
