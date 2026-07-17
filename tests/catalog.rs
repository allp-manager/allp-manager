use allp::{
    backends::{
        builtin_backends, system::apt::AptBackend, universal::flatpak::FlatpakBackend, Backend,
        CommandMap,
    },
    domain::{
        AllpResult, BackendCategory, ExecutionPlan, MatchKind, NativeCommand, OriginalUser,
        PackageCandidate, PackageDomain, PrivilegeRequirement, RuntimePrivilegeContext,
    },
    execution::{render_execution_plan_with_context, CommandOutput, ProcessRunner, ProcessStatus},
};
use std::{collections::BTreeSet, path::PathBuf};

struct NoopRunner;

impl ProcessRunner for NoopRunner {
    fn capture(&self, _command: &NativeCommand) -> AllpResult<CommandOutput> {
        unreachable!("plan construction should not capture output")
    }

    fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
        unreachable!("plan construction should not execute")
    }
}

#[test]
fn builtin_backend_ids_are_unique() {
    let backends = builtin_backends();
    let mut ids = BTreeSet::new();
    for backend in backends {
        assert!(
            ids.insert(backend.id()),
            "duplicate backend id: {}",
            backend.id()
        );
        assert!(
            !backend.capabilities().is_empty(),
            "{} has no capabilities",
            backend.id()
        );
        assert!(
            !backend.command_requirements().is_empty()
                || !backend.optional_command_requirements().is_empty(),
            "{} has no required or optional command requirements",
            backend.id()
        );
    }
}

#[test]
fn flatpak_user_update_deescalates_to_original_sudo_user() {
    let backend = FlatpakBackend;
    let mut commands = CommandMap::new();
    commands.insert("flatpak".to_owned(), PathBuf::from("/usr/bin/flatpak"));

    let plans = backend
        .plan_update(&commands, &NoopRunner, None, None)
        .expect("Flatpak update plan should be constructed")
        .plans;
    let plan = plans.first().expect("Flatpak should have an update plan");
    let context = RuntimePrivilegeContext::SudoRootWithOriginalUser(OriginalUser {
        name: "alice".to_owned(),
        uid: Some(1000),
        gid: Some(1000),
    });

    assert_eq!(plan.privilege, PrivilegeRequirement::OriginalUserRequired);
    assert_eq!(plan.scope.as_deref(), Some("User"));
    assert_eq!(
        render_execution_plan_with_context(plan, &context),
        "sudo -u alice -- /usr/bin/flatpak update --user"
    );
}

#[test]
fn backend_plan_construction_includes_action_and_argument_boundary() {
    let backend = AptBackend;
    let mut commands = CommandMap::new();
    commands.insert("apt-get".to_owned(), PathBuf::from("/usr/bin/apt-get"));

    let candidate = PackageCandidate {
        backend_id: "apt".to_owned(),
        backend_name: "APT".to_owned(),
        category: BackendCategory::System,
        domain: PackageDomain::System,
        package_id: "git".to_owned(),
        display_name: "git".to_owned(),
        version: None,
        description: None,
        source: Some("APT repositories".to_owned()),
        installers: vec!["APT".to_owned()],
        artifact_kind: "system package".to_owned(),
        scope: Some("system".to_owned()),
        match_kind: MatchKind::Exact,
        identity: PackageCandidate::infer_identity(
            MatchKind::Exact,
            PackageDomain::System,
            "system package",
        ),
        metadata: Default::default(),
    };

    let plan = backend
        .plan_install(&commands, &candidate)
        .expect("APT install plan should be constructed");

    assert_eq!(plan.action, "Install system package");
    assert_eq!(plan.package_id.as_deref(), Some("git"));
    assert!(plan.command.args.iter().any(|arg| arg == "--"));
    assert!(plan.requires_root);
    assert_eq!(plan.privilege, PrivilegeRequirement::RootRequired);
}
