use crate::domain::{OriginalUser, RuntimePrivilegeContext};
use serde::Serialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
    Linux,
    MacOs,
    Windows,
    Other,
}

impl OperatingSystem {
    pub fn current() -> Self {
        match env::consts::OS {
            "linux" => Self::Linux,
            "macos" => Self::MacOs,
            "windows" => Self::Windows,
            _ => Self::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::MacOs => "macOS",
            Self::Windows => "Windows",
            Self::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Distribution {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributionFamily {
    Debian,
    RedHat,
    Arch,
    Suse,
    Alpine,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X86_64,
    Aarch64,
    X86,
    Arm,
    RiscV64,
    Other,
}

impl Architecture {
    pub fn current() -> Self {
        Self::from_name(env::consts::ARCH)
    }

    fn from_name(value: &str) -> Self {
        match value {
            "x86_64" => Self::X86_64,
            "aarch64" => Self::Aarch64,
            "x86" | "i686" | "i586" => Self::X86,
            "arm" => Self::Arm,
            "riscv64" => Self::RiscV64,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::X86 => "x86",
            Self::Arm => "arm",
            Self::RiscV64 => "riscv64",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LibcFamily {
    Glibc,
    Musl,
    Other,
}

impl LibcFamily {
    pub fn current(os: OperatingSystem) -> Option<Self> {
        Self::from_target_env(os, option_env!("CARGO_CFG_TARGET_ENV").unwrap_or(""))
    }

    fn from_target_env(os: OperatingSystem, target_env: &str) -> Option<Self> {
        if os != OperatingSystem::Linux {
            return None;
        }
        if target_env == "musl" || cfg!(target_env = "musl") {
            Some(Self::Musl)
        } else if target_env == "gnu" || cfg!(target_env = "gnu") {
            Some(Self::Glibc)
        } else {
            Some(Self::Other)
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Glibc => "glibc",
            Self::Musl => "musl",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEnvironment {
    Native,
    Wsl,
    Container,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserIdentity {
    pub name: String,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

impl From<&OriginalUser> for UserIdentity {
    fn from(user: &OriginalUser) -> Self {
        Self {
            name: user.name.clone(),
            uid: user.uid,
            gid: user.gid,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformContext {
    pub os: OperatingSystem,
    pub distribution: Option<Distribution>,
    pub distribution_family: Option<DistributionFamily>,
    pub architecture: Architecture,
    pub libc: Option<LibcFamily>,
    pub environment: RuntimeEnvironment,
    pub is_wsl: bool,
    pub is_container: bool,
    pub is_root: bool,
    pub current_user: UserIdentity,
    pub original_user: Option<UserIdentity>,
    pub current_executable: PathBuf,
    pub executable_owner: Option<UserIdentity>,
    pub executable_writable: bool,
    pub home_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
    pub config_dir: PathBuf,
}

impl PlatformContext {
    pub fn detect(privilege: &RuntimePrivilegeContext) -> Self {
        let os = OperatingSystem::current();
        let (distribution, distribution_family) = if os == OperatingSystem::Linux {
            detect_linux_distribution(Path::new("/etc/os-release"))
        } else {
            (None, None)
        };
        let is_wsl = detect_wsl();
        let is_container = detect_container();
        let environment = if is_wsl {
            RuntimeEnvironment::Wsl
        } else if is_container {
            RuntimeEnvironment::Container
        } else {
            RuntimeEnvironment::Native
        };
        let home_dir = home_directory(os);
        let (cache_dir, state_dir, config_dir) = application_directories(os, &home_dir);
        let current_executable = env::current_exe().unwrap_or_else(|_| PathBuf::from("allp"));
        let executable_owner = executable_owner(&current_executable);
        let executable_writable = path_is_writable(&current_executable, privilege.is_root());
        let current_user = current_user_identity();

        Self {
            os,
            distribution,
            distribution_family,
            architecture: Architecture::current(),
            libc: LibcFamily::current(os),
            environment,
            is_wsl,
            is_container,
            is_root: privilege.is_root(),
            current_user,
            original_user: privilege.original_user().map(UserIdentity::from),
            current_executable,
            executable_owner,
            executable_writable,
            home_dir,
            cache_dir,
            state_dir,
            config_dir,
        }
    }

    pub fn target_triple(&self) -> Option<String> {
        let arch = self.architecture.as_str();
        match self.os {
            OperatingSystem::Linux => match self.libc {
                Some(LibcFamily::Glibc) => Some(format!("{arch}-unknown-linux-gnu")),
                Some(LibcFamily::Musl) => Some(format!("{arch}-unknown-linux-musl")),
                _ => None,
            },
            OperatingSystem::MacOs => Some(format!("{arch}-apple-darwin")),
            OperatingSystem::Windows => Some(format!("{arch}-pc-windows-msvc")),
            OperatingSystem::Other => None,
        }
    }
}

pub fn detect_linux_distribution(
    path: &Path,
) -> (Option<Distribution>, Option<DistributionFamily>) {
    let Ok(contents) = fs::read_to_string(path) else {
        return (None, None);
    };
    parse_os_release(&contents)
}

fn parse_os_release(contents: &str) -> (Option<Distribution>, Option<DistributionFamily>) {
    let mut values = std::collections::BTreeMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        values.insert(key, value.trim_matches('"').trim_matches('\'').to_owned());
    }
    let Some(id) = values.get("ID").cloned() else {
        return (None, None);
    };
    let name = values
        .get("PRETTY_NAME")
        .or_else(|| values.get("NAME"))
        .cloned()
        .unwrap_or_else(|| id.clone());
    let version = values.get("VERSION_ID").cloned();
    let family_text = format!(
        "{} {}",
        id.to_ascii_lowercase(),
        values
            .get("ID_LIKE")
            .cloned()
            .unwrap_or_default()
            .to_ascii_lowercase()
    );
    let family = if contains_word(&family_text, &["debian", "ubuntu", "mint", "pop"]) {
        DistributionFamily::Debian
    } else if contains_word(&family_text, &["fedora", "rhel", "centos", "rocky", "alma"]) {
        DistributionFamily::RedHat
    } else if contains_word(&family_text, &["arch", "manjaro", "endeavouros"]) {
        DistributionFamily::Arch
    } else if contains_word(&family_text, &["suse", "opensuse", "sles"]) {
        DistributionFamily::Suse
    } else if contains_word(&family_text, &["alpine"]) {
        DistributionFamily::Alpine
    } else {
        DistributionFamily::Other
    };
    (Some(Distribution { id, name, version }), Some(family))
}

fn contains_word(value: &str, words: &[&str]) -> bool {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|part| words.contains(&part))
}

fn detect_wsl() -> bool {
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .or_else(|_| fs::read_to_string("/proc/version"))
        .unwrap_or_default();
    detect_wsl_from(
        env::var_os("WSL_DISTRO_NAME").is_some(),
        env::var_os("WSL_INTEROP").is_some(),
        &kernel,
    )
}

fn detect_container() -> bool {
    let cgroup = fs::read_to_string("/proc/1/cgroup").unwrap_or_default();
    detect_container_from(
        env::var_os("container").is_some(),
        Path::new("/.dockerenv").exists(),
        Path::new("/run/.containerenv").exists(),
        &cgroup,
    )
}

fn detect_wsl_from(has_distro: bool, has_interop: bool, kernel: &str) -> bool {
    has_distro || has_interop || kernel.to_ascii_lowercase().contains("microsoft")
}

fn detect_container_from(
    has_container_env: bool,
    has_dockerenv: bool,
    has_containerenv: bool,
    cgroup: &str,
) -> bool {
    has_container_env
        || has_dockerenv
        || has_containerenv
        || ["docker", "containerd", "kubepods", "lxc", "podman"]
            .iter()
            .any(|marker| cgroup.to_ascii_lowercase().contains(marker))
}

fn home_directory(os: OperatingSystem) -> PathBuf {
    let home = match os {
        OperatingSystem::Windows => env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")),
        _ => env::var_os("HOME"),
    };
    home.map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn application_directories(os: OperatingSystem, home: &Path) -> (PathBuf, PathBuf, PathBuf) {
    match os {
        OperatingSystem::Windows => {
            let base = env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join("Local"));
            let root = base.join("Allp");
            (root.join("cache"), root.join("state"), root.join("config"))
        }
        OperatingSystem::MacOs => (
            home.join("Library").join("Caches").join("allp"),
            home.join("Library")
                .join("Application Support")
                .join("allp")
                .join("state"),
            home.join("Library")
                .join("Application Support")
                .join("allp"),
        ),
        _ => (
            env_path("XDG_CACHE_HOME", home.join(".cache")).join("allp"),
            env_path("XDG_STATE_HOME", home.join(".local").join("state")).join("allp"),
            env_path("XDG_CONFIG_HOME", home.join(".config")).join("allp"),
        ),
    }
}

fn env_path(key: &str, fallback: PathBuf) -> PathBuf {
    env::var_os(key).map(PathBuf::from).unwrap_or(fallback)
}

fn executable_owner(path: &Path) -> Option<UserIdentity> {
    let metadata = fs::metadata(path).ok()?;
    #[cfg(unix)]
    {
        Some(UserIdentity {
            name: format!("uid {}", metadata.uid()),
            uid: Some(metadata.uid()),
            gid: Some(metadata.gid()),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn current_user_identity() -> UserIdentity {
    let (uid, gid, _) = effective_unix_identity();
    let name = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| uid.map_or_else(|| "unknown".to_owned(), |uid| format!("uid {uid}")));
    UserIdentity { name, uid, gid }
}

pub fn path_is_writable(path: &Path, is_root: bool) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return path
            .parent()
            .and_then(|parent| fs::metadata(parent).ok())
            .map(|metadata| !metadata.permissions().readonly())
            .unwrap_or(false);
    };
    if is_root {
        return true;
    }
    #[cfg(unix)]
    {
        let mode = metadata.permissions().mode();
        let (uid, _gid, groups) = effective_unix_identity();
        if uid == Some(metadata.uid()) {
            mode & 0o200 != 0
        } else if groups.contains(&metadata.gid()) {
            mode & 0o020 != 0
        } else {
            mode & 0o002 != 0
        }
    }
    #[cfg(not(unix))]
    {
        !metadata.permissions().readonly()
    }
}

#[cfg(unix)]
fn effective_unix_identity() -> (Option<u32>, Option<u32>, Vec<u32>) {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        let uid = status.lines().find_map(|line| {
            line.strip_prefix("Uid:")?
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()
        });
        let gid = status.lines().find_map(|line| {
            line.strip_prefix("Gid:")?
                .split_whitespace()
                .nth(1)?
                .parse()
                .ok()
        });
        let groups = status
            .lines()
            .find_map(|line| line.strip_prefix("Groups:"))
            .map(|values| {
                values
                    .split_whitespace()
                    .filter_map(|value| value.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        if uid.is_some() {
            return (uid, gid, groups);
        }
    }
    let uid = numeric_id(&["-u"]);
    let gid = numeric_id(&["-g"]);
    let groups = Command::new("id")
        .arg("-G")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    (uid, gid, groups)
}

#[cfg(not(unix))]
fn effective_unix_identity() -> (Option<u32>, Option<u32>, Vec<u32>) {
    (None, None, Vec::new())
}

#[cfg(unix)]
fn numeric_id(args: &[&str]) -> Option<u32> {
    Command::new("id")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_distribution_and_family() {
        let (distribution, family) = parse_os_release(
            "ID=ubuntu\nPRETTY_NAME=\"Ubuntu 24.04\"\nVERSION_ID=\"24.04\"\nID_LIKE=debian\n",
        );
        let distribution = distribution.expect("distribution should parse");
        assert_eq!(distribution.id, "ubuntu");
        assert_eq!(distribution.version.as_deref(), Some("24.04"));
        assert_eq!(family, Some(DistributionFamily::Debian));
    }

    #[test]
    fn architecture_is_normalized() {
        assert_ne!(Architecture::current().as_str(), "");
        assert_eq!(Architecture::from_name("aarch64"), Architecture::Aarch64);
        assert_eq!(Architecture::from_name("mystery"), Architecture::Other);
    }

    #[test]
    fn libc_is_only_reported_for_linux() {
        assert_eq!(LibcFamily::current(OperatingSystem::MacOs), None);
        assert_eq!(
            LibcFamily::from_target_env(OperatingSystem::Linux, "musl"),
            Some(LibcFamily::Musl)
        );
    }

    #[test]
    fn wsl_detection_uses_environment_or_kernel_markers() {
        assert!(detect_wsl_from(true, false, "Linux"));
        assert!(detect_wsl_from(false, false, "microsoft-standard-WSL2"));
        assert!(!detect_wsl_from(false, false, "Linux native"));
    }

    #[test]
    fn container_detection_uses_environment_files_or_cgroups() {
        assert!(detect_container_from(true, false, false, ""));
        assert!(detect_container_from(false, false, false, "kubepods.slice"));
        assert!(!detect_container_from(false, false, false, "user.slice"));
    }

    #[cfg(unix)]
    #[test]
    fn writable_path_detection_honors_mode_bits() {
        use std::os::unix::fs::PermissionsExt;
        let path =
            std::env::temp_dir().join(format!("allp-platform-writable-{}", std::process::id()));
        fs::write(&path, b"allp").expect("fixture should be written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444))
            .expect("fixture permissions should change");
        assert!(!path_is_writable(&path, false));
        fs::remove_file(path).expect("fixture should be removed");
    }
}
