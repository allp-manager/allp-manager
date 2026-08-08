use crate::domain::{
    AllpError, AllpResult, NativeCommand, OriginalUser, PrivilegeRequirement,
    RuntimePrivilegeContext,
};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

pub fn is_effective_root() -> bool {
    runtime_context().is_root()
}

pub fn runtime_context() -> RuntimePrivilegeContext {
    let effective_uid = effective_uid();

    if effective_uid != Some(0) {
        return RuntimePrivilegeContext::NormalUser;
    }

    if let Some(account) = validated_sudo_account() {
        return RuntimePrivilegeContext::SudoRootWithOriginalUser(OriginalUser {
            name: account.name,
            uid: Some(account.uid),
            gid: Some(account.gid),
        });
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
    prepare_command_with_context_for_effective_uid(
        command,
        privilege,
        context,
        effective_uid(),
        None,
    )
}

fn prepare_command_with_context_for_effective_uid(
    command: &NativeCommand,
    privilege: PrivilegeRequirement,
    context: &RuntimePrivilegeContext,
    current_uid: Option<u32>,
    sudo_override: Option<&Path>,
) -> AllpResult<Command> {
    let root_required_program = (privilege == PrivilegeRequirement::RootRequired)
        .then(|| resolve_root_required_executable(&command.program))
        .transpose()?;
    let program = root_required_program.as_deref().unwrap_or(&command.program);

    let mut process = if privilege.requires_sudo(context) {
        let sudo = sudo_override
            .map(Path::to_path_buf)
            .map_or_else(resolve_sudo, Ok)?;
        let mut process = Command::new(sudo);
        process.arg("--").arg(program);
        process
    } else if privilege.requires_original_user(context) {
        let Some(user) = context.original_user() else {
            return Err(AllpError::InvalidInput(
                "refusing to run a user-scoped operation as root without an original sudo user"
                    .to_owned(),
            ));
        };
        return prepare_original_user_command(command, user, current_uid, sudo_override);
    } else if privilege == PrivilegeRequirement::OriginalUserRequired
        && matches!(context, RuntimePrivilegeContext::RootDirect)
    {
        return Err(AllpError::InvalidInput(
            "refusing to run a user-scoped operation as root without an original sudo user"
                .to_owned(),
        ));
    } else {
        Command::new(program)
    };

    process.args(&command.args);
    if let Some(current_dir) = &command.current_dir {
        process.current_dir(current_dir);
    }
    for (key, value) in &command.env {
        process.env(key, value);
    }

    Ok(process)
}

fn user_path(home: &str) -> String {
    format!(
        "{home}/.local/bin:{home}/bin:/home/linuxbrew/.linuxbrew/bin:/opt/homebrew/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAccount {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: PathBuf,
    pub shell: PathBuf,
}

/// Prepares a native command for one exact, system-database-validated non-root account.
///
/// This is intentionally owner-specific: callers do not rely on ambient `SUDO_USER` after
/// selecting an executable owner. When Allp is elevated, the prepared process de-escalates
/// through `sudo -H -u` and reconstructs the target account's deterministic environment.
pub struct UserContextExecutor;

impl UserContextExecutor {
    pub fn prepare(command: &NativeCommand, requested: &UserAccount) -> AllpResult<Command> {
        Self::prepare_for_effective_uid(command, requested, effective_uid(), None)
    }

    fn prepare_for_effective_uid(
        command: &NativeCommand,
        requested: &UserAccount,
        current_uid: Option<u32>,
        sudo_override: Option<&Path>,
    ) -> AllpResult<Command> {
        let account = validate_user_account(requested)?;
        if account.uid == 0 {
            return Err(AllpError::InvalidInput(
                "refusing to execute a user-scoped command as root".to_owned(),
            ));
        }

        let deescalating = current_uid == Some(0);
        if !deescalating && current_uid != Some(account.uid) {
            return Err(AllpError::InvalidInput(format!(
                "refusing to execute as {} (uid {}) from unrelated uid {:?}",
                account.name, account.uid, current_uid
            )));
        }

        let environment = user_environment(&account);
        if deescalating {
            validate_elevated_executable(&command.program)?;
            let sudo = sudo_override
                .map(Path::to_path_buf)
                .map_or_else(resolve_sudo, Ok)?;
            return Ok(build_deescalated_user_command(
                command,
                &account,
                &sudo,
                &environment,
            ));
        }

        let mut process = Command::new(&command.program);
        process.env_clear();
        for (key, value) in &environment {
            process.env(key, value);
        }

        process.args(&command.args);
        if let Some(current_dir) = &command.current_dir {
            process.current_dir(current_dir);
        }
        for (key, value) in &command.env {
            process.env(key, value);
        }
        Ok(process)
    }
}

fn build_deescalated_user_command(
    command: &NativeCommand,
    account: &UserAccount,
    sudo: &Path,
    environment: &[(String, String)],
) -> Command {
    let mut process = Command::new(sudo);
    process
        .arg("-H")
        .arg("-u")
        .arg(&account.name)
        .arg("--")
        .arg("/usr/bin/env")
        .arg("-i");
    for (key, value) in environment {
        process.arg(format!("{key}={value}"));
    }
    for (key, value) in &command.env {
        let mut assignment = key.clone();
        assignment.push("=");
        assignment.push(value);
        process.arg(assignment);
    }
    process.arg(&command.program).args(&command.args);
    if let Some(current_dir) = &command.current_dir {
        process.current_dir(current_dir);
    }
    process
}

fn prepare_original_user_command(
    command: &NativeCommand,
    user: &OriginalUser,
    current_uid: Option<u32>,
    sudo_override: Option<&Path>,
) -> AllpResult<Command> {
    let account = validate_original_user_account(user)?;
    UserContextExecutor::prepare_for_effective_uid(command, &account, current_uid, sudo_override)
}

fn validate_original_user_account(user: &OriginalUser) -> AllpResult<UserAccount> {
    let account = user_account_by_name(&user.name).ok_or_else(|| {
        AllpError::InvalidInput(format!(
            "original sudo user {} is not present in the system account database",
            user.name
        ))
    })?;
    if user.uid != Some(account.uid) || user.gid != Some(account.gid) {
        return Err(AllpError::InvalidInput(format!(
            "original sudo user {} no longer matches validated uid/gid {}:{}",
            user.name, account.uid, account.gid
        )));
    }
    Ok(account)
}

pub fn user_account_by_name(name: &str) -> Option<UserAccount> {
    #[cfg(not(unix))]
    {
        let _ = name;
        return None;
    }
    #[cfg(unix)]
    {
        system_accounts()
            .into_iter()
            .find(|account| account.name == name)
    }
}

pub fn user_account_by_uid(uid: u32) -> Option<UserAccount> {
    #[cfg(not(unix))]
    {
        let _ = uid;
        None
    }
    #[cfg(unix)]
    {
        system_accounts()
            .into_iter()
            .find(|account| account.uid == uid)
    }
}

pub fn user_group_ids(account: &UserAccount) -> BTreeSet<u32> {
    let mut groups = BTreeSet::from([account.gid]);
    #[cfg(unix)]
    if let Ok(contents) = fs::read_to_string("/etc/group") {
        groups.extend(parse_supplementary_groups(&contents, &account.name));
    }
    groups
}

fn validate_user_account(requested: &UserAccount) -> AllpResult<UserAccount> {
    let account = user_account_by_name(&requested.name).ok_or_else(|| {
        AllpError::InvalidInput(format!(
            "user {} is not present in the system account database",
            requested.name
        ))
    })?;
    if account.uid != requested.uid || account.gid != requested.gid {
        return Err(AllpError::InvalidInput(format!(
            "user {} no longer matches validated uid/gid {}:{}",
            requested.name, requested.uid, requested.gid
        )));
    }
    Ok(account)
}

fn user_environment(account: &UserAccount) -> Vec<(String, String)> {
    let home = account.home.to_string_lossy();
    let shell = account.shell.to_string_lossy();
    let mut environment = vec![
        ("LC_ALL".to_owned(), "C".to_owned()),
        ("LANG".to_owned(), "C".to_owned()),
        ("HOME".to_owned(), home.to_string()),
        ("USER".to_owned(), account.name.clone()),
        ("LOGNAME".to_owned(), account.name.clone()),
        ("PATH".to_owned(), user_path(&home)),
        ("SHELL".to_owned(), shell.to_string()),
        ("XDG_CONFIG_HOME".to_owned(), format!("{home}/.config")),
        ("XDG_CACHE_HOME".to_owned(), format!("{home}/.cache")),
        ("XDG_DATA_HOME".to_owned(), format!("{home}/.local/share")),
        ("XDG_STATE_HOME".to_owned(), format!("{home}/.local/state")),
    ];
    let runtime = format!("/run/user/{}", account.uid);
    if Path::new(&runtime).is_dir() {
        environment.push(("XDG_RUNTIME_DIR".to_owned(), runtime));
    }
    environment
}

fn validated_sudo_account() -> Option<UserAccount> {
    let name = env::var("SUDO_USER").ok()?;
    if name.is_empty() || name == "root" {
        return None;
    }
    let account = user_account_by_name(&name)?;
    if !env_identity_field_matches("SUDO_UID", account.uid)
        || !env_identity_field_matches("SUDO_GID", account.gid)
    {
        return None;
    }
    Some(account)
}

fn env_identity_field_matches(key: &str, expected: u32) -> bool {
    match env::var(key) {
        Ok(value) => value.parse::<u32>() == Ok(expected),
        Err(env::VarError::NotPresent | env::VarError::NotUnicode(_)) => false,
    }
}

#[cfg(unix)]
fn system_accounts() -> Vec<UserAccount> {
    fs::read_to_string("/etc/passwd")
        .ok()
        .map(|contents| parse_passwd(&contents))
        .unwrap_or_default()
}

#[cfg(unix)]
fn parse_passwd(contents: &str) -> Vec<UserAccount> {
    contents
        .lines()
        .filter_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            if fields.len() < 7 || fields[0].is_empty() || fields[5].is_empty() {
                return None;
            }
            Some(UserAccount {
                name: fields[0].to_owned(),
                uid: fields[2].parse().ok()?,
                gid: fields[3].parse().ok()?,
                home: PathBuf::from(fields[5]),
                shell: PathBuf::from(if fields[6].is_empty() {
                    "/bin/sh"
                } else {
                    fields[6]
                }),
            })
        })
        .collect()
}

