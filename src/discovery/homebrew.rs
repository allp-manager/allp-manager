use crate::{
    domain::{NativeCommand, RuntimePrivilegeContext},
    execution::{
        privilege::{user_account_by_name, UserAccount},
        ProcessRunner,
    },
    platform::{OperatingSystem, PlatformContext, UserIdentity},
    state,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[cfg(unix)]
use crate::execution::privilege::{user_account_by_uid, user_group_ids};

const CONFIG_FILE: &str = "homebrew.json";
const STATE_FILE: &str = "homebrew-installation.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomebrewDiscoverySource {
    ExplicitConfiguration,
    CurrentPath,
    PersistedInstallation,
    EnvironmentPrefix,
    OriginalUserEnvironment,
    LinuxStandardPrefix,
    AppleSiliconStandardPrefix,
    IntelMacStandardPrefix,
}

impl HomebrewDiscoverySource {
    pub fn label(self) -> &'static str {
        match self {
            Self::ExplicitConfiguration => "Explicit Allp configuration",
            Self::CurrentPath => "Current PATH",
            Self::PersistedInstallation => "Persisted Homebrew",
            Self::EnvironmentPrefix => "HOMEBREW_PREFIX",
            Self::OriginalUserEnvironment => "Original user environment",
            Self::LinuxStandardPrefix => "Linux standard prefix",
            Self::AppleSiliconStandardPrefix => "Apple Silicon standard prefix",
            Self::IntelMacStandardPrefix => "Intel macOS standard prefix",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HomebrewCandidate {
    pub source: HomebrewDiscoverySource,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomebrewAttemptStatus {
    NotFound,
    Rejected,
    Validated,
    Duplicate,
    Unavailable,
}

impl HomebrewAttemptStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotFound => "not found",
            Self::Rejected => "rejected",
            Self::Validated => "validated",
            Self::Duplicate => "duplicate",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HomebrewDiscoveryAttempt {
    pub source: HomebrewDiscoverySource,
    pub executable: Option<PathBuf>,
    pub status: HomebrewAttemptStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomebrewPlatform {
    Linux,
    AppleSiliconMac,
    IntelMac,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HomebrewInstallation {
    pub executable: PathBuf,
    pub resolved_executable: PathBuf,
    pub version: String,
    pub prefix: PathBuf,
    pub repository: Option<PathBuf>,
    pub cellar: Option<PathBuf>,
    pub owner: UserIdentity,
    pub platform: HomebrewPlatform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomebrewInstallationRecord {
    pub executable: PathBuf,
    pub prefix: PathBuf,
    pub owner_uid: u32,
    pub owner_gid: u32,
    pub version: String,
    /// Seconds since the Unix epoch. The record is always revalidated before use.
    pub validated_at: u64,
}

impl HomebrewInstallationRecord {
    fn from_installation(installation: &HomebrewInstallation) -> Option<Self> {
        Some(Self {
            executable: installation.executable.clone(),
            prefix: installation.prefix.clone(),
            owner_uid: installation.owner.uid?,
            owner_gid: installation.owner.gid?,
            version: installation.version.clone(),
            validated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HomebrewProblemKind {
    InstalledButUnusable,
    WrongOwner,
    PermissionProblem,
    BrokenInstallation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HomebrewProblem {
    pub kind: HomebrewProblemKind,
    pub executable: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "detail", rename_all = "snake_case")]
pub enum HomebrewDetectionState {
    Ready(HomebrewInstallation),
    InstalledButUnusable(HomebrewProblem),
    WrongOwner(HomebrewProblem),
    PermissionProblem(HomebrewProblem),
    BrokenInstallation(HomebrewProblem),
    NotInstalled,
}

impl HomebrewDetectionState {
    pub fn installation(&self) -> Option<&HomebrewInstallation> {
        match self {
            Self::Ready(installation) => Some(installation),
            _ => None,
        }
    }

    pub fn problem(&self) -> Option<&HomebrewProblem> {
        match self {
            Self::InstalledButUnusable(problem)
            | Self::WrongOwner(problem)
            | Self::PermissionProblem(problem)
            | Self::BrokenInstallation(problem) => Some(problem),
            Self::Ready(_) | Self::NotInstalled => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HomebrewDiscovery {
    pub state: HomebrewDetectionState,
    pub attempts: Vec<HomebrewDiscoveryAttempt>,
}

pub trait HomebrewLocator: Send + Sync {
    fn locate(
        &self,
        platform: &PlatformContext,
        privilege: &RuntimePrivilegeContext,
        runner: &dyn ProcessRunner,
    ) -> HomebrewDiscovery;
}

pub struct SystemHomebrewLocator {
    process_path: Option<OsString>,
    environment_prefix: Option<PathBuf>,
    standard_candidates: Option<Vec<HomebrewCandidate>>,
    include_original_user_candidates: bool,
    read_configuration: bool,
    read_persisted_state: bool,
    write_persisted_state: bool,
}

impl Default for SystemHomebrewLocator {
    fn default() -> Self {
        Self {
            process_path: env::var_os("PATH"),
            environment_prefix: env::var_os("HOMEBREW_PREFIX").map(PathBuf::from),
            standard_candidates: None,
            include_original_user_candidates: true,
            read_configuration: true,
            read_persisted_state: true,
            write_persisted_state: true,
        }
    }
}

impl SystemHomebrewLocator {
    pub fn new() -> Self {
        Self::default()
    }

    fn collect_candidates(
        &self,
        platform: &PlatformContext,
        privilege: &RuntimePrivilegeContext,
        attempts: &mut Vec<HomebrewDiscoveryAttempt>,
    ) -> Vec<HomebrewCandidate> {
        let mut candidates = Vec::new();

        if self.read_configuration {
            let path = platform.config_dir.join(CONFIG_FILE);
            match state::read_json::<HomebrewConfiguration>(&path) {
                Ok(Some(configuration)) if configuration.executable.is_absolute() => candidates
                    .push(HomebrewCandidate {
                        source: HomebrewDiscoverySource::ExplicitConfiguration,
                        executable: configuration.executable,
                    }),
                Ok(Some(configuration)) => attempts.push(provider_rejected(
                    HomebrewDiscoverySource::ExplicitConfiguration,
                    Some(configuration.executable),
                    "configured Homebrew executable must be an absolute path".to_owned(),
                )),
                Ok(None) => attempts.push(provider_absent(
                    HomebrewDiscoverySource::ExplicitConfiguration,
                    format!("configuration file does not exist: {}", path.display()),
                )),
                Err(error) => attempts.push(provider_rejected(
                    HomebrewDiscoverySource::ExplicitConfiguration,
                    None,
                    format!("could not read configuration: {error}"),
                )),
            }
        }

        let mut path_candidates = Vec::new();
        for directory in self
            .process_path
            .as_deref()
            .into_iter()
            .flat_map(env::split_paths)
        {
            let executable = directory.join("brew");
            if !directory.is_absolute() {
                attempts.push(provider_rejected(
                    HomebrewDiscoverySource::CurrentPath,
                    Some(executable),
                    "relative PATH entries are not trusted for Homebrew discovery".to_owned(),
                ));
                continue;
            }
            if path_entry_exists(&executable) {
                path_candidates.push(HomebrewCandidate {
                    source: HomebrewDiscoverySource::CurrentPath,
                    executable,
                });
            }
        }
        if path_candidates.is_empty() {
            attempts.push(provider_absent(
                HomebrewDiscoverySource::CurrentPath,
                "brew was not present in the current process PATH".to_owned(),
            ));
        } else {
            candidates.extend(path_candidates);
        }

        if self.read_persisted_state {
            let path = platform.state_dir.join(STATE_FILE);
            match state::read_json::<HomebrewInstallationRecord>(&path) {
                Ok(Some(record)) if record.executable.is_absolute() => {
                    candidates.push(HomebrewCandidate {
                        source: HomebrewDiscoverySource::PersistedInstallation,
                        executable: record.executable,
                    })
                }
                Ok(Some(record)) => attempts.push(provider_rejected(
                    HomebrewDiscoverySource::PersistedInstallation,
                    Some(record.executable),
                    "persisted Homebrew executable must be an absolute path".to_owned(),
                )),
                Ok(None) => attempts.push(provider_absent(
                    HomebrewDiscoverySource::PersistedInstallation,
                    format!("installation record does not exist: {}", path.display()),
                )),
                Err(error) => attempts.push(provider_rejected(
                    HomebrewDiscoverySource::PersistedInstallation,
                    None,
                    format!("could not read persisted installation: {error}"),
                )),
            }
        }

        if let Some(prefix) = self.environment_prefix.as_ref() {
            if prefix.is_absolute() {
                candidates.push(HomebrewCandidate {
                    source: HomebrewDiscoverySource::EnvironmentPrefix,
                    executable: prefix.join("bin").join("brew"),
                });
            } else {
                attempts.push(provider_rejected(
                    HomebrewDiscoverySource::EnvironmentPrefix,
                    Some(prefix.join("bin").join("brew")),
                    "HOMEBREW_PREFIX was not absolute".to_owned(),
                ));
            }
        } else {
            attempts.push(provider_absent(
                HomebrewDiscoverySource::EnvironmentPrefix,
                "HOMEBREW_PREFIX was not set".to_owned(),
            ));
        }

        if self.include_original_user_candidates {
            if let Some(user) = privilege.original_user() {
                if let Some(account) = user_account_by_name(&user.name) {
                    for relative in [
                        Path::new(".linuxbrew/bin/brew"),
                        Path::new("homebrew/bin/brew"),
                    ] {
                        candidates.push(HomebrewCandidate {
                            source: HomebrewDiscoverySource::OriginalUserEnvironment,
                            executable: account.home.join(relative),
                        });
                    }
                } else {
                    attempts.push(provider_rejected(
                        HomebrewDiscoverySource::OriginalUserEnvironment,
                        None,
                        format!("original user {} is not in the account database", user.name),
                    ));
                }
            } else {
                attempts.push(provider_absent(
                    HomebrewDiscoverySource::OriginalUserEnvironment,
                    "no validated original sudo user is available".to_owned(),
                ));
            }
        }

        if !standard_paths_disabled() {
            candidates.extend(
                self.standard_candidates
                    .clone()
                    .unwrap_or_else(|| standard_candidates(platform.os)),
            );
        }

        candidates
    }
}

impl HomebrewLocator for SystemHomebrewLocator {
    fn locate(
        &self,
        platform: &PlatformContext,
        privilege: &RuntimePrivilegeContext,
        runner: &dyn ProcessRunner,
    ) -> HomebrewDiscovery {
        let mut attempts = Vec::new();
        let candidates = self.collect_candidates(platform, privilege, &mut attempts);
        let mut seen = BTreeSet::new();
        let mut selected = None;
        let mut first_problem = None;

        for candidate in candidates {
            let executable = candidate.executable;
            if !executable.is_absolute() {
                attempts.push(provider_rejected(
                    candidate.source,
                    Some(executable),
                    "Homebrew provider returned a relative executable path".to_owned(),
                ));
                continue;
            }
            if !seen.insert(executable.clone()) {
                attempts.push(HomebrewDiscoveryAttempt {
                    source: candidate.source,
                    executable: Some(executable),
                    status: HomebrewAttemptStatus::Duplicate,
                    message: Some(
                        "same candidate was already supplied by a higher-priority provider"
                            .to_owned(),
                    ),
                });
                continue;
            }

            if !path_entry_exists(&executable) {
                attempts.push(HomebrewDiscoveryAttempt {
                    source: candidate.source,
                    executable: Some(executable),
                    status: HomebrewAttemptStatus::NotFound,
                    message: None,
                });
                continue;
            }

            let execution_user =
                match execution_user_for_candidate(&executable, platform, privilege) {
                    Ok(user) => user,
                    Err(problem) => {
                        attempts.push(problem_attempt(candidate.source, &problem));
                        first_problem.get_or_insert(problem);
                        continue;
                    }
                };

            match validate_homebrew_candidate(
                &executable,
                &execution_user,
                platform,
                privilege,
                runner,
            ) {
                Ok(installation) => {
                    attempts.push(HomebrewDiscoveryAttempt {
                        source: candidate.source,
                        executable: Some(executable),
                        status: HomebrewAttemptStatus::Validated,
                        message: Some(format!(
                            "Homebrew {} at {}",
                            installation.version,
                            installation.prefix.display()
                        )),
                    });
                    if selected.is_none() {
                        selected = Some(installation);
                    }
                }
                Err(problem) => {
                    attempts.push(problem_attempt(candidate.source, &problem));
                    first_problem.get_or_insert(problem);
                }
            }
        }

        let state = if let Some(installation) = selected {
            if self.write_persisted_state {
                if let Some(record) = HomebrewInstallationRecord::from_installation(&installation) {
                    let path = platform.state_dir.join(STATE_FILE);
                    if let Err(error) = state::write_json_atomically(&path, &record) {
                        attempts.push(provider_rejected(
                            HomebrewDiscoverySource::PersistedInstallation,
                            Some(path),
                            format!("validated installation could not be persisted: {error}"),
                        ));
                    }
                }
            }
            HomebrewDetectionState::Ready(installation)
        } else if let Some(problem) = first_problem {
            detection_state_for_problem(problem)
        } else {
            HomebrewDetectionState::NotInstalled
        };

        HomebrewDiscovery { state, attempts }
    }
}

#[derive(Debug, Deserialize)]
struct HomebrewConfiguration {
    executable: PathBuf,
}

pub fn validate_homebrew_candidate(
    executable: &Path,
    execution_user: &UserAccount,
    platform: &PlatformContext,
    privilege: &RuntimePrivilegeContext,
    runner: &dyn ProcessRunner,
) -> Result<HomebrewInstallation, HomebrewProblem> {
    let executable = executable.to_path_buf();
    if !executable.is_absolute() {
        return Err(problem(
            HomebrewProblemKind::BrokenInstallation,
            executable,
            "candidate path is not absolute",
        ));
    }

    let link_metadata = fs::symlink_metadata(&executable).map_err(|error| {
        problem(
            HomebrewProblemKind::BrokenInstallation,
            executable.clone(),
            format!("could not stat candidate: {error}"),
        )
    })?;
    if !link_metadata.file_type().is_file() && !link_metadata.file_type().is_symlink() {
        return Err(problem(
            HomebrewProblemKind::BrokenInstallation,
            executable,
            "candidate is neither a regular file nor a symbolic link",
        ));
    }

    let resolved_executable = fs::canonicalize(&executable).map_err(|error| {
        problem(
            HomebrewProblemKind::BrokenInstallation,
            executable.clone(),
            format!("could not resolve candidate symlink: {error}"),
        )
    })?;
    let metadata = fs::metadata(&resolved_executable).map_err(|error| {
        problem(
            HomebrewProblemKind::BrokenInstallation,
            executable.clone(),
            format!("could not stat resolved executable: {error}"),
        )
    })?;
    if !metadata.is_file() {
        return Err(problem(
            HomebrewProblemKind::BrokenInstallation,
            executable,
            "resolved candidate is not a regular file",
        ));
    }

    #[cfg(unix)]
    {
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(problem(
                HomebrewProblemKind::PermissionProblem,
                executable,
                "resolved candidate has no executable permission bits",
            ));
        }
        if !is_executable_by(&metadata, execution_user) {
            return Err(problem(
                HomebrewProblemKind::PermissionProblem,
                executable,
                format!(
                    "candidate is not executable by {} (uid {})",
                    execution_user.name, execution_user.uid
                ),
            ));
        }
    }

    if matches!(privilege, RuntimePrivilegeContext::RootDirect) {
        return Err(problem(
            HomebrewProblemKind::WrongOwner,
            executable,
            "refusing to execute Homebrew as direct root without an original sudo user",
        ));
    }

    let version_output = runner
        .capture_in_user_context(
            &NativeCommand::new(&executable).arg("--version"),
            execution_user,
        )
        .map_err(|error| {
            problem(
                HomebrewProblemKind::InstalledButUnusable,
                executable.clone(),
                format!("brew --version could not start: {error}"),
            )
        })?;
    if !version_output.success {
        return Err(problem(
            HomebrewProblemKind::InstalledButUnusable,
            executable,
            command_failure(
                "brew --version",
                version_output.code,
                &version_output.stderr,
            ),
        ));
    }
    let version = parse_version(&version_output.stdout).ok_or_else(|| {
        problem(
            HomebrewProblemKind::InstalledButUnusable,
            executable.clone(),
            "brew --version returned no recognizable Homebrew version",
        )
    })?;

    let prefix_output = runner
        .capture_in_user_context(
            &NativeCommand::new(&executable).arg("--prefix"),
            execution_user,
        )
        .map_err(|error| {
            problem(
                HomebrewProblemKind::InstalledButUnusable,
                executable.clone(),
                format!("brew --prefix could not start: {error}"),
            )
        })?;
    if !prefix_output.success {
        return Err(problem(
            HomebrewProblemKind::InstalledButUnusable,
            executable,
            command_failure("brew --prefix", prefix_output.code, &prefix_output.stderr),
        ));
    }
    let prefix = parse_prefix(&prefix_output.stdout).ok_or_else(|| {
        problem(
            HomebrewProblemKind::BrokenInstallation,
            executable.clone(),
            "brew --prefix did not return one absolute path",
        )
    })?;
    let prefix_metadata = fs::metadata(&prefix).map_err(|error| {
        problem(
            HomebrewProblemKind::BrokenInstallation,
            executable.clone(),
            format!(
                "reported prefix {} is unavailable: {error}",
                prefix.display()
            ),
        )
    })?;
    if !prefix_metadata.is_dir() {
        return Err(problem(
            HomebrewProblemKind::BrokenInstallation,
            executable,
            format!("reported prefix {} is not a directory", prefix.display()),
        ));
    }
    let resolved_prefix = fs::canonicalize(&prefix).map_err(|error| {
        problem(
            HomebrewProblemKind::BrokenInstallation,
            executable.clone(),
            format!("reported prefix could not be resolved: {error}"),
        )
    })?;
    if !resolved_executable.starts_with(&resolved_prefix) {
        return Err(problem(
            HomebrewProblemKind::BrokenInstallation,
            executable,
            format!(
                "resolved executable {} is outside reported prefix {}",
                resolved_executable.display(),
                resolved_prefix.display()
            ),
        ));
    }

    #[cfg(unix)]
    let owner = owner_identity(metadata.uid(), metadata.gid());
    #[cfg(not(unix))]
    let owner = UserIdentity {
        name: execution_user.name.clone(),
        uid: Some(execution_user.uid),
        gid: Some(execution_user.gid),
    };

    Ok(HomebrewInstallation {
        executable,
        resolved_executable,
        version,
        repository: existing_directory(prefix.join("Homebrew")),
        cellar: existing_directory(prefix.join("Cellar")),
        platform: classify_platform(platform.os, &prefix),
        prefix,
        owner,
    })
}

/// Revalidates one already-selected Homebrew executable immediately before mutation.
///
/// This deliberately does not fall through to another candidate: a plan is bound to one
/// executable, so an identity change invalidates that plan instead of silently redirecting it.
pub fn revalidate_homebrew_executable(
    executable: &Path,
    privilege: &RuntimePrivilegeContext,
    runner: &dyn ProcessRunner,
) -> Result<HomebrewInstallation, HomebrewProblem> {
    if !executable.is_absolute() {
        return Err(problem(
            HomebrewProblemKind::BrokenInstallation,
            executable.to_path_buf(),
            "selected Homebrew executable is no longer an absolute path",
        ));
    }
    let platform = PlatformContext::detect(privilege);
    let execution_user = execution_user_for_candidate(executable, &platform, privilege)?;
    validate_homebrew_candidate(executable, &execution_user, &platform, privilege, runner)
}

fn execution_user_for_candidate(
    executable: &Path,
    platform: &PlatformContext,
    privilege: &RuntimePrivilegeContext,
) -> Result<UserAccount, HomebrewProblem> {
    #[cfg(unix)]
    {
        let metadata = fs::metadata(executable).map_err(|error| {
            problem(
                HomebrewProblemKind::BrokenInstallation,
                executable.to_path_buf(),
                format!("could not stat candidate owner: {error}"),
            )
        })?;
        let owner_uid = metadata.uid();
        match privilege {
            RuntimePrivilegeContext::SudoRootWithOriginalUser(user) => {
                let Some(account) = user_account_by_name(&user.name) else {
                    return Err(problem(
                        HomebrewProblemKind::WrongOwner,
                        executable.to_path_buf(),
                        format!("original sudo user {} is not a system account", user.name),
                    ));
                };
                if user.uid != Some(account.uid)
                    || user.gid != Some(account.gid)
                    || account.uid != owner_uid
                {
                    return Err(problem(
                        HomebrewProblemKind::WrongOwner,
                        executable.to_path_buf(),
                        format!(
                            "candidate owner uid {owner_uid} does not match validated original user {} (uid {})",
                            account.name, account.uid
                        ),
                    ));
                }
                Ok(account)
            }
            RuntimePrivilegeContext::RootDirect => Err(problem(
                HomebrewProblemKind::WrongOwner,
                executable.to_path_buf(),
                "Homebrew is present, but direct-root Allp has no validated execution user",
            )),
            RuntimePrivilegeContext::NormalUser => {
                let uid = platform.current_user.uid.ok_or_else(|| {
                    problem(
                        HomebrewProblemKind::WrongOwner,
                        executable.to_path_buf(),
                        "current user has no validated uid",
                    )
                })?;
                let account = user_account_by_uid(uid).ok_or_else(|| {
                    problem(
                        HomebrewProblemKind::WrongOwner,
                        executable.to_path_buf(),
                        format!("current uid {uid} is not a system account"),
                    )
                })?;
                Ok(account)
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = executable;
        let _ = platform;
        if matches!(privilege, RuntimePrivilegeContext::RootDirect) {
            Err(problem(
                HomebrewProblemKind::WrongOwner,
                PathBuf::from("brew"),
                "direct-root Homebrew execution is unavailable",
            ))
        } else {
            Err(problem(
                HomebrewProblemKind::WrongOwner,
                PathBuf::from("brew"),
                "owner-specific Homebrew execution is unsupported on this platform",
            ))
        }
    }
}

#[cfg(unix)]
fn is_executable_by(metadata: &fs::Metadata, user: &UserAccount) -> bool {
    executable_mode_allows(
        metadata.permissions().mode(),
        metadata.uid(),
        metadata.gid(),
        user.uid,
        &user_group_ids(user),
    )
}

#[cfg(unix)]
fn executable_mode_allows(
    mode: u32,
    owner_uid: u32,
    owner_gid: u32,
    user_uid: u32,
    user_groups: &BTreeSet<u32>,
) -> bool {
    if user_uid == owner_uid {
        mode & 0o100 != 0
    } else if user_groups.contains(&owner_gid) {
        mode & 0o010 != 0
    } else {
        mode & 0o001 != 0
    }
}

#[cfg(unix)]
fn owner_identity(uid: u32, gid: u32) -> UserIdentity {
    let name = user_account_by_uid(uid)
        .map(|account| account.name)
        .unwrap_or_else(|| format!("uid {uid}"));
    UserIdentity {
        name,
        uid: Some(uid),
        gid: Some(gid),
    }
}

fn parse_version(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("Homebrew ")
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .map(str::to_owned)
    })
}

fn parse_prefix(output: &str) -> Option<PathBuf> {
    let values = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if values.len() != 1 {
        return None;
    }
    let prefix = PathBuf::from(values[0]);
    prefix.is_absolute().then_some(prefix)
}

fn classify_platform(os: OperatingSystem, prefix: &Path) -> HomebrewPlatform {
    match os {
        OperatingSystem::Linux => HomebrewPlatform::Linux,
        OperatingSystem::MacOs if prefix.starts_with("/opt/homebrew") => {
            HomebrewPlatform::AppleSiliconMac
        }
        OperatingSystem::MacOs if prefix.starts_with("/usr/local") => HomebrewPlatform::IntelMac,
        _ => HomebrewPlatform::Other,
    }
}

fn standard_candidates(os: OperatingSystem) -> Vec<HomebrewCandidate> {
    match os {
        OperatingSystem::Linux => vec![HomebrewCandidate {
            source: HomebrewDiscoverySource::LinuxStandardPrefix,
            executable: PathBuf::from("/home/linuxbrew/.linuxbrew/bin/brew"),
        }],
        OperatingSystem::MacOs => vec![
            HomebrewCandidate {
                source: HomebrewDiscoverySource::AppleSiliconStandardPrefix,
                executable: PathBuf::from("/opt/homebrew/bin/brew"),
            },
            HomebrewCandidate {
                source: HomebrewDiscoverySource::IntelMacStandardPrefix,
                executable: PathBuf::from("/usr/local/bin/brew"),
            },
        ],
        OperatingSystem::Windows | OperatingSystem::Other => Vec::new(),
    }
}

fn standard_paths_disabled() -> bool {
    cfg!(debug_assertions) && env::var_os("ALLP_DISABLE_STANDARD_PATHS").is_some()
}

fn path_entry_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn existing_directory(path: PathBuf) -> Option<PathBuf> {
    path.is_dir().then_some(path)
}

fn provider_absent(source: HomebrewDiscoverySource, message: String) -> HomebrewDiscoveryAttempt {
    HomebrewDiscoveryAttempt {
        source,
        executable: None,
        status: HomebrewAttemptStatus::NotFound,
        message: Some(message),
    }
}

fn provider_rejected(
    source: HomebrewDiscoverySource,
    executable: Option<PathBuf>,
    message: String,
) -> HomebrewDiscoveryAttempt {
    HomebrewDiscoveryAttempt {
        source,
        executable,
        status: HomebrewAttemptStatus::Rejected,
        message: Some(message),
    }
}

fn problem_attempt(
    source: HomebrewDiscoverySource,
    problem: &HomebrewProblem,
) -> HomebrewDiscoveryAttempt {
    HomebrewDiscoveryAttempt {
        source,
        executable: Some(problem.executable.clone()),
        status: HomebrewAttemptStatus::Unavailable,
        message: Some(problem.message.clone()),
    }
}

fn problem(
    kind: HomebrewProblemKind,
    executable: PathBuf,
    message: impl Into<String>,
) -> HomebrewProblem {
    HomebrewProblem {
        kind,
        executable,
        message: message.into(),
    }
}

fn detection_state_for_problem(problem: HomebrewProblem) -> HomebrewDetectionState {
    match problem.kind {
        HomebrewProblemKind::InstalledButUnusable => {
            HomebrewDetectionState::InstalledButUnusable(problem)
        }
        HomebrewProblemKind::WrongOwner => HomebrewDetectionState::WrongOwner(problem),
        HomebrewProblemKind::PermissionProblem => {
            HomebrewDetectionState::PermissionProblem(problem)
        }
        HomebrewProblemKind::BrokenInstallation => {
            HomebrewDetectionState::BrokenInstallation(problem)
        }
    }
}

fn command_failure(command: &str, code: Option<i32>, stderr: &str) -> String {
    let detail = stderr.trim();
    if detail.is_empty() {
        format!("{command} failed with exit code {:?}", code)
    } else {
        format!("{command} failed with exit code {:?}: {detail}", code)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{
        backends::{homebrew::HomebrewBackend, Backend},
        domain::{AllpResult, ExecutionPlan, OriginalUser, PrivilegeRequirement},
        execution::{CommandOutput, ProcessStatus},
        platform::{Architecture, LibcFamily, RuntimeEnvironment},
    };
    use std::{collections::BTreeMap, sync::Mutex, time::Duration};

    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    struct ProbeRunner {
        prefixes: BTreeMap<PathBuf, PathBuf>,
        broken_versions: BTreeSet<PathBuf>,
        calls: Mutex<Vec<(PathBuf, String, PrivilegeRequirement)>>,
        users: Mutex<Vec<(String, u32)>>,
    }

    impl ProbeRunner {
        fn new(entries: impl IntoIterator<Item = (PathBuf, PathBuf)>) -> Self {
            Self {
                prefixes: entries.into_iter().collect(),
                broken_versions: BTreeSet::new(),
                calls: Mutex::new(Vec::new()),
                users: Mutex::new(Vec::new()),
            }
        }
    }

    impl ProcessRunner for ProbeRunner {
        fn capture(&self, command: &NativeCommand) -> AllpResult<crate::execution::CommandOutput> {
            self.capture_with_privilege(command, PrivilegeRequirement::NoElevation)
        }

        fn capture_with_privilege(
            &self,
            command: &NativeCommand,
            privilege: PrivilegeRequirement,
        ) -> AllpResult<CommandOutput> {
            let argument = command
                .args
                .first()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.calls.lock().expect("calls lock").push((
                command.program.clone(),
                argument.clone(),
                privilege,
            ));
            let success =
                !(argument == "--version" && self.broken_versions.contains(&command.program));
            let stdout = match argument.as_str() {
                "--version" if success => "Homebrew 6.0.15\n".to_owned(),
                "--prefix" => self
                    .prefixes
                    .get(&command.program)
                    .map(|path| format!("{}\n", path.display()))
                    .unwrap_or_default(),
                _ => String::new(),
            };
            Ok(CommandOutput {
                success,
                code: Some(if success { 0 } else { 1 }),
                signal: None,
                duration: Duration::ZERO,
                stdout,
                stderr: if success {
                    String::new()
                } else {
                    "broken brew".to_owned()
                },
            })
        }

        fn capture_in_user_context(
            &self,
            command: &NativeCommand,
            user: &UserAccount,
        ) -> AllpResult<CommandOutput> {
            self.users
                .lock()
                .expect("users lock")
                .push((user.name.clone(), user.uid));
            self.capture_with_privilege(command, PrivilegeRequirement::OriginalUserRequired)
        }

        fn execute(&self, _plan: &ExecutionPlan) -> AllpResult<ProcessStatus> {
            unreachable!("locator only captures probes")
        }
    }

    fn fixture_root(label: &str) -> PathBuf {
        let root = env::temp_dir().join(format!(
            "allp-homebrew-locator-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&root).expect("fixture root");
        root
    }

    #[cfg(unix)]
    fn fake_brew(prefix: &Path) -> PathBuf {
        let executable = prefix.join("bin/brew");
        fs::create_dir_all(executable.parent().expect("brew parent")).expect("brew parent");
        fs::write(&executable, b"#!/bin/sh\n").expect("brew fixture");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
            .expect("brew executable");
        executable
    }

    #[cfg(unix)]
    fn platform(root: &Path, elevated: bool) -> (PlatformContext, RuntimePrivilegeContext) {
        let metadata = fs::metadata(root).expect("fixture metadata");
        let uid = metadata.uid();
        let gid = metadata.gid();
        let account = user_account_by_uid(uid).expect("test uid should have an account");
        let original = UserIdentity {
            name: account.name.clone(),
            uid: Some(uid),
            gid: Some(gid),
        };
        let privilege = if elevated {
            RuntimePrivilegeContext::SudoRootWithOriginalUser(OriginalUser {
                name: account.name,
                uid: Some(uid),
                gid: Some(account.gid),
            })
        } else {
            RuntimePrivilegeContext::NormalUser
        };
        (
            PlatformContext {
                os: OperatingSystem::Linux,
                distribution: None,
                distribution_family: None,
                architecture: Architecture::X86_64,
                libc: Some(LibcFamily::Glibc),
                environment: RuntimeEnvironment::Native,
                is_wsl: false,
                is_container: false,
                is_root: elevated,
                current_user: if elevated {
                    UserIdentity {
                        name: "root".to_owned(),
                        uid: Some(0),
                        gid: Some(0),
                    }
                } else {
                    original.clone()
                },
                original_user: elevated.then_some(original),
                current_executable: root.join("allp"),
                executable_owner: None,
                executable_writable: true,
                home_dir: root.to_path_buf(),
                cache_dir: root.join("cache"),
                state_dir: root.join("state"),
                config_dir: root.join("config"),
            },
            privilege,
        )
    }

    fn locator(path: Option<OsString>, standards: Vec<HomebrewCandidate>) -> SystemHomebrewLocator {
        SystemHomebrewLocator {
            process_path: path,
            environment_prefix: None,
            standard_candidates: Some(standards),
            include_original_user_candidates: false,
            read_configuration: false,
            read_persisted_state: true,
            write_persisted_state: true,
        }
    }

    #[cfg(unix)]
    #[test]
    fn sudo_root_path_can_miss_brew_while_linux_standard_prefix_is_ready() {
        let root = fixture_root("sudo-standard");
        let prefix = root.join("home/linuxbrew/.linuxbrew");
        let executable = fake_brew(&prefix);
        let (platform, privilege) = platform(&root, true);
        let runner = ProbeRunner::new([(executable.clone(), prefix.clone())]);
        let locator = locator(
            Some(root.join("root-path-without-brew").into_os_string()),
            vec![HomebrewCandidate {
                source: HomebrewDiscoverySource::LinuxStandardPrefix,
                executable: executable.clone(),
            }],
        );

        let discovery = locator.locate(&platform, &privilege, &runner);
        let installation = discovery
            .state
            .installation()
            .expect("standard Linuxbrew should be ready");
        assert_eq!(installation.executable, executable);
        assert_eq!(installation.version, "6.0.15");
        assert_eq!(installation.prefix, prefix);
        assert!(runner
            .calls
            .lock()
            .expect("calls")
            .iter()
            .all(|(_, _, privilege)| *privilege == PrivilegeRequirement::OriginalUserRequired));
        let owner_contexts = runner.users.lock().expect("users");
        assert_eq!(owner_contexts.len(), 2);
        assert!(owner_contexts
            .iter()
            .all(|(name, uid)| name == &installation.owner.name
                && Some(*uid) == installation.owner.uid));
        assert!(discovery.attempts.iter().any(|attempt| {
            attempt.source == HomebrewDiscoverySource::CurrentPath
                && attempt.status == HomebrewAttemptStatus::NotFound
        }));
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn current_path_has_priority_over_standard_prefix() {
        let root = fixture_root("path-priority");
        let custom_prefix = root.join("custom");
        let standard_prefix = root.join("standard");
        let custom = fake_brew(&custom_prefix);
        let standard = fake_brew(&standard_prefix);
        let (platform, privilege) = platform(&root, false);
        let runner = ProbeRunner::new([
            (custom.clone(), custom_prefix),
            (standard.clone(), standard_prefix),
        ]);
        let locator = locator(
            Some(custom.parent().expect("custom bin").as_os_str().to_owned()),
            vec![HomebrewCandidate {
                source: HomebrewDiscoverySource::LinuxStandardPrefix,
                executable: standard,
            }],
        );

        let discovery = locator.locate(&platform, &privilege, &runner);
        assert_eq!(
            discovery
                .state
                .installation()
                .map(|value| &value.executable),
            Some(&custom)
        );
        assert_eq!(
            discovery
                .attempts
                .iter()
                .filter(|attempt| attempt.status == HomebrewAttemptStatus::Validated)
                .count(),
            2,
            "all valid candidates remain visible in diagnostics"
        );
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn stale_persisted_path_is_rejected_and_standard_discovery_continues() {
        let root = fixture_root("stale-state");
        let prefix = root.join("valid");
        let executable = fake_brew(&prefix);
        let (platform, privilege) = platform(&root, false);
        fs::create_dir_all(&platform.state_dir).expect("state dir");
        state::write_json_atomically(
            &platform.state_dir.join(STATE_FILE),
            &HomebrewInstallationRecord {
                executable: root.join("old/homebrew/bin/brew"),
                prefix: root.join("old/homebrew"),
                owner_uid: platform.current_user.uid.expect("uid"),
                owner_gid: platform.current_user.gid.expect("gid"),
                version: "5.0.0".to_owned(),
                validated_at: 1,
            },
        )
        .expect("stale state");
        let runner = ProbeRunner::new([(executable.clone(), prefix)]);
        let locator = locator(
            None,
            vec![HomebrewCandidate {
                source: HomebrewDiscoverySource::LinuxStandardPrefix,
                executable: executable.clone(),
            }],
        );

        let discovery = locator.locate(&platform, &privilege, &runner);
        assert_eq!(
            discovery
                .state
                .installation()
                .map(|value| &value.executable),
            Some(&executable)
        );
        assert!(discovery.attempts.iter().any(|attempt| {
            attempt.source == HomebrewDiscoverySource::PersistedInstallation
                && attempt.status == HomebrewAttemptStatus::NotFound
        }));
        let refreshed =
            state::read_json::<HomebrewInstallationRecord>(&platform.state_dir.join(STATE_FILE))
                .expect("refreshed state")
                .expect("installation record");
        assert_eq!(refreshed.executable, executable);
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn failed_version_probe_is_installed_but_unusable() {
        let root = fixture_root("broken-version");
        let prefix = root.join("broken");
        let executable = fake_brew(&prefix);
        let (platform, privilege) = platform(&root, false);
        let mut runner = ProbeRunner::new([(executable.clone(), prefix)]);
        runner.broken_versions.insert(executable.clone());
        let locator = locator(
            Some(executable.parent().expect("bin").as_os_str().to_owned()),
            Vec::new(),
        );

        let discovery = locator.locate(&platform, &privilege, &runner);
        assert!(matches!(
            discovery.state,
            HomebrewDetectionState::InstalledButUnusable(_)
        ));
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn relative_config_state_and_path_candidates_are_rejected() {
        let root = fixture_root("relative-providers");
        let (platform, privilege) = platform(&root, false);
        fs::create_dir_all(&platform.config_dir).expect("config dir");
        fs::create_dir_all(&platform.state_dir).expect("state dir");
        fs::write(
            platform.config_dir.join(CONFIG_FILE),
            br#"{"executable":"relative-config/bin/brew"}"#,
        )
        .expect("relative configuration");
        state::write_json_atomically(
            &platform.state_dir.join(STATE_FILE),
            &HomebrewInstallationRecord {
                executable: PathBuf::from("relative-state/bin/brew"),
                prefix: PathBuf::from("relative-state"),
                owner_uid: platform.current_user.uid.expect("uid"),
                owner_gid: platform.current_user.gid.expect("gid"),
                version: "6.0.15".to_owned(),
                validated_at: 1,
            },
        )
        .expect("relative state");
        let runner = ProbeRunner::new([]);
        let locator = SystemHomebrewLocator {
            process_path: Some(OsString::from("relative-path")),
            environment_prefix: None,
            standard_candidates: Some(Vec::new()),
            include_original_user_candidates: false,
            read_configuration: true,
            read_persisted_state: true,
            write_persisted_state: false,
        };

        let discovery = locator.locate(&platform, &privilege, &runner);
        assert!(matches!(
            discovery.state,
            HomebrewDetectionState::NotInstalled
        ));
        for source in [
            HomebrewDiscoverySource::ExplicitConfiguration,
            HomebrewDiscoverySource::CurrentPath,
            HomebrewDiscoverySource::PersistedInstallation,
        ] {
            assert!(discovery.attempts.iter().any(|attempt| {
                attempt.source == source && attempt.status == HomebrewAttemptStatus::Rejected
            }));
        }
        assert!(runner.calls.lock().expect("calls").is_empty());
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn selected_executable_is_revalidated_and_permission_change_is_rejected() {
        let root = fixture_root("mutation-revalidation");
        let prefix = root.join("brew-prefix");
        let executable = fake_brew(&prefix);
        let (platform, privilege) = platform(&root, false);
        let runner = ProbeRunner::new([(executable.clone(), prefix)]);
        let locator = locator(
            Some(executable.parent().expect("bin").as_os_str().to_owned()),
            Vec::new(),
        );
        let discovery = locator.locate(&platform, &privilege, &runner);
        assert!(discovery.state.installation().is_some());
        assert_eq!(runner.calls.lock().expect("calls").len(), 2);

        let plan = ExecutionPlan {
            backend_id: "brew".to_owned(),
            backend_name: "Homebrew".to_owned(),
            operation: crate::domain::OperationKind::Update,
            action: "test mutation".to_owned(),
            package_id: None,
            source: None,
            scope: None,
            details: Vec::new(),
            command: NativeCommand::new(&executable).arg("update"),
            privilege: PrivilegeRequirement::OriginalUserRequired,
            requires_root: false,
            interactive: false,
        };
        HomebrewBackend
            .validate_before_execution(&plan, &runner, &privilege)
            .expect("unchanged selected executable should revalidate through backend hook");
        assert_eq!(runner.calls.lock().expect("calls").len(), 4);

        fs::set_permissions(&executable, fs::Permissions::from_mode(0o644))
            .expect("remove executable permission");
        let problem = revalidate_homebrew_executable(&executable, &privilege, &runner)
            .expect_err("permission change must invalidate the planned executable");
        assert_eq!(problem.kind, HomebrewProblemKind::PermissionProblem);
        let hook_error = HomebrewBackend
            .validate_before_execution(&plan, &runner, &privilege)
            .expect_err("backend hook must refuse the permission-changed executable");
        assert!(hook_error
            .to_string()
            .contains("became unusable after planning"));
        fs::remove_dir_all(root).expect("fixture cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn supplementary_group_execute_permission_is_honored() {
        assert!(executable_mode_allows(
            0o050,
            2000,
            3000,
            1000,
            &BTreeSet::from([1000, 3000]),
        ));
        assert!(!executable_mode_allows(
            0o050,
            2000,
            3000,
            1000,
            &BTreeSet::from([1000]),
        ));
    }

    #[cfg(unix)]
    #[test]
    fn capability_registry_uses_validated_locator_identity() {
        let root = fixture_root("capability-sync");
        let prefix = root.join("brew-prefix");
        let executable = fake_brew(&prefix);
        let (platform, privilege) = platform(&root, false);
        let runner = ProbeRunner::new([(executable.clone(), prefix)]);
        let locator = locator(
            Some(executable.parent().expect("bin").as_os_str().to_owned()),
            Vec::new(),
        );
        let homebrew = locator.locate(&platform, &privilege, &runner);
        let report = crate::discovery::DiscoveryReport {
            entries: vec![crate::discovery::BackendDetection {
                backend_id: "brew".to_owned(),
                backend_name: "Homebrew".to_owned(),
                category: crate::domain::BackendCategory::Development,
                package_domains: vec![crate::domain::PackageDomain::Homebrew],
                state: crate::discovery::DetectionState::Ready,
                capabilities: Vec::new(),
                aliases: vec!["homebrew".to_owned()],
                commands: std::collections::BTreeMap::new(),
                missing: Vec::new(),
                message: None,
                homebrew: Some(homebrew),
            }],
        };
        let mut capabilities = crate::capabilities::CapabilityRegistry::default();
        capabilities.apply_discovery(&report);
        let capability = capabilities.executable("brew").expect("brew capability");
        assert_eq!(
            capability.availability,
            crate::capabilities::CapabilityAvailability::Available
        );
        assert_eq!(
            capability.resolved_path.as_deref(),
            Some(executable.as_path())
        );
        assert_eq!(capability.version.as_deref(), Some("6.0.15"));
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
