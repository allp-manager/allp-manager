use std::{error::Error, fmt, io};

pub type AllpResult<T> = Result<T, AllpError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AllpExitCode {
    Success = 0,
    InvalidCliOrInput = 2,
    PackageNotFound = 3,
    AmbiguousSelection = 4,
    BackendNotDetected = 5,
    UnsupportedOperation = 6,
    NativeCommandFailed = 7,
    PartialFailure = 8,
    TimeoutOrCancellation = 9,
    InternalError = 10,
    BackendBusy = 11,
}

impl AllpExitCode {
    pub fn code(self) -> u8 {
        self as u8
    }
}

#[derive(Debug)]
pub enum AllpError {
    InvalidInput(String),
    BackendNotDetected(String),
    NoConfiguredRemotes {
        backend: String,
    },
    UnsupportedOperation {
        backend: String,
        operation: String,
    },
    PackageNotFound(String),
    AmbiguousSelection(String),
    NonInteractiveSelectionRequired,
    ValidationFailed {
        backend: String,
        message: String,
    },
    CandidateUnavailable {
        backend: String,
        message: String,
    },
    ValidationStartFailed {
        backend: String,
        executable: String,
        reason: String,
    },
    MetadataParseFailed {
        backend: String,
        message: String,
    },
    PartialFailure(String),
    Timeout(String),
    CommandFailed {
        backend: String,
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    BackendBusy {
        backend: String,
        command: String,
        code: Option<i32>,
        lock_path: Option<String>,
        holder_pid: Option<u32>,
        holder_process: Option<String>,
    },
    Parse {
        backend: String,
        message: String,
    },
    Io(io::Error),
}

impl AllpError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::InvalidInput(_) => AllpExitCode::InvalidCliOrInput.code(),
            Self::PackageNotFound(_) => AllpExitCode::PackageNotFound.code(),
            Self::AmbiguousSelection(_) | Self::NonInteractiveSelectionRequired => {
                AllpExitCode::AmbiguousSelection.code()
            }
            Self::BackendNotDetected(_) => AllpExitCode::BackendNotDetected.code(),
            Self::NoConfiguredRemotes { .. } => AllpExitCode::BackendNotDetected.code(),
            Self::UnsupportedOperation { .. } => AllpExitCode::UnsupportedOperation.code(),
            Self::ValidationFailed { .. }
            | Self::CandidateUnavailable { .. }
            | Self::ValidationStartFailed { .. } => AllpExitCode::NativeCommandFailed.code(),
            Self::MetadataParseFailed { .. } => AllpExitCode::InternalError.code(),
            Self::CommandFailed { .. } => AllpExitCode::NativeCommandFailed.code(),
            Self::BackendBusy { .. } => AllpExitCode::BackendBusy.code(),
            Self::PartialFailure(_) => AllpExitCode::PartialFailure.code(),
            Self::Timeout(_) => AllpExitCode::TimeoutOrCancellation.code(),
            Self::Parse { .. } | Self::Io(_) => AllpExitCode::InternalError.code(),
        }
    }
}

impl fmt::Display for AllpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::BackendNotDetected(name) => write!(
                f,
                "The requested backend \"{name}\" is not available.\n\nRun:\n  allp detect --verbose"
            ),
            Self::NoConfiguredRemotes { backend } => {
                write!(f, "{backend} skipped · no configured remotes")
            }
            Self::UnsupportedOperation { backend, operation } => {
                write!(
                    f,
                    "{backend} does not support the '{operation}' operation.\n\nRun:\n  allp detect --verbose"
                )
            }
            Self::PackageNotFound(name) => write!(
                f,
                "Package \"{name}\" was not found.\n\nTry:\n  allp search {name} --all"
            ),
            Self::AmbiguousSelection(message) => write!(f, "{message}"),
            Self::NonInteractiveSelectionRequired => {
                write!(
                    f,
                    "Multiple choices were found, but interactive selection is disabled.\n\nUse --from with a backend ID or provide an exact package ID."
                )
            }
            Self::ValidationFailed { backend, message } => {
                write!(f, "{backend} validation failed")?;
                if !message.trim().is_empty() {
                    write!(f, "\n{}", message.trim())?;
                }
                Ok(())
            }
            Self::CandidateUnavailable { backend, message } => {
                write!(f, "{backend} candidate unavailable")?;
                if !message.trim().is_empty() {
                    write!(f, "\n{}", message.trim())?;
                }
                Ok(())
            }
            Self::ValidationStartFailed {
                backend,
                executable,
                reason,
            } => {
                write!(
                    f,
                    "Unable to start {backend} validation\n  executable: {executable}\n  reason: {reason}"
                )
            }
            Self::MetadataParseFailed { backend, message } => {
                write!(f, "{backend} metadata parsing failed")?;
                if !message.trim().is_empty() {
                    write!(f, "\n{}", message.trim())?;
                }
                Ok(())
            }
            Self::PartialFailure(message) => write!(f, "{message}"),
            Self::Timeout(message) => write!(f, "{message}"),
            Self::CommandFailed {
                backend,
                command,
                code,
                stderr,
            } => {
                write!(f, "{backend} command failed")?;
                if let Some(code) = code {
                    write!(f, " with exit code {code}")?;
                }
                write!(f, ": {command}")?;
                if !stderr.trim().is_empty() {
                    write!(f, "\n{}", stderr.trim())?;
                }
                Ok(())
            }
            Self::BackendBusy {
                backend,
                command,
                code,
                lock_path,
                holder_pid,
                holder_process,
            } => {
                write!(
                    f,
                    "{backend} is busy because another package-management operation owns a lock"
                )?;
                if let Some(lock_path) = lock_path {
                    write!(f, "\nLock: {lock_path}")?;
                }
                if let Some(process) = holder_process {
                    write!(f, "\nHolder: {process}")?;
                }
                if let Some(pid) = holder_pid {
                    write!(f, "\nPID: {pid}")?;
                }
                if let Some(code) = code {
                    write!(f, "\nNative exit code: {code}")?;
                }
                write!(
                    f,
                    "\nCommand: {command}\nAnother package-management operation is currently running. Do not remove the lock file."
                )
            }
            Self::Parse { backend, message } => {
                write!(f, "failed to parse {backend} output: {message}")
            }
            Self::Io(error) => write!(f, "I/O error: {error}"),
        }
    }
}

impl Error for AllpError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for AllpError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
