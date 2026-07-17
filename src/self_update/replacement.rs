use super::{
    checksum::verify_sha256, github::validate_release_asset_url, ReleaseDescriptor,
    OFFICIAL_REPOSITORY,
};
use crate::{
    discovery::path::find_executable,
    domain::{AllpError, AllpResult, NativeCommand},
    platform::{OperatingSystem, PlatformContext},
    release::{ReleaseAsset, Version},
};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct StagedRelease {
    pub version: Version,
    pub binary_path: PathBuf,
    pub staging_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub enum ReplacementOutcome {
    Replaced,
    RequiresElevation { command: NativeCommand },
    DeferredForWindows { staged_binary: PathBuf },
}

pub fn stage_release(
    release: &ReleaseDescriptor,
    asset: &ReleaseAsset,
    platform: &PlatformContext,
) -> AllpResult<StagedRelease> {
    if asset.size == 0 || asset.size > MAX_ASSET_BYTES {
        return Err(AllpError::InvalidInput(format!(
            "release asset size {} exceeds Allp's safety policy",
            asset.size
        )));
    }
    let url = format!(
        "https://github.com/{}/{}/releases/download/{}/{}",
        OFFICIAL_REPOSITORY.owner, OFFICIAL_REPOSITORY.name, release.tag, asset.archive
    );
    validate_release_asset_url(OFFICIAL_REPOSITORY, &release.tag, &url)?;
    let staging_dir = create_staging_directory(&platform.cache_dir, release.version)?;
    let archive_path = staging_dir.join(&asset.archive);
    let result = (|| -> AllpResult<StagedRelease> {
        download_asset(&url, &archive_path, asset.size)?;
        verify_sha256(&archive_path, &asset.sha256)?;
        let extract_dir = staging_dir.join("extracted");
        fs::create_dir(&extract_dir)?;
        extract_archive_safely(&archive_path, &extract_dir, platform.os)?;
        let binary_path = find_staged_binary(&extract_dir, &asset.binary)?;
        verify_staged_binary(&binary_path, release.version)?;
        Ok(StagedRelease {
            version: release.version,
            binary_path,
            staging_dir: staging_dir.clone(),
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    result
}

pub fn apply_replacement(
    staged: &StagedRelease,
    platform: &PlatformContext,
) -> AllpResult<ReplacementOutcome> {
    if platform.os == OperatingSystem::Windows {
        return Ok(ReplacementOutcome::DeferredForWindows {
            staged_binary: staged.binary_path.clone(),
        });
    }
    if !platform.executable_writable {
        let helper = NativeCommand::new(&platform.current_executable).args([
            "internal-replace",
            "--staged",
            staged.binary_path.to_string_lossy().as_ref(),
            "--destination",
            platform.current_executable.to_string_lossy().as_ref(),
            "--version",
            &staged.version.to_string(),
        ]);
        return Ok(ReplacementOutcome::RequiresElevation { command: helper });
    }
    replace_binary_atomically(
        &staged.binary_path,
        &platform.current_executable,
        staged.version,
    )?;
    Ok(ReplacementOutcome::Replaced)
}

pub fn schedule_deferred_replacement(
    staged: &StagedRelease,
    platform: &PlatformContext,
    continuation: &[OsString],
) -> AllpResult<()> {
    if platform.os != OperatingSystem::Windows {
        return Err(AllpError::UnsupportedOperation {
            backend: "Allp self-update".to_owned(),
            operation: "deferred replacement outside Windows".to_owned(),
        });
    }
    let helper = staged.staging_dir.join("allp-replace-helper.exe");
    fs::copy(&platform.current_executable, &helper)?;
    let mut command = Command::new(&helper);
    command
        .args([
            "internal-deferred-replace",
            "--staged",
            staged.binary_path.to_string_lossy().as_ref(),
            "--destination",
            platform.current_executable.to_string_lossy().as_ref(),
            "--version",
            &staged.version.to_string(),
            "--cleanup-dir",
            staged.staging_dir.to_string_lossy().as_ref(),
        ])
        .arg("--")
        .args(continuation)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

pub fn run_deferred_replacement(
    staged: &Path,
    destination: &Path,
    expected_version: Version,
    cleanup_dir: &Path,
    continuation: &[OsString],
) -> AllpResult<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match replace_binary_atomically(staged, destination, expected_version) {
            Ok(()) => break,
            Err(AllpError::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::AlreadyExists
                ) && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(error) => return Err(error),
        }
    }
    if !continuation.is_empty() {
        Command::new(destination)
            .args(continuation)
            .env(super::SELF_UPDATE_COMPLETED_ENV, "1")
            .env(super::SELF_UPDATE_VERSION_ENV, expected_version.to_string())
            .env("ALLP_SELF_UPDATE_CLEANUP_DIR", cleanup_dir)
            .spawn()?;
    }
    Ok(())
}

pub fn replace_binary_atomically(
    staged: &Path,
    destination: &Path,
    expected_version: Version,
) -> AllpResult<()> {
    let parent = destination.parent().ok_or_else(|| {
        AllpError::InvalidInput(format!(
            "installed executable has no parent directory: {}",
            destination.display()
        ))
    })?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("allp");
    let replacement = parent.join(format!(".{name}.update-{}", std::process::id()));
    let backup = parent.join(format!(".{name}.rollback-{}", std::process::id()));
    if replacement.exists() || backup.exists() {
        return Err(AllpError::InvalidInput(
            "a previous Allp replacement staging file still exists".to_owned(),
        ));
    }

    let destination_metadata = fs::metadata(destination)?;
    let current_permissions = destination_metadata.permissions();
    fs::copy(staged, &replacement)?;
    fs::set_permissions(&replacement, current_permissions)?;
    preserve_destination_owner(&replacement, &destination_metadata)?;
    sync_file(&replacement)?;
    verify_staged_binary(&replacement, expected_version)?;

    if let Err(error) = rename_with_transient_retry(destination, &backup) {
        let _ = fs::remove_file(&replacement);
        return Err(error.into());
    }
    if let Err(error) = rename_with_transient_retry(&replacement, destination) {
        let _ = rename_with_transient_retry(&backup, destination);
        let _ = fs::remove_file(&replacement);
        return Err(error.into());
    }

    if let Err(error) = verify_staged_binary(destination, expected_version) {
        let failed = parent.join(format!(".{name}.failed-{}", std::process::id()));
        let _ = rename_with_transient_retry(destination, &failed);
        let rollback = rename_with_transient_retry(&backup, destination);
        let _ = fs::remove_file(&failed);
        if let Err(rollback_error) = rollback {
            return Err(AllpError::Io(std::io::Error::other(format!(
                "post-install verification failed ({error}); rollback also failed: {rollback_error}"
            ))));
        }
        return Err(AllpError::InvalidInput(format!(
            "post-install verification failed; the previous Allp binary was restored: {error}"
        )));
    }
    fs::remove_file(backup)?;
    Ok(())
}

fn rename_with_transient_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error)
                if transient_rename_error(&error) && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

fn transient_rename_error(error: &std::io::Error) -> bool {
    if matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::WouldBlock
    ) {
        return true;
    }
    #[cfg(unix)]
    {
        // Linux reports ETXTBSY when an interpreter still holds a newly verified file.
        error.raw_os_error() == Some(26)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn download_asset(url: &str, destination: &Path, expected_size: u64) -> AllpResult<()> {
    let curl = find_executable("curl").ok_or_else(|| {
        AllpError::BackendNotDetected("curl HTTPS client is required for self-update".to_owned())
    })?;
    let output = Command::new(curl)
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-redirs",
            "5",
            "--connect-timeout",
            "10",
            "--max-time",
            "180",
            "--max-filesize",
            &MAX_ASSET_BYTES.to_string(),
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--output",
        ])
        .arg(destination)
        .arg(url)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(AllpError::CommandFailed {
            backend: "Allp self-update download".to_owned(),
            command: format!("HTTPS GET {url}"),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let actual_size = fs::metadata(destination)?.len();
    if actual_size != expected_size {
        return Err(AllpError::InvalidInput(format!(
            "release asset size mismatch: expected {expected_size} bytes, received {actual_size}"
        )));
    }
    Ok(())
}

fn extract_archive_safely(
    archive: &Path,
    destination: &Path,
    os: OperatingSystem,
) -> AllpResult<()> {
    match os {
        OperatingSystem::Linux | OperatingSystem::MacOs => {
            let tar = find_executable("tar").ok_or_else(|| {
                AllpError::BackendNotDetected("tar is required to extract this update".to_owned())
            })?;
            let paths = Command::new(&tar).args(["-tzf"]).arg(archive).output()?;
            if !paths.status.success() {
                return Err(AllpError::CommandFailed {
                    backend: "Allp self-update archive".to_owned(),
                    command: format!("{} -tzf {}", tar.display(), archive.display()),
                    code: paths.status.code(),
                    stderr: String::from_utf8_lossy(&paths.stderr).into_owned(),
                });
            }
            validate_archive_listing_paths(&String::from_utf8_lossy(&paths.stdout))?;
            let verbose = Command::new(&tar).args(["-tvzf"]).arg(archive).output()?;
            if !verbose.status.success() {
                return Err(AllpError::CommandFailed {
                    backend: "Allp self-update archive".to_owned(),
                    command: format!("{} -tvzf {}", tar.display(), archive.display()),
                    code: verbose.status.code(),
                    stderr: String::from_utf8_lossy(&verbose.stderr).into_owned(),
                });
            }
            validate_tar_entry_types(&String::from_utf8_lossy(&verbose.stdout))?;
            let status = Command::new(&tar)
                .args(["-xzf"])
                .arg(archive)
                .args(["-C"])
                .arg(destination)
                .args(["--no-same-owner", "--no-same-permissions"])
                .status()?;
            if !status.success() {
                return Err(AllpError::CommandFailed {
                    backend: "Allp self-update archive".to_owned(),
                    command: format!("{} -xzf {}", tar.display(), archive.display()),
                    code: status.code(),
                    stderr: "archive extraction failed".to_owned(),
                });
            }
            Ok(())
        }
        OperatingSystem::Windows => {
            let tar = find_executable("tar").ok_or_else(|| {
                AllpError::BackendNotDetected(
                    "Windows tar support is required to inspect and extract this update".to_owned(),
                )
            })?;
            let listing = Command::new(&tar).args(["-tf"]).arg(archive).output()?;
            if !listing.status.success() {
                return Err(AllpError::CommandFailed {
                    backend: "Allp self-update archive".to_owned(),
                    command: format!("{} -tf {}", tar.display(), archive.display()),
                    code: listing.status.code(),
                    stderr: String::from_utf8_lossy(&listing.stderr).into_owned(),
                });
            }
            for entry in String::from_utf8_lossy(&listing.stdout).lines() {
                validate_archive_path(Path::new(entry.trim()))?;
            }
            let verbose = Command::new(&tar).args(["-tvf"]).arg(archive).output()?;
            if !verbose.status.success() {
                return Err(AllpError::CommandFailed {
                    backend: "Allp self-update archive".to_owned(),
                    command: format!("{} -tvf {}", tar.display(), archive.display()),
                    code: verbose.status.code(),
                    stderr: String::from_utf8_lossy(&verbose.stderr).into_owned(),
                });
            }
            validate_tar_entry_types(&String::from_utf8_lossy(&verbose.stdout))?;
            let status = Command::new(&tar)
                .args(["-xf"])
                .arg(archive)
                .args(["-C"])
                .arg(destination)
                .status()?;
            if !status.success() {
                return Err(AllpError::CommandFailed {
                    backend: "Allp self-update archive".to_owned(),
                    command: format!("{} -xf {}", tar.display(), archive.display()),
                    code: status.code(),
                    stderr: "ZIP extraction failed".to_owned(),
                });
            }
            Ok(())
        }
        OperatingSystem::Other => Err(AllpError::UnsupportedOperation {
            backend: "Allp self-update".to_owned(),
            operation: "archive extraction on this platform".to_owned(),
        }),
    }
}

fn validate_archive_listing_paths(listing: &str) -> AllpResult<()> {
    for path in listing
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        validate_archive_path(Path::new(path))?;
    }
    Ok(())
}

fn validate_tar_entry_types(listing: &str) -> AllpResult<()> {
    for line in listing.lines().filter(|line| !line.trim().is_empty()) {
        let kind = line.as_bytes().first().copied().unwrap_or(b'?');
        if matches!(kind, b'l' | b'h') {
            return Err(AllpError::InvalidInput(
                "release archive contains a symbolic or hard link".to_owned(),
            ));
        }
        if !matches!(kind, b'-' | b'd') {
            return Err(AllpError::InvalidInput(
                "release archive contains an unsupported special entry".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> AllpResult<()> {
    if path.is_absolute() || path.as_os_str().is_empty() {
        return Err(AllpError::InvalidInput(
            "release archive contains an absolute or empty path".to_owned(),
        ));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AllpError::InvalidInput(
            "release archive contains a path-traversal entry".to_owned(),
        ));
    }
    Ok(())
}

fn find_staged_binary(root: &Path, binary_name: &str) -> AllpResult<PathBuf> {
    let mut directories = vec![(root.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = directories.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(AllpError::InvalidInput(
                    "release extraction contains a symbolic link".to_owned(),
                ));
            }
            if file_type.is_file() && entry.file_name() == binary_name {
                return Ok(path);
            }
            if file_type.is_dir() && depth < 2 {
                directories.push((path, depth + 1));
            }
        }
    }
    Err(AllpError::InvalidInput(format!(
        "release archive does not contain the expected binary {binary_name}"
    )))
}

fn verify_staged_binary(path: &Path, expected: Version) -> AllpResult<()> {
    let output = Command::new(path).arg("--version").output()?;
    if !output.status.success() {
        return Err(AllpError::InvalidInput(format!(
            "staged binary failed --version with exit code {:?}",
            output.status.code()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = stdout
        .split_whitespace()
        .find_map(|word| word.trim_start_matches('v').parse::<Version>().ok());
    if parsed != Some(expected) {
        return Err(AllpError::InvalidInput(format!(
            "staged binary version mismatch: expected {expected}, got {}",
            parsed
                .map(|version| version.to_string())
                .unwrap_or_else(|| "unparseable output".to_owned())
        )));
    }
    Ok(())
}

fn create_staging_directory(root: &Path, version: Version) -> AllpResult<PathBuf> {
    fs::create_dir_all(root)?;
    for attempt in 0..100u32 {
        let path = root.join(format!(
            ".allp-update-{version}-{}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AllpError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique self-update staging directory",
    )))
}

fn sync_file(path: &Path) -> AllpResult<()> {
    let mut file = fs::OpenOptions::new().write(true).open(path)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

fn preserve_destination_owner(path: &Path, metadata: &fs::Metadata) -> AllpResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let replacement = fs::metadata(path)?;
        if replacement.uid() != metadata.uid() || replacement.gid() != metadata.gid() {
            std::os::unix::fs::chown(path, Some(metadata.uid()), Some(metadata.gid()))?;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (path, metadata);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::RuntimePrivilegeContext, platform::PlatformContext};

    #[test]
    fn path_traversal_archive_entry_is_rejected() {
        let error = validate_archive_path(Path::new("root/../../etc/passwd"))
            .expect_err("parent path must fail");
        assert!(error.to_string().contains("path-traversal"));
    }

    #[test]
    fn symlink_archive_entry_is_rejected() {
        let error = validate_tar_entry_types("lrwxrwxrwx user/group 0 date root/allp -> /bin/sh\n")
            .expect_err("symlink must fail");
        assert!(error.to_string().contains("symbolic or hard link"));
    }

    #[cfg(unix)]
    #[test]
    fn version_mismatch_is_rejected_before_replacement() {
        use std::os::unix::fs::PermissionsExt;
        let root = replacement_fixture("version-mismatch");
        let path = root.join("allp");
        fs::write(&path, b"#!/bin/sh\nprintf '%s\\n' 'allp 9.9.9'\n")
            .expect("fixture should be written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("fixture should be executable");
        let error = verify_staged_binary(&path, Version::new(0, 3, 4))
            .expect_err("mismatched binary must fail");
        assert!(error.to_string().contains("version mismatch"));
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replacement_installs_verified_binary() {
        use std::os::unix::fs::PermissionsExt;
        let root = replacement_fixture("success");
        let destination = root.join("allp");
        let staged = root.join("staged-allp");
        write_version_script(&destination, "0.3.3", 0o755);
        write_version_script(&staged, "0.3.4", 0o755);

        replace_binary_atomically(&staged, &destination, Version::new(0, 3, 4))
            .expect("verified replacement should succeed");
        let output = Command::new(&destination)
            .arg("--version")
            .output()
            .expect("replacement should run");
        assert!(String::from_utf8_lossy(&output.stdout).contains("0.3.4"));
        assert_eq!(
            fs::metadata(&destination).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn post_install_verification_failure_restores_previous_binary() {
        use std::os::unix::fs::PermissionsExt;
        let root = replacement_fixture("rollback");
        let destination = root.join("allp");
        let staged = root.join("staged-allp");
        write_version_script(&destination, "0.3.3", 0o755);
        fs::write(
            &staged,
            b"#!/bin/sh\ncase \"$0\" in */allp) v=9.9.9 ;; *) v=0.3.4 ;; esac\nprintf 'allp %s\\n' \"$v\"\n",
        )
        .expect("staged fixture should be written");
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).unwrap();

        let error = replace_binary_atomically(&staged, &destination, Version::new(0, 3, 4))
            .expect_err("post-install mismatch must roll back");
        assert!(
            error
                .to_string()
                .contains("previous Allp binary was restored"),
            "unexpected replacement error: {error}"
        );
        let output = Command::new(&destination)
            .arg("--version")
            .output()
            .expect("restored binary should run");
        assert!(String::from_utf8_lossy(&output.stdout).contains("0.3.3"));
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn non_writable_installation_creates_minimal_elevated_replacement() {
        let mut platform = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        platform.os = OperatingSystem::Linux;
        platform.current_executable = PathBuf::from("/usr/local/bin/allp");
        platform.executable_writable = false;
        let staged = StagedRelease {
            version: Version::new(0, 3, 4),
            binary_path: PathBuf::from("/tmp/allp-staged"),
            staging_dir: PathBuf::from("/tmp/allp-staging"),
        };
        let outcome = apply_replacement(&staged, &platform).expect("plan should be created");
        let ReplacementOutcome::RequiresElevation { command } = outcome else {
            panic!("non-writable path should require elevation");
        };
        assert_eq!(command.program, PathBuf::from("/usr/local/bin/allp"));
        assert_eq!(
            command
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec![
                "internal-replace",
                "--staged",
                "/tmp/allp-staged",
                "--destination",
                "/usr/local/bin/allp",
                "--version",
                "0.3.4",
            ]
        );
    }

    #[test]
    fn windows_replacement_is_deferred() {
        let mut platform = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        platform.os = OperatingSystem::Windows;
        let staged = StagedRelease {
            version: Version::new(0, 3, 4),
            binary_path: PathBuf::from(r"C:\Temp\allp.exe"),
            staging_dir: PathBuf::from(r"C:\Temp\allp-update"),
        };
        assert!(matches!(
            apply_replacement(&staged, &platform).expect("Windows update should be deferred"),
            ReplacementOutcome::DeferredForWindows { .. }
        ));
    }

    #[cfg(unix)]
    fn replacement_fixture(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        for attempt in 0..100u32 {
            let root = std::env::temp_dir().join(format!(
                "allp-replacement-{label}-{}-{nonce}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => return root,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("fixture directory should be created: {error}"),
            }
        }
        panic!("could not allocate a unique replacement fixture directory")
    }

    #[cfg(unix)]
    fn write_version_script(path: &Path, version: &str, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, format!("#!/bin/sh\nprintf 'allp {version}\\n'\n"))
            .expect("version fixture should be written");
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .expect("version fixture should be executable");
    }
}