#[cfg(unix)]
fn parse_supplementary_groups(contents: &str, user: &str) -> BTreeSet<u32> {
    contents
        .lines()
        .filter_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            if fields.len() < 4 || !fields[3].split(',').any(|member| member == user) {
                return None;
            }
            fields[2].parse().ok()
        })
        .collect()
}

fn resolve_sudo() -> AllpResult<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(path) = env::var_os("ALLP_TEST_SUDO_EXECUTABLE").map(PathBuf::from) {
        validate_elevated_executable(&path)?;
        return Ok(path);
    }
    resolve_trusted_root_helper("sudo")
}

fn resolve_root_required_executable(path: &Path) -> AllpResult<PathBuf> {
    // Integration tests exercise the full sudo command construction with isolated fake
    // executables. This escape hatch is absent from release builds and is coupled to the
    // already-explicit fake-sudo test boundary.
    #[cfg(debug_assertions)]
    if env::var_os("ALLP_TEST_SUDO_EXECUTABLE").is_some() {
        validate_elevated_executable(path)?;
        return fs::canonicalize(path).map_err(Into::into);
    }

    validate_trusted_root_executable(path, "root-required executable")
}

fn resolve_trusted_root_helper(name: &str) -> AllpResult<PathBuf> {
    let candidates = trusted_root_helper_candidates(name, env::var_os("PATH").as_deref());

    let mut seen = BTreeSet::new();
    let mut rejected = Vec::new();
    for candidate in candidates {
        let Ok(resolved) = fs::canonicalize(&candidate) else {
            continue;
        };
        if !seen.insert(resolved.clone()) {
            continue;
        }
        match validate_trusted_root_helper(&resolved) {
            Ok(()) => return Ok(resolved),
            Err(error) => rejected.push(format!("{}: {error}", candidate.display())),
        }
    }
    let detail = if rejected.is_empty() {
        "no fixed or PATH candidate exists".to_owned()
    } else {
        format!("rejected candidate(s): {}", rejected.join("; "))
    };
    Err(AllpError::BackendNotDetected(format!(
        "trusted root-owned {name} helper was not found; {detail}"
    )))
}

