use allp::{
    backends::{Backend, CommandMap, CommandRequirement},
    cli::Renderer,
    discovery::{DetectedBackend, DetectedBackendSet, DiscoveryReport},
    domain::{
        AllpResult, BackendCategory, Capability, ExecutionPlan, MatchKind, NativeCommand,
        PackageCandidate, PackageDomain, RuntimePrivilegeContext,
    },
    execution::{CommandOutput, ProcessRunner, ProcessStatus},
    operations::{search, OperationContext},
};
use std::sync::Arc;

struct ExampleBackend;
struct ListOnlyBackend;

const CAPABILITIES: &[Capability] = &[Capability::Search];
const LIST_ONLY_CAPABILITIES: &[Capability] = &[Capability::List];
const REQUIREMENTS: &[CommandRequirement] = &[];

impl Backend for ExampleBackend {
    fn id(&self) -> &'static str {
        "example"
    }
    fn display_name(&self) -> &'static str {
        "Example"
    }
    fn category(&self) -> BackendCategory {
        BackendCategory::Development
    }
    fn capabilities(&self) -> &'static [Capability] {
        CAPABILITIES
    }
    fn command_requirements(&self) -> &'static [CommandRequirement] {
        REQUIREMENTS
    }

    fn search(
        &self,
        _commands: &CommandMap,
        _runner: &dyn ProcessRunner,
        query: &str,
    ) -> AllpResult<Vec<PackageCandidate>> {
        Ok(vec![PackageCandidate {
            backend_id: self.id().to_owned(),
            backend_name: self.display_name().to_owned(),
            category: self.category(),
            domain: PackageDomain::Python,
            package_id: query.to_owned(),
            display_name: query.to_owned(),
            version: Some("1.0.0".to_owned()),
            description: Some("A test package".to_owned()),
            source: Some("example registry".to_owned()),
            installers: vec!["example".to_owned()],
            artifact_kind: "development package".to_owned(),
            scope: Some("global".to_owned()),
            match_kind: MatchKind::Exact,
            identity: PackageCandidate::infer_identity(
                MatchKind::Exact,
                PackageDomain::Python,
                "development package",
            ),
        }])
    }
}

impl Backend for ListOnlyBackend {
    fn id(&self) -> &'static str {
        "list-only"
    }
    fn display_name(&self) -> &'static str {
        "List Only"
    }
    fn category(&self) -> BackendCategory {
        BackendCategory::Development
    }
    fn capabilities(&self) -> &'static [Capability] {
        LIST_ONLY_CAPABILITIES
    }
    fn command_requirements(&self) -> &'static [CommandRequirement] {
        REQUIREMENTS
    }
}

struct NoopRunner;

impl ProcessRunner for NoopRunner {
    fn capture(&self, _command: &NativeCommand) -> AllpResult<CommandOutput> {
        unreachable!("the example backend does not invoke a native query")
    }

    fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
        unreachable!("the search test does not execute mutating commands")
    }
}

#[test]
fn generic_search_accepts_a_new_backend_without_operation_changes() {
    let backends = DetectedBackendSet::new(vec![
        DetectedBackend {
            backend: Arc::new(ExampleBackend),
            commands: CommandMap::new(),
        },
        DetectedBackend {
            backend: Arc::new(ListOnlyBackend),
            commands: CommandMap::new(),
        },
    ]);
    let runner = NoopRunner;
    let renderer = Renderer::new(true, false);
    let discovery = DiscoveryReport {
        entries: Vec::new(),
    };
    let privilege_context = RuntimePrivilegeContext::NormalUser;
    let context = OperationContext {
        backends: &backends,
        discovery: &discovery,
        runner: &runner,
        renderer: &renderer,
        privilege_context: &privilege_context,
        dry_run: false,
        no_interactive: true,
        yes: false,
        verbose: 0,
        backend_filter: None,
        search_scope: None,
        target: None,
        root_context_notice_shown: false,
    };

    let report = search::gather(&context, "demo").expect("search should succeed");
    assert!(report.complete);
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].backend_id, "example");
}
