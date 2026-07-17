use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Search,
    Install,
    Remove,
    Update,
    Upgrade,
    List,
    Info,
}

impl Capability {
    pub fn label(self) -> &'static str {
        match self {
            Self::Search => "Search",
            Self::Install => "Install",
            Self::Remove => "Remove",
            Self::Update => "Update",
            Self::Upgrade => "Upgrade",
            Self::List => "List",
            Self::Info => "Info",
        }
    }
}
