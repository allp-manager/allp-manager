use crate::domain::{CandidateGroup, PackageCandidate, PrivilegeStatus, SearchScope};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendSearchIssueKind {
    UnrecognizedOutput,
    CommandFailed,
    IncompleteMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendSearchIssue {
    pub kind: BackendSearchIssueKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct BackendSearchReport {
    pub candidates: Vec<PackageCandidate>,
    pub issues: Vec<BackendSearchIssue>,
}

impl BackendSearchReport {
    pub fn complete(candidates: Vec<PackageCandidate>) -> Self {
        Self {
            candidates,
            issues: Vec::new(),
        }
    }

    pub fn with_issue(mut self, issue: BackendSearchIssue) -> Self {
        self.issues.push(issue);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendIssue {
    pub backend_id: String,
    pub backend_name: String,
    pub kind: BackendSearchIssueKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchReport {
    pub query: String,
    pub effective_scope: SearchScope,
    pub complete: bool,
    pub candidates: Vec<PackageCandidate>,
    pub groups: Vec<CandidateGroup>,
    pub issues: Vec<BackendIssue>,
    #[serde(skip)]
    pub backend_summaries: Vec<SearchBackendSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchBackendSummary {
    pub backend_id: String,
    pub backend_name: String,
    pub state: SearchBackendState,
    pub result_count: usize,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchBackendState {
    Available,
    Unavailable,
    Skipped,
    NoConfiguredRemotes,
    SearchFailed,
    UnrecognizedOutput,
    PartialResults,
    ParsedResults,
    NoMatches,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Updated,
    UpToDate,
    Deferred,
    Completed,
    Available,
    Selected,
    AlreadyInstalled,
    NotApplicable,
    NotSelected,
    Unavailable,
    Protected,
    Busy,
    Blocked,
    Cancelled,
    Success,
    Failed,
    Skipped,
    DryRun,
}

impl OperationStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Updated => "Updated",
            Self::UpToDate => "Up to date",
            Self::Deferred => "Deferred",
            Self::Completed => "Completed",
            Self::Available => "Available",
            Self::Selected => "Selected",
            Self::AlreadyInstalled => "Already installed",
            Self::NotApplicable => "Not applicable",
            Self::NotSelected => "Not selected",
            Self::Unavailable => "Unavailable",
            Self::Protected => "Protected",
            Self::Busy => "Busy",
            Self::Blocked => "Blocked",
            Self::Cancelled => "Cancelled",
            Self::Success => "Success",
            Self::Failed => "Failed",
            Self::Skipped => "Skipped",
            Self::DryRun => "Dry run",
        }
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed | Self::Busy | Self::Blocked)
    }

    pub fn is_optional_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }

    pub fn counts_as_executed(&self) -> bool {
        matches!(
            self,
            Self::Updated | Self::Completed | Self::Success | Self::Failed
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendOperationRecord {
    pub backend_id: String,
    pub backend_name: String,
    pub action: Option<String>,
    pub command: Option<String>,
    pub status: OperationStatus,
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privilege_status: Option<PrivilegeStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultiOperationReport {
    pub operation: String,
    pub records: Vec<BackendOperationRecord>,
}

impl MultiOperationReport {
    pub fn has_failures(&self) -> bool {
        self.records.iter().any(|record| record.status.is_failure())
    }
}

#[derive(Debug, Clone, Default)]
pub struct MaintenancePlan {
    pub plans: Vec<crate::domain::ExecutionPlan>,
    pub records: Vec<BackendOperationRecord>,
}

impl MaintenancePlan {
    pub fn from_plans(plans: Vec<crate::domain::ExecutionPlan>) -> Self {
        Self {
            plans,
            records: Vec::new(),
        }
    }

    pub fn skipped(
        backend_id: impl Into<String>,
        backend_name: impl Into<String>,
        message: impl Into<String>,
    ) -> BackendOperationRecord {
        Self::record(backend_id, backend_name, OperationStatus::Skipped, message)
    }

    pub fn record(
        backend_id: impl Into<String>,
        backend_name: impl Into<String>,
        status: OperationStatus,
        message: impl Into<String>,
    ) -> BackendOperationRecord {
        let message = message.into();
        BackendOperationRecord {
            backend_id: backend_id.into(),
            backend_name: backend_name.into(),
            action: None,
            command: None,
            status,
            message: (!message.is_empty()).then_some(message),
            privilege_status: None,
        }
    }
}
