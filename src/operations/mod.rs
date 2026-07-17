pub mod detect;
pub mod info;
pub mod install;
pub mod list;
mod maintenance;
pub mod remove;
pub mod search;
pub mod update;
pub mod upgrade;

use crate::{
    cli::Renderer,
    discovery::{DetectedBackend, DetectedBackendSet, DiscoveryReport},
    domain::{AllpError, AllpResult, DeveloperTarget, RuntimePrivilegeContext, SearchScope},
    execution::ProcessRunner,
};

pub struct OperationContext<'a> {
    pub backends: &'a DetectedBackendSet,
    pub discovery: &'a DiscoveryReport,
    pub runner: &'a dyn ProcessRunner,
    pub renderer: &'a Renderer,
    pub privilege_context: &'a RuntimePrivilegeContext,
    pub dry_run: bool,
    pub no_interactive: bool,
    pub yes: bool,
    pub verbose: u8,
    pub backend_filter: Option<&'a str>,
    pub search_scope: Option<SearchScope>,
    pub target: Option<DeveloperTarget>,
    pub root_context_notice_shown: bool,
}

impl<'a> OperationContext<'a> {
    pub fn eligible_backends(&self) -> AllpResult<Vec<&DetectedBackend>> {
        if let Some(filter) = self.backend_filter {
            let backend = self
                .backends
                .get(filter)
                .ok_or_else(|| AllpError::BackendNotDetected(filter.to_owned()))?;
            if let Some(scope) = self.search_scope {
                if scope != SearchScope::AllSources
                    && !scope.matches_backend_domains(backend.backend.package_domains())
                {
                    return Err(AllpError::InvalidInput(format!(
                        "--from {filter} is outside --scope {}; use --scope all or remove one selector",
                        scope.cli_value()
                    )));
                }
            }
            return Ok(vec![backend]);
        }

        let mut backends = self.backends.iter().collect::<Vec<_>>();
        if let Some(scope) = self.search_scope {
            if scope != SearchScope::AllSources {
                backends.retain(|backend| {
                    scope.matches_backend_domains(backend.backend.package_domains())
                });
            }
        }
        Ok(backends)
    }

    pub fn backend(&self, backend_id: &str) -> AllpResult<&DetectedBackend> {
        self.backends
            .get(backend_id)
            .ok_or_else(|| AllpError::BackendNotDetected(backend_id.to_owned()))
    }
}

pub fn validate_package_id(package_id: &str) -> AllpResult<()> {
    if package_id.is_empty() {
        return Err(AllpError::InvalidInput(
            "package identifier cannot be empty".to_owned(),
        ));
    }
    if package_id.starts_with('-') {
        return Err(AllpError::InvalidInput(format!(
            "package identifier '{package_id}' begins with '-' and is unsafe to pass to a native manager"
        )));
    }
    if package_id.contains('\0') {
        return Err(AllpError::InvalidInput(
            "package identifier contains a NUL byte".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_package_id;

    #[test]
    fn rejects_package_ids_that_could_be_native_options() {
        let error = validate_package_id("-danger").expect_err("leading dash must be rejected");

        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn accepts_normal_package_ids() {
        validate_package_id("git-scm").expect("ordinary package ID should be accepted");
    }
}
