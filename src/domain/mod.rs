pub mod capability;
pub mod category;
pub mod error;
pub mod execution;
pub mod package;
pub mod report;
pub mod software_identity;

pub use capability::Capability;
pub use category::BackendCategory;
pub use error::{AllpError, AllpExitCode, AllpResult};
pub use execution::{
    ExecutionPlan, NativeCommand, OperationKind, OriginalUser, PrivilegeRequirement,
    RuntimePrivilegeContext,
};
pub use package::{
    DeveloperTarget, InstalledPackage, MatchKind, PackageCandidate, PackageDomain, PackageInfo,
    ResultSection, SearchScope,
};
pub use report::{
    BackendIssue, BackendOperationRecord, MaintenancePlan, MultiOperationReport, OperationStatus,
    SearchBackendState, SearchBackendSummary, SearchReport,
};
pub use software_identity::{
    DistributionRelationship, IdentityConfidence, IdentityMetadata, NameMatchKind, SoftwareType,
};