fn trusted_root_helper_candidates(name: &str, path: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut candidates = vec![
        PathBuf::from("/usr/bin").join(name),
        PathBuf::from("/bin").join(name),
    ];
    candidates.extend(
        path.into_iter()
            .flat_map(env::split_paths)
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(name)),
    );
    candidates
}

fn validate_trusted_root_helper(path: &Path) -> AllpResult<()> {
    validate_trusted_root_executable(path, "trusted helper").map(|_| ())
}

fn validate_trusted_root_executable(path: &Path, subject: &str) -> AllpResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AllpError::InvalidInput(format!(
            "{subject} path is not absolute: {}",
            path.display()
        )));
    }
    let resolved = fs::canonicalize(path)?;
    let metadata = fs::metadata(&resolved)?;
    if !metadata.is_file() {
        return Err(AllpError::InvalidInput(format!(
            "{subject} is not a regular file: {}",
            resolved.display()
        )));
    }

    #[cfg(unix)]
    {
        if metadata.uid() != 0 {
            return Err(AllpError::InvalidInput(format!(
                "{subject} is not owned by root: {}",
                resolved.display()
            )));
        }
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(AllpError::InvalidInput(format!(
                "{subject} is group/world-writable: {}",
                resolved.display()
            )));
        }
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(AllpError::InvalidInput(format!(
                "{subject} is not executable: {}",
                resolved.display()
            )));
        }
        let mut ancestor = resolved.parent();
        while let Some(directory) = ancestor {
            let metadata = fs::metadata(directory)?;
            if !metadata.is_dir()
                || metadata.uid() != 0
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(AllpError::InvalidInput(format!(
                    "{subject} ancestor is not a root-owned, non-writable directory: {}",
                    directory.display()
                )));
            }
            ancestor = directory.parent();
        }
    }

    Ok(resolved)
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

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    fn non_root_account() -> super::UserAccount {
        super::system_accounts()
            .into_iter()
            .find(|account| account.uid != 0)
            .expect("at least one non-root system account is required")
    }

    #[cfg(unix)]
    #[test]
    fn passwd_parser_preserves_validated_user_environment() {
        let accounts = super::parse_passwd(
            "root:x:0:0:root:/root:/bin/bash\ntestuser:x:1000:1001:Test:/home/testuser:/bin/zsh\n",
        );
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[1].name, "testuser");
        assert_eq!(accounts[1].uid, 1000);
        assert_eq!(accounts[1].gid, 1001);
        assert_eq!(accounts[1].home, std::path::Path::new("/home/testuser"));
        assert_eq!(accounts[1].shell, std::path::Path::new("/bin/zsh"));
    }

    #[cfg(unix)]
    #[test]
    fn supplementary_group_parser_matches_exact_members() {
        let groups = super::parse_supplementary_groups(
            "wheel:x:10:alice,bob\nusers:x:100:malice\ndev:x:200:alice\n",
            "alice",
        );
        assert_eq!(groups, std::collections::BTreeSet::from([10, 200]));
    }

    #[cfg(unix)]
    #[test]
    fn user_context_executor_rejects_root_target() {
        let root = super::user_account_by_uid(0).expect("root account should exist");
        let error = super::UserContextExecutor::prepare(
            &crate::domain::NativeCommand::new("/bin/true"),
            &root,
        )
        .expect_err("root target must be rejected");
        assert!(error.to_string().contains("as root"));
    }

    #[cfg(unix)]
    #[test]
    fn user_context_executor_revalidates_account_and_sets_user_environment() {
        let uid = super::effective_uid().expect("effective uid");
        if uid == 0 {
            return;
        }
        let account = super::user_account_by_uid(uid).expect("current account");
        let command = super::UserContextExecutor::prepare(
            &crate::domain::NativeCommand::new("/bin/true").env("HOMEBREW_NO_AUTO_UPDATE", "1"),
            &account,
        )
        .expect("current account context");
        assert_eq!(command.get_program(), "/bin/true");
        let environment = command
            .get_envs()
            .filter_map(|(key, value)| Some((key.to_str()?, value?.to_str()?)))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(environment.get("HOME").copied(), account.home.to_str());
        assert_eq!(environment.get("LC_ALL").copied(), Some("C"));
        assert_eq!(environment.get("LANG").copied(), Some("C"));
        assert!(!environment.contains_key("BASH_ENV"));
        assert!(!environment.contains_key("RUBYOPT"));
        assert!(!environment.contains_key("HOMEBREW_PREFIX"));
        assert_eq!(
            environment.get("HOMEBREW_NO_AUTO_UPDATE").copied(),
            Some("1")
        );
        assert_eq!(
            environment.get("USER").copied(),
            Some(account.name.as_str())
        );

        let mut stale = account;
        stale.gid = stale.gid.saturating_add(1);
        let error = super::UserContextExecutor::prepare(
            &crate::domain::NativeCommand::new("/bin/true"),
            &stale,
        )
        .expect_err("changed account identity must be rejected");
        assert!(error.to_string().contains("no longer matches"));
    }

    #[cfg(unix)]
    #[test]
    fn elevated_generic_original_user_command_uses_sanitized_validated_context() {
        let account = non_root_account();
        let original = crate::domain::OriginalUser {
            name: account.name.clone(),
            uid: Some(account.uid),
            gid: Some(account.gid),
        };
        let context = crate::domain::RuntimePrivilegeContext::SudoRootWithOriginalUser(original);
        let native =
            crate::domain::NativeCommand::new("/bin/true").env("HOMEBREW_NO_AUTO_UPDATE", "1");

        let command = super::prepare_command_with_context_for_effective_uid(
            &native,
            crate::domain::PrivilegeRequirement::OriginalUserRequired,
            &context,
            Some(0),
            Some(std::path::Path::new("/usr/bin/sudo")),
        )
        .expect("validated original-user command");

        assert_eq!(command.get_program(), "/usr/bin/sudo");
        let arguments = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            &arguments[..6],
            [
                "-H",
                "-u",
                account.name.as_str(),
                "--",
                "/usr/bin/env",
                "-i"
            ]
        );
        assert!(arguments.iter().any(|value| value == "LC_ALL=C"));
        assert!(arguments.iter().any(|value| value == "LANG=C"));
        assert!(arguments
            .iter()
            .any(|value| value == "HOMEBREW_NO_AUTO_UPDATE=1"));
        assert!(!arguments.iter().any(|value| value.starts_with("BASH_ENV=")));
        assert!(!arguments.iter().any(|value| value.starts_with("RUBYOPT=")));
        assert!(!arguments
            .iter()
            .any(|value| value.starts_with("HOMEBREW_PREFIX=")));
        assert_eq!(arguments.last().map(String::as_str), Some("/bin/true"));
    }

    #[cfg(unix)]
    #[test]
    fn elevated_generic_original_user_command_rejects_changed_account_tuple() {
        let account = non_root_account();
        let context = crate::domain::RuntimePrivilegeContext::SudoRootWithOriginalUser(
            crate::domain::OriginalUser {
                name: account.name,
                uid: Some(account.uid),
                gid: Some(account.gid.saturating_add(1)),
            },
        );
        let error = super::prepare_command_with_context_for_effective_uid(
            &crate::domain::NativeCommand::new("/bin/true"),
            crate::domain::PrivilegeRequirement::OriginalUserRequired,
            &context,
            Some(0),
            Some(std::path::Path::new("/usr/bin/sudo")),
        )
        .expect_err("changed original-user identity must be rejected");
        assert!(error
            .to_string()
            .contains("no longer matches validated uid/gid"));
    }

    #[cfg(unix)]
    #[test]
    fn trusted_sudo_candidates_prefer_fixed_paths_and_ignore_relative_path_entries() {
        let candidates = super::trusted_root_helper_candidates(
            "sudo",
            Some(std::ffi::OsStr::new(
                "relative:/tmp/path-shadow:/usr/local/bin",
            )),
        );
        assert_eq!(candidates[0], std::path::Path::new("/usr/bin/sudo"));
        assert_eq!(candidates[1], std::path::Path::new("/bin/sudo"));
        assert_eq!(candidates[2], std::path::Path::new("/tmp/path-shadow/sudo"));
        assert!(!candidates
            .iter()
            .any(|candidate| candidate == std::path::Path::new("relative/sudo")));
    }

    #[cfg(unix)]
    #[test]
    fn trusted_sudo_validator_rejects_user_controlled_helper_or_ancestor() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "allp-untrusted-sudo-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&root).expect("test directory");
        let helper = root.join("sudo");
        std::fs::write(&helper, "#!/bin/sh\nexit 0\n").expect("test helper");
        let mut permissions = std::fs::metadata(&helper)
            .expect("test helper metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).expect("test helper permissions");

        let error = super::validate_trusted_root_helper(&helper)
            .expect_err("user-controlled sudo helper must be rejected");
        assert!(error.to_string().contains("trusted helper"));
        std::fs::remove_dir_all(root).expect("test directory cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn root_required_validator_rejects_user_controlled_executable_or_ancestor() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "allp-untrusted-root-program-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&root).expect("test directory");
        let executable = root.join("apt-get");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("test executable");
        let mut permissions = std::fs::metadata(&executable)
            .expect("test executable metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions).expect("test executable permissions");

        let error =
            super::validate_trusted_root_executable(&executable, "root-required executable")
                .expect_err("a user-controlled root-required executable must be rejected");
        assert!(error.to_string().contains("root-required executable"));
        assert!(
            error.to_string().contains("not owned by root")
                || error.to_string().contains("ancestor is not a root-owned")
        );
        std::fs::remove_dir_all(root).expect("test directory cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn root_required_preparation_executes_the_validated_canonical_path() {
        use std::os::unix::fs::MetadataExt;

        let candidates = ["/usr/bin/true", "/bin/true", "/usr/bin/env"];
        let Some(source) = candidates.iter().map(std::path::Path::new).find(|path| {
            std::fs::metadata(path)
                .map(|metadata| metadata.uid() == 0)
                .unwrap_or(false)
        }) else {
            // Some sandboxed test environments remap host root-owned files to an
            // unprivileged overflow UID. The rejection test above remains effective there.
            return;
        };
        let expected = std::fs::canonicalize(source).expect("system true executable");
        let native = crate::domain::NativeCommand::new(source);

        let command = super::prepare_command_with_context_for_effective_uid(
            &native,
            crate::domain::PrivilegeRequirement::RootRequired,
            &crate::domain::RuntimePrivilegeContext::RootDirect,
            Some(0),
            None,
        )
        .expect("root-owned executable should pass the root boundary");

        assert_eq!(command.get_program(), expected.as_os_str());
    }
}
