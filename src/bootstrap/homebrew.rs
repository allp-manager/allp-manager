use crate::{
    domain::{
        BackendCategory, DistributionRelationship, ExecutionPlan, IdentityMetadata, MatchKind,
        NativeCommand, OperationKind, PackageCandidate, PackageDomain, PrivilegeRequirement,
        SearchScope, SoftwareType,
    },
    identity::{
        catalog::{self, HOMEBREW_ID},
        resolver,
    },
};

const BOOTSTRAP_ID: &str = "homebrew-bootstrap";
const INSTALLER_URL: &str = "https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh";

pub fn matches_query(query: &str) -> bool {
    resolver::resolve_query(query).is_some_and(|identity| identity.id == HOMEBREW_ID)
}

pub fn matches_scope(scope: SearchScope) -> bool {
    scope.matches_backend_domains(&[PackageDomain::Homebrew])
}

pub fn matches_filter(filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    let value = filter.to_ascii_lowercase();
    matches!(
        value.as_str(),
        "official" | "bootstrap" | "homebrew-bootstrap" | "homebrew" | "brew"
    )
}

pub fn is_candidate(candidate: &PackageCandidate) -> bool {
    candidate.backend_id == BOOTSTRAP_ID
}

pub fn candidate() -> PackageCandidate {
    let identity = catalog::find_by_id(HOMEBREW_ID).expect("Homebrew identity should exist");
    PackageCandidate {
        backend_id: BOOTSTRAP_ID.to_owned(),
        backend_name: "Homebrew official installer".to_owned(),
        category: BackendCategory::Development,
        domain: PackageDomain::Homebrew,
        package_id: "homebrew".to_owned(),
        display_name: "Homebrew".to_owned(),
        version: None,
        description: Some("Official Homebrew package manager bootstrap installer".to_owned()),
        source: Some(INSTALLER_URL.to_owned()),
        installers: vec!["official-bootstrap".to_owned()],
        artifact_kind: "official installer".to_owned(),
        scope: Some("current user".to_owned()),
        match_kind: MatchKind::Exact,
        identity: IdentityMetadata::official(
            identity.id,
            identity.display_name,
            SoftwareType::PackageManager,
            DistributionRelationship::OfficialInstaller,
        ),
        metadata: Default::default(),
    }
}

pub fn plan_install() -> crate::domain::AllpResult<ExecutionPlan> {
    let script = installer_script();
    Ok(ExecutionPlan {
        backend_id: BOOTSTRAP_ID.to_owned(),
        backend_name: "Homebrew official installer".to_owned(),
        operation: OperationKind::Bootstrap,
        action: "Bootstrap Homebrew with the official installer".to_owned(),
        package_id: Some("homebrew".to_owned()),
        source: Some(INSTALLER_URL.to_owned()),
        scope: Some("current user; installer may request sudo for its own setup".to_owned()),
        details: Vec::new(),
        command: NativeCommand::new("/bin/bash").arg("-c").arg(script),
        privilege: PrivilegeRequirement::OriginalUserRequired,
        requires_root: false,
        interactive: true,
    })
}

fn installer_script() -> String {
    format!(
        "set -eu\n\
         umask 077\n\
         installer=\"$(mktemp \"${{TMPDIR:-/tmp}}/allp-homebrew-install.XXXXXX\")\"\n\
         cleanup() {{ rm -f \"$installer\"; }}\n\
         trap cleanup EXIT\n\
         if command -v curl >/dev/null 2>&1; then\n\
           curl -fsSL {url} -o \"$installer\"\n\
         elif command -v wget >/dev/null 2>&1; then\n\
           wget -qO \"$installer\" {url}\n\
         else\n\
           printf '%s\\n' 'curl or wget is required to download the official Homebrew installer' >&2\n\
           exit 127\n\
         fi\n\
         /bin/bash \"$installer\"",
        url = shell_quote(INSTALLER_URL),
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::render_native_command;

    #[test]
    fn homebrew_bootstrap_plan_does_not_use_curl_pipe_bash() {
        let plan = plan_install().expect("plan should be built");
        let rendered = render_native_command(&plan.command);

        assert!(rendered.contains("curl -fsSL"));
        assert!(rendered.contains("/bin/bash"));
        assert!(!rendered.contains("| bash"));
        assert!(!rendered.contains("| /bin/bash"));
        assert_eq!(plan.privilege, PrivilegeRequirement::OriginalUserRequired);
    }
}
