use serde::Serialize;
use std::{ffi::OsString, path::PathBuf, time::Duration};

/// The result at the administrator-privilege boundary.
///
/// This is deliberately distinct from a package-manager exit result. A
/// blocked privilege operation is useful both to the terminal renderer and to
/// machine-readable reports, without attributing an authentication problem to
/// the backend that was not allowed to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivilegeStatus {
    NotRequired,
    AlreadyRoot,
    Authenticated,
    AuthenticationFailed,
    AuthenticationCancelled,
    AuthenticationTimedOut,
    CredentialExpired,
    Unavailable,
}

impl PrivilegeStatus {
    pub fn permits_execution(self) -> bool {
        matches!(
            self,
            Self::NotRequired | Self::AlreadyRoot | Self::Authenticated
        )
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::NotRequired => "administrator access was not required",
            Self::AlreadyRoot => "already running as administrator",
            Self::Authenticated => "administrator authentication succeeded",
            Self::AuthenticationFailed => "administrator authentication failed",
            Self::AuthenticationCancelled => "administrator authentication cancelled",
            Self::AuthenticationTimedOut => "administrator authentication timed out",
            Self::CredentialExpired => "administrator credentials expired",
            Self::Unavailable => "administrator authentication unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Install,
    Bootstrap,
    Remove,
    Update,
    Upgrade,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Bootstrap => "bootstrap",
            Self::Remove => "remove",
            Self::Update => "update",
            Self::Upgrade => "upgrade",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OriginalUser {
    pub name: String,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum RuntimePrivilegeContext {
    NormalUser,
    RootDirect,
    SudoRootWithOriginalUser(OriginalUser),
}

impl RuntimePrivilegeContext {
    pub fn is_root(&self) -> bool {
        matches!(self, Self::RootDirect | Self::SudoRootWithOriginalUser(_))
    }

    pub fn original_user(&self) -> Option<&OriginalUser> {
        match self {
            Self::SudoRootWithOriginalUser(user) => Some(user),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeRequirement {
    NoElevation,
    RootRequired,
    OriginalUserRequired,
    Conditional,
}

impl PrivilegeRequirement {
    pub fn label(self, context: &RuntimePrivilegeContext) -> &'static str {
        match (self, context.is_root()) {
            (Self::NoElevation, _) => "Current user",
            (Self::RootRequired, true) => "Already running as administrator",
            (Self::RootRequired, false) => "Administrator access required",
            (Self::OriginalUserRequired, true) => "Original user context",
            (Self::OriginalUserRequired, false) => "Current user",
            (Self::Conditional, _) => "Conditional",
        }
    }

    pub fn requires_sudo(self, context: &RuntimePrivilegeContext) -> bool {
        self == Self::RootRequired && !context.is_root()
    }

    pub fn requires_original_user(self, context: &RuntimePrivilegeContext) -> bool {
        self == Self::OriginalUserRequired && context.is_root()
    }
}

#[derive(Debug, Clone)]
pub struct NativeCommand {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub current_dir: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
    pub timeout: Option<Duration>,
}

impl NativeCommand {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            current_dir: None,
            env: Vec::new(),
            timeout: None,
        }
    }

    pub fn arg(mut self, value: impl Into<OsString>) -> Self {
        self.args.push(value.into());
        self
    }

    pub fn args<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(values.into_iter().map(Into::into));
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub backend_id: String,
    pub backend_name: String,
    pub operation: OperationKind,
    pub action: String,
    pub package_id: Option<String>,
    pub source: Option<String>,
    pub scope: Option<String>,
    pub details: Vec<(String, String)>,
    pub command: NativeCommand,
    pub privilege: PrivilegeRequirement,
    pub requires_root: bool,
    pub interactive: bool,
}
