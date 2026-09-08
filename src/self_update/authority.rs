use crate::{
    discovery::path::find_executable, domain::NativeCommand, execution::ProcessRunner,
    platform::PlatformContext,
};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateAuthority {
    AllpSelfUpdate,
    NativePackageManager { manager: String, package_id: String },
}

impl UpdateAuthority {
    pub fn native_manager(&self) -> Option<(&str, &str)> {
        match self {
            Self::NativePackageManager {
                manager,
                package_id,
            } => Some((manager, package_id)),
            Self::AllpSelfUpdate => None,
        }
    }
}

pub fn detect_update_authority(
    platform: &PlatformContext,
    runner: &dyn ProcessRunner,
) -> UpdateAuthority {
    if !matches!(platform.os, crate::platform::OperatingSystem::Linux) {
        return UpdateAuthority::AllpSelfUpdate;
    }

    let commands = ownership_commands(platform);
    detect_with_commands(&platform.current_executable, &commands, runner)
}

fn ownership_commands(platform: &PlatformContext) -> Vec<(&'static str, PathBuf)> {
    let mut commands = Vec::new();
    for (manager, executable) in [
        ("apt/dpkg", "dpkg-query"),
        ("dnf/rpm", "rpm"),
        ("pacman", "pacman"),
    ] {
        if let Some(path) = trusted_manager_command(executable, platform.is_root) {
            commands.push((manager, path));
        }
    }
    commands
}

fn trusted_manager_command(name: &str, running_as_root: bool) -> Option<PathBuf> {
    if running_as_root {
        super::trusted_helper::resolve_self_update_helper(name).ok()
    } else {
        find_executable(name)
    }
}

fn detect_with_commands(
    executable: &Path,
    commands: &[(&str, PathBuf)],
    runner: &dyn ProcessRunner,
) -> UpdateAuthority {
    for (manager, program) in commands {
        let command = match *manager {
            "apt/dpkg" => NativeCommand::new(program).arg("--search").arg(executable),
            "dnf/rpm" => NativeCommand::new(program).arg("-qf").arg(executable),
            "pacman" => NativeCommand::new(program).arg("-Qo").arg(executable),
            _ => continue,
        };
        let Ok(output) = runner.capture(&command) else {
            continue;
        };
        if !output.success {
            continue;
        }
        if let Some(package_id) = parse_owned_package(manager, &output.stdout) {
            return UpdateAuthority::NativePackageManager {
                manager: (*manager).to_owned(),
                package_id,
            };
        }
    }
    UpdateAuthority::AllpSelfUpdate
}

fn parse_owned_package(manager: &str, stdout: &str) -> Option<String> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?.trim();
    let package_id = match manager {
        "apt/dpkg" => line.split_once(':')?.0.trim(),
        "dnf/rpm" => line.split_whitespace().next()?,
        "pacman" => line
            .split_once(" is owned by ")?
            .1
            .split_whitespace()
            .next()?,
        _ => return None,
    };
    (!package_id.is_empty()).then(|| package_id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{AllpResult, ExecutionPlan},
        execution::{CommandOutput, ProcessStatus},
    };
    use std::{collections::BTreeMap, time::Duration};

    struct FakeRunner {
        outputs: BTreeMap<String, (bool, String)>,
    }

    impl ProcessRunner for FakeRunner {
        fn capture(&self, command: &NativeCommand) -> AllpResult<CommandOutput> {
            let name = command
                .program
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let (success, stdout) = self
                .outputs
                .get(name)
                .cloned()
                .unwrap_or((false, String::new()));
            Ok(CommandOutput {
                success,
                code: Some(if success { 0 } else { 1 }),
                signal: None,
                duration: Duration::ZERO,
                stdout,
                stderr: String::new(),
            })
        }

        fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
            unreachable!("ownership detection is read-only")
        }
    }

    #[test]
    fn package_owner_prevents_allp_from_claiming_update_authority() {
        let runner = FakeRunner {
            outputs: BTreeMap::from([(
                "dpkg-query".to_owned(),
                (true, "allp: /usr/bin/allp\n".to_owned()),
            )]),
        };
        let commands = vec![("apt/dpkg", PathBuf::from("/usr/bin/dpkg-query"))];

        assert_eq!(
            detect_with_commands(Path::new("/usr/bin/allp"), &commands, &runner),
            UpdateAuthority::NativePackageManager {
                manager: "apt/dpkg".to_owned(),
                package_id: "allp".to_owned(),
            }
        );
    }

    #[test]
    fn unowned_binary_remains_self_managed() {
        let runner = FakeRunner {
            outputs: BTreeMap::new(),
        };
        let commands = vec![("dnf/rpm", PathBuf::from("/usr/bin/rpm"))];

        assert_eq!(
            detect_with_commands(Path::new("/home/user/.local/bin/allp"), &commands, &runner),
            UpdateAuthority::AllpSelfUpdate
        );
    }
}
