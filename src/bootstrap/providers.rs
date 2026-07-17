use crate::{
    capabilities::CapabilityRegistry,
    domain::{ExecutionPlan, NativeCommand, OperationKind, PrivilegeRequirement},
    platform::{DistributionFamily, PlatformContext},
    requirements::{Requirement, RequirementMutation},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapActionKind {
    InstallMissingExecutable,
    EnableService,
    AddRemote,
    ChangeSystemConfiguration,
}

pub trait BootstrapProvider {
    fn id(&self) -> &'static str;
    fn supports(&self, requirement: &Requirement, context: &PlatformContext) -> bool;
    fn plan(
        &self,
        requirement: &Requirement,
        context: &PlatformContext,
        capabilities: &CapabilityRegistry,
    ) -> Result<BootstrapPlan, String>;
}

#[derive(Debug, Clone)]
pub struct BootstrapPlan {
    pub action_kind: BootstrapActionKind,
    pub requirement_id: String,
    pub execution: ExecutionPlan,
}

#[derive(Debug, Clone, Copy)]
pub struct SystemPackageBootstrapProvider {
    id: &'static str,
    family: DistributionFamily,
    executable: &'static str,
    install_args: &'static [&'static str],
}

const PROVIDERS: &[SystemPackageBootstrapProvider] = &[
    SystemPackageBootstrapProvider::new("apt", DistributionFamily::Debian, "apt-get", &["install"]),
    SystemPackageBootstrapProvider::new("dnf", DistributionFamily::RedHat, "dnf", &["install"]),
    SystemPackageBootstrapProvider::new("pacman", DistributionFamily::Arch, "pacman", &["-S"]),
    SystemPackageBootstrapProvider::new("zypper", DistributionFamily::Suse, "zypper", &["install"]),
    SystemPackageBootstrapProvider::new("apk", DistributionFamily::Alpine, "apk", &["add"]),
];

impl SystemPackageBootstrapProvider {
    pub const fn new(
        id: &'static str,
        family: DistributionFamily,
        executable: &'static str,
        install_args: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            family,
            executable,
            install_args,
        }
    }
}

impl BootstrapProvider for SystemPackageBootstrapProvider {
    fn id(&self) -> &'static str {
        self.id
    }

    fn supports(&self, requirement: &Requirement, context: &PlatformContext) -> bool {
        context.distribution_family == Some(self.family)
            && requirement.mutating_action == Some(RequirementMutation::InstallExecutable)
            && package_for(self.id, &requirement.id).is_some()
    }

    fn plan(
        &self,
        requirement: &Requirement,
        context: &PlatformContext,
        capabilities: &CapabilityRegistry,
    ) -> Result<BootstrapPlan, String> {
        if !self.supports(requirement, context) {
            return Err(format!(
                "{} cannot safely bootstrap {} on this platform",
                self.id, requirement.id
            ));
        }
        let program = capabilities
            .resolved_executable(self.executable)
            .ok_or_else(|| format!("{} executable is unavailable", self.executable))?;
        let package =
            package_for(self.id, &requirement.id).expect("support checked package mapping");
        let command = NativeCommand::new(program)
            .args(self.install_args.iter().copied())
            .arg(package);
        Ok(BootstrapPlan {
            action_kind: BootstrapActionKind::InstallMissingExecutable,
            requirement_id: requirement.id.clone(),
            execution: ExecutionPlan {
                backend_id: format!("bootstrap-{}", self.id),
                backend_name: format!("{} prerequisite provider", self.id.to_ascii_uppercase()),
                operation: OperationKind::Bootstrap,
                action: format!("Install required component {}", requirement.id),
                package_id: Some(package.to_owned()),
                source: Some(self.id.to_owned()),
                scope: Some("system".to_owned()),
                details: vec![
                    ("Requirement".to_owned(), requirement.description.clone()),
                    (
                        "Mutation".to_owned(),
                        "Install missing executable".to_owned(),
                    ),
                ],
                command,
                privilege: PrivilegeRequirement::RootRequired,
                requires_root: true,
                interactive: true,
            },
        })
    }
}

pub fn select_provider<'a>(
    requirement: &Requirement,
    context: &PlatformContext,
    capabilities: &CapabilityRegistry,
) -> Option<&'a SystemPackageBootstrapProvider> {
    PROVIDERS.iter().find(|provider| {
        provider.supports(requirement, context)
            && capabilities
                .resolved_executable(provider.executable)
                .is_some()
    })
}

fn package_for(provider: &str, requirement: &str) -> Option<&'static str> {
    match (provider, requirement) {
        ("apt" | "dnf" | "pacman" | "zypper" | "apk", "flatpak") => Some("flatpak"),
        ("apt" | "dnf" | "zypper", "snap") => Some("snapd"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::RuntimePrivilegeContext,
        platform::PlatformContext,
        requirements::{Requirement, RequirementKind},
    };

    #[test]
    fn unsupported_provider_mapping_returns_manual_guidance_path() {
        assert_eq!(package_for("apk", "snap"), None);
    }

    #[test]
    fn flatpak_mapping_exists_for_initial_linux_providers() {
        for provider in ["apt", "dnf", "pacman", "zypper", "apk"] {
            assert_eq!(package_for(provider, "flatpak"), Some("flatpak"));
        }
    }

    #[test]
    fn requirement_mutation_must_be_install_executable() {
        let mut context = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        context.distribution_family = Some(DistributionFamily::Debian);
        let requirement = Requirement {
            id: "flatpak".to_owned(),
            kind: RequirementKind::Remote,
            description: "remote".to_owned(),
            alternatives: Vec::new(),
            mutating_action: Some(RequirementMutation::AddRemote),
        };
        assert!(!PROVIDERS[0].supports(&requirement, &context));
    }
}
