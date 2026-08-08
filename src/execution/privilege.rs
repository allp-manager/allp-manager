use crate::{
    discovery::path::find_executable,
    domain::{
        AllpError, AllpResult, NativeCommand, OriginalUser, PrivilegeRequirement,
        RuntimePrivilegeContext,
    },
};
use std::{env, fs, path::Path, process::Command};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn is_effective_root() -> bool {
    runtime_context().is_root()
}

pub fn runtime_context() -> RuntimePrivilegeContext {
    let effective_uid = effective_uid();

    if effective_uid != Some(0) {
        return RuntimePrivilegeContext::NormalUser;
    }

    if let Ok(name) = env::var("SUDO_USER") {
        if !name.is_empty() && name != "root" {
            return RuntimePrivilegeContext::SudoRootWithOriginalUser(OriginalUser {
                name,
                uid: env::var("SUDO_UID")
                    .ok()
                    .and_then(|value| value.parse().ok()),
                gid: env::var("SUDO_GID")
                    .ok()
                    .and_then(|value| value.parse().ok()),
            });
        }
    }

    RuntimePrivilegeContext::RootDirect
}

#[cfg(unix)]
fn effective_uid() -> Option<u32> {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        if let Some(uid) = status.lines().find_map(|line| {
            let values = line.strip_prefix("Uid:")?;
            values.split_whitespace().nth(1)?.parse::<u32>().ok()
        }) {
            return Some(uid);
        }
    }
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(not(unix))]
fn effective_uid() -> Option<u32> {
    None
}

pub fn prepare_command(
    command: &NativeCommand,
    privilege: PrivilegeRequirement,
) -> AllpResult<Command> {
    prepare_command_with_context(command, privilege, &runtime_context())
}

pub fn prepare_command_with_context(
    command: &NativeCommand,
    privilege: PrivilegeRequirement,
    context: &RuntimePrivilegeContext,
) -> AllpResult<Command> {
    if privilege == PrivilegeRequirement::RootRequired {
        validate_elevated_executable(&command.program)?;
    }

    let mut environment_is_command_argv = false;
    let mut process = if privilege.requires_sudo(context) {
        let sudo = find_executable("sudo").ok_or_else(|| {
            AllpError::BackendNotDetected("sudo is required but was not found".to_owned())
        })?;
        validate_elevated_executable(&sudo)?;
        let mut process = Command::new(sudo);
        process.arg("--").arg(&command.program);
        process
    } else if privilege.requires_original_user(context) {
        let Some(user) = context.original_user() else {
            return Err(AllpError::InvalidInput(
                "refusing to run a user-scoped operation as root without an original sudo user"
                    .to_owned(),
            ));
        };
        let sudo = find_executable("sudo").ok_or_else(|| {
            AllpError::BackendNotDetected(
                "sudo is required to return to the original user but was not found".to_owned(),
            )
        })?;
        validate_elevated_executable(&sudo)?;
        let mut process = Command::new(sudo);
        environment_is_command_argv = true;
        process
            .arg("-H")
            .arg("-u")
            .arg(&user.name)
            .arg("--")
            .arg("/usr/bin/env");
        if let Some(identity) = identity_for_user(&user.name) {
            let path = user_path(&identity.home);
            process
                .arg(format!("HOME={}", identity.home))
                .arg(format!("USER={}", user.name))
                .arg(format!("LOGNAME={}", user.name))
                .arg(format!("PATH={path}"))
                .arg(format!("SHELL={}", identity.shell))
                .arg(format!("XDG_CONFIG_HOME={}/.config", identity.home))
                .arg(format!("XDG_CACHE_HOME={}/.cache", identity.home))
                .arg(format!("XDG_DATA_HOME={}/.local/share", identity.home))
                .arg(format!("XDG_STATE_HOME={}/.local/state", identity.home));
            if let Some(uid) = user.uid {
                let runtime = format!("/run/user/{uid}");
                if Path::new(&runtime).is_dir() {
                    process.arg(format!("XDG_RUNTIME_DIR={runtime}"));
                }
            }
        }
        for (key, value) in &command.env {
            let mut assignment = key.clone();
            assignment.push("=");
            assignment.push(value);
            process.arg(assignment);
        }
        process.arg(&command.program);
        process
    } else if privilege == PrivilegeRequirement::OriginalUserRequired
        && matches!(context, RuntimePrivilegeContext::RootDirect)
    {
        return Err(AllpError::InvalidInput(
            "refusing to run a user-scoped operation as root without an original sudo user"
                .to_owned(),
        ));
    } else {
        Command::new(&command.program)
    };

    process.args(&command.args);
    if let Some(current_dir) = &command.current_dir {
        process.current_dir(current_dir);
    }
    if !environment_is_command_argv {
        for (key, value) in &command.env {
            process.env(key, value);
        }
    }

    Ok(process)
}

fn user_path(home: &str) -> String {
    let inherited = env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".to_owned());
    format!("{home}/.local/bin:{home}/bin:{inherited}")
}

struct UserEnvironment {
    home: String,
    shell: String,
}

fn identity_for_user(name: &str) -> Option<UserEnvironment> {
    #[cfg(not(unix))]
    {
        let _ = name;
        return env::var("USERPROFILE").ok().map(|home| UserEnvironment {
            home,
            shell: env::var("COMSPEC").unwrap_or_default(),
        });
    }
    #[cfg(unix)]
    {
        let passwd = fs::read_to_string("/etc/passwd").ok()?;
        passwd.lines().find_map(|line| {
            let mut fields = line.split(':');
            let username = fields.next()?;
            if username != name {
                return None;
            }
            let home = fields.nth(4)?;
            let shell = fields.next().unwrap_or("/bin/sh");
            (!home.is_empty()).then(|| UserEnvironment {
                home: home.to_owned(),
                shell: shell.to_owned(),
            })
        })
    }
}

fn validate_elevated_executable(path: &Path) -> AllpResult<()> {
    if !path.is_absolute() {
        return Err(AllpError::InvalidInput(format!(
            "refusing to elevate non-absolute executable path: {}",
            path.display()
        )));
    }

    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AllpError::InvalidInput(format!(
            "refusing to elevate non-file executable path: {}",
            path.display()
        )));
    }

    #[cfg(unix)]
    {
        let mode = metadata.permissions().mode();
        if mode & 0o022 != 0 {
            return Err(AllpError::InvalidInput(format!(
                "refusing to elevate group/world-writable executable path: {}",
                path.display()
            )));
        }
    }

    Ok(())
}
