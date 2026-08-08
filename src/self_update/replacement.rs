use super::{
    checksum::{sha256_file, verify_sha256},
    github::validate_release_asset_url,
    trusted_helper::resolve_self_update_helper,
    ReleaseDescriptor, OFFICIAL_REPOSITORY,
};
use crate::{
    build_identity::{AllpBuildIdentity, BuildChannel},
    domain::{AllpError, AllpResult, NativeCommand},
    platform::{OperatingSystem, PlatformContext},
    release::{ReleaseAsset, Version},
};
use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::fs::DirBuilderExt;

const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 256 * 1024 * 1024;
const TRANSIENT_FS_RETRY_TIMEOUT: Duration = Duration::from_secs(5);
const TRANSIENT_FS_RETRY_DELAY: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub struct StagedRelease {
    pub version: Version,
    pub display_version: String,
    pub expected_binary: ExpectedBinary,
    pub expected_identity: Option<ExpectedBuildIdentity>,
    pub binary_path: PathBuf,
    pub staging_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedBinary {
    pub sha256: String,
    pub size: u64,
}

impl ExpectedBinary {
    fn from_path(path: &Path) -> AllpResult<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() {
            return Err(AllpError::InvalidInput(format!(
                "staged binary is not a regular file: {}",
                path.display()
            )));
        }
        let size = metadata.len();
        if size == 0 || size > MAX_BINARY_BYTES {
            return Err(AllpError::InvalidInput(format!(
                "staged binary size {size} is outside Allp's safety policy"
            )));
        }
        let sha256 = sha256_file(path)?;
        let current = fs::symlink_metadata(path)?;
        if !current.file_type().is_file() || current.len() != size {
            return Err(AllpError::InvalidInput(
                "staged binary changed while its identity was measured".to_owned(),
            ));
        }
        Ok(Self { sha256, size })
    }

    pub fn from_internal_args(sha256: &str, size: u64) -> AllpResult<Self> {
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AllpError::InvalidInput(
                "internal replacement requires a 64-hex staged binary SHA-256".to_owned(),
            ));
        }
        if size == 0 || size > MAX_BINARY_BYTES {
            return Err(AllpError::InvalidInput(format!(
                "internal replacement staged binary size {size} is outside Allp's safety policy"
            )));
        }
        Ok(Self {
            sha256: sha256.to_ascii_lowercase(),
            size,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedBuildIdentity {
    pub git_commit: String,
    pub build_id: String,
    pub target: String,
}

impl ExpectedBuildIdentity {
    fn from_published(identity: &AllpBuildIdentity) -> AllpResult<Self> {
        identity
            .validate_published()
            .map_err(AllpError::InvalidInput)?;
        if identity.channel != BuildChannel::Continuous || !identity.official {
            return Err(AllpError::InvalidInput(
                "continuous replacement requires an official continuous build identity".to_owned(),
            ));
        }
        Ok(Self {
            git_commit: identity.git_commit.clone(),
            build_id: identity.build_id.clone(),
            target: identity.target.clone(),
        })
    }

    pub fn from_internal_args(
        commit: Option<&str>,
        build_id: Option<&str>,
        target: Option<&str>,
    ) -> AllpResult<Option<Self>> {
        match (commit, build_id, target) {
            (None, None, None) => Ok(None),
            (Some(commit), Some(build_id), Some(target))
                if valid_full_git_commit(commit)
                    && !build_id.trim().is_empty()
                    && !target.trim().is_empty() =>
            {
                Ok(Some(Self {
                    git_commit: commit.to_owned(),
                    build_id: build_id.to_owned(),
                    target: target.to_owned(),
                }))
            }
            _ => Err(AllpError::InvalidInput(
                "internal continuous replacement requires a full 40- or 64-hex commit, build ID, and target together".to_owned(),
            )),
        }
    }
}

fn valid_full_git_commit(commit: &str) -> bool {
    matches!(commit.len(), 40 | 64) && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn replacement_verification_arguments(
    binary: &ExpectedBinary,
    identity: Option<&ExpectedBuildIdentity>,
) -> Vec<OsString> {
    let mut arguments = vec![
        OsString::from("--binary-sha256"),
        OsString::from(&binary.sha256),
        OsString::from("--binary-size"),
        OsString::from(binary.size.to_string()),
    ];
    if let Some(identity) = identity {
        arguments.extend([
            OsString::from("--commit"),
            OsString::from(&identity.git_commit),
            OsString::from("--build-id"),
            OsString::from(&identity.build_id),
            OsString::from("--target"),
            OsString::from(&identity.target),
        ]);
    }
    arguments
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
    let display_version = release
        .build_identity
        .as_ref()
        .map(|identity| identity.display_version())
        .unwrap_or_else(|| release.version.to_string());
    let expected_identity = release
        .build_identity
        .as_ref()
        .map(ExpectedBuildIdentity::from_published)
        .transpose()?;
    let result = (|| -> AllpResult<StagedRelease> {
        download_asset(&url, &archive_path, asset.size)?;
        verify_sha256(&archive_path, &asset.sha256)?;
        let extract_dir = staging_dir.join("extracted");
        fs::create_dir(&extract_dir)?;
        extract_archive_safely(&archive_path, &extract_dir, platform.os)?;
        let binary_path = find_staged_binary(&extract_dir, &asset.binary)?;
        let expected_binary = ExpectedBinary::from_path(&binary_path)?;
        verify_staged_binary(
            &binary_path,
            &display_version,
            &expected_binary,
            expected_identity.as_ref(),
        )?;
        Ok(StagedRelease {
            version: release.version,
            display_version: display_version.clone(),
            expected_binary,
            expected_identity: expected_identity.clone(),
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
        let mut helper = NativeCommand::new(&platform.current_executable).args([
            "internal-replace",
            "--staged",
            staged.binary_path.to_string_lossy().as_ref(),
            "--destination",
            platform.current_executable.to_string_lossy().as_ref(),
            "--version",
            &staged.display_version,
        ]);
        helper = helper.args(replacement_verification_arguments(
            &staged.expected_binary,
            staged.expected_identity.as_ref(),
        ));
        return Ok(ReplacementOutcome::RequiresElevation { command: helper });
    }
    replace_binary_atomically_verified(
        &staged.binary_path,
        &platform.current_executable,
        &staged.display_version,
        &staged.expected_binary,
        staged.expected_identity.as_ref(),
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
    verify_binary_evidence(&staged.binary_path, &staged.expected_binary)?;
    let helper = staged.staging_dir.join("allp-replace-helper.exe");
    fs::copy(&platform.current_executable, &helper)?;
    let mut command = deferred_replacement_command(&helper, staged, platform, continuation);
    command.spawn()?;
    Ok(())
}

fn deferred_replacement_command(
    helper: &Path,
    staged: &StagedRelease,
    platform: &PlatformContext,
    continuation: &[OsString],
) -> Command {
    let mut command = Command::new(helper);
    command.args([
        "internal-deferred-replace",
        "--staged",
        staged.binary_path.to_string_lossy().as_ref(),
        "--destination",
        platform.current_executable.to_string_lossy().as_ref(),
        "--version",
        &staged.display_version,
        "--cleanup-dir",
        staged.staging_dir.to_string_lossy().as_ref(),
    ]);
    command.args(replacement_verification_arguments(
        &staged.expected_binary,
        staged.expected_identity.as_ref(),
    ));
    command
        .arg("--")
        .args(continuation)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

pub fn run_deferred_replacement(
    staged: &Path,
    destination: &Path,
    expected_version: Version,
    cleanup_dir: &Path,
    continuation: &[OsString],
) -> AllpResult<()> {
    let expected_binary = ExpectedBinary::from_path(staged)?;
    run_deferred_replacement_verified(
        staged,
        destination,
        &expected_version.to_string(),
        &expected_binary,
        None,
        cleanup_dir,
        continuation,
    )
}

pub fn run_deferred_replacement_verified(
    staged: &Path,
    destination: &Path,
    expected_display_version: &str,
    expected_binary: &ExpectedBinary,
    expected_identity: Option<&ExpectedBuildIdentity>,
    cleanup_dir: &Path,
    continuation: &[OsString],
) -> AllpResult<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        match replace_binary_atomically_verified(
            staged,
            destination,
            expected_display_version,
            expected_binary,
            expected_identity,
        ) {
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
            .env(super::SELF_UPDATE_VERSION_ENV, expected_display_version)
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
    let expected_binary = ExpectedBinary::from_path(staged)?;
    replace_binary_atomically_verified(
        staged,
        destination,
        &expected_version.to_string(),
        &expected_binary,
        None,
    )
}

pub fn replace_binary_atomically_verified(
    staged: &Path,
    destination: &Path,
    expected_display_version: &str,
    expected_binary: &ExpectedBinary,
    expected_identity: Option<&ExpectedBuildIdentity>,
) -> AllpResult<()> {
    verify_binary_evidence(staged, expected_binary)?;
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
    verify_staged_binary(
        &replacement,
        expected_display_version,
        expected_binary,
        expected_identity,
    )?;

    if let Err(error) = rename_with_transient_retry(destination, &backup) {
        let _ = fs::remove_file(&replacement);
        return Err(error.into());
    }
    if let Err(error) = rename_with_transient_retry(&replacement, destination) {
        let _ = rename_with_transient_retry(&backup, destination);
        let _ = fs::remove_file(&replacement);
        return Err(error.into());
    }

    if let Err(error) = verify_staged_binary(
        destination,
        expected_display_version,
        expected_binary,
        expected_identity,
    ) {
        let failed = parent.join(format!(".{name}.failed-{}", std::process::id()));
        let failed_cleanup = remove_or_move_failed_binary(destination, &failed);
        let rollback = rename_with_transient_retry(&backup, destination);
        let _ = fs::remove_file(&failed);
        if let Err(rollback_error) = rollback {
            let cleanup_context = failed_cleanup
                .err()
                .map(|cleanup_error| {
                    format!("; failed binary cleanup also failed: {cleanup_error}")
                })
                .unwrap_or_default();
            return Err(AllpError::Io(std::io::Error::other(format!(
                "post-install verification failed ({error}); rollback also failed: {rollback_error}{cleanup_context}"
            ))));
        }
        return Err(AllpError::InvalidInput(format!(
            "post-install verification failed; the previous Allp binary was restored: {error}"
        )));
    }
    fs::remove_file(backup)?;
    Ok(())
}

fn remove_or_move_failed_binary(destination: &Path, failed: &Path) -> std::io::Result<()> {
    match remove_file_with_transient_retry(destination) {
        Ok(()) => Ok(()),
        Err(remove_error) => {
            rename_with_transient_retry(destination, failed).map_err(|rename_error| {
                std::io::Error::other(format!(
                    "remove failed: {remove_error}; rename failed: {rename_error}"
                ))
            })
        }
    }
}

fn remove_file_with_transient_retry(path: &Path) -> std::io::Result<()> {
    let deadline = Instant::now() + TRANSIENT_FS_RETRY_TIMEOUT;
    loop {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if transient_filesystem_error(&error) && Instant::now() < deadline => {
                std::thread::sleep(TRANSIENT_FS_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
}

fn rename_with_transient_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let deadline = Instant::now() + TRANSIENT_FS_RETRY_TIMEOUT;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if transient_filesystem_error(&error) && Instant::now() < deadline => {
                std::thread::sleep(TRANSIENT_FS_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
}

fn transient_filesystem_error(error: &std::io::Error) -> bool {
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
    let curl = resolve_self_update_helper("curl")?;
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
            let tar = resolve_self_update_helper("tar")?;
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
            let tar = resolve_self_update_helper("tar")?;
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

fn verify_binary_evidence(path: &Path, expected: &ExpectedBinary) -> AllpResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(AllpError::InvalidInput(format!(
            "replacement binary is not a regular file: {}",
            path.display()
        )));
    }
    if metadata.len() != expected.size {
        return Err(AllpError::InvalidInput(format!(
            "replacement binary size mismatch: expected {} bytes, found {}",
            expected.size,
            metadata.len()
        )));
    }
    verify_sha256(path, &expected.sha256).map_err(|_| {
        AllpError::InvalidInput(format!(
            "replacement binary SHA-256 mismatch for {}",
            path.display()
        ))
    })?;
    let current = fs::symlink_metadata(path)?;
    if !current.file_type().is_file() || current.len() != expected.size {
        return Err(AllpError::InvalidInput(
            "replacement binary changed during SHA-256 verification".to_owned(),
        ));
    }
    Ok(())
}

fn verify_staged_binary(
    path: &Path,
    expected_display_version: &str,
    expected_binary: &ExpectedBinary,
    expected_identity: Option<&ExpectedBuildIdentity>,
) -> AllpResult<()> {
    verify_binary_evidence(path, expected_binary)?;
    let output = version_output_with_transient_retry(path)?;
    verify_binary_evidence(path, expected_binary)?;
    if !output.status.success() {
        return Err(AllpError::InvalidInput(format!(
            "staged binary failed --version with exit code {:?}",
            output.status.code()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed = stdout
        .lines()
        .next()
        .and_then(|line| line.trim().strip_prefix("allp "))
        .map(str::trim);
    if parsed != Some(expected_display_version) {
        return Err(AllpError::InvalidInput(format!(
            "staged binary version mismatch: expected {expected_display_version}, got {}",
            parsed
                .map(str::to_owned)
                .unwrap_or_else(|| "unparseable output".to_owned())
        )));
    }
    if let Some(expected) = expected_identity {
        verify_binary_evidence(path, expected_binary)?;
        let verbose = version_output_with_transient_retry_verbose(path)?;
        verify_binary_evidence(path, expected_binary)?;
        if !verbose.status.success() {
            return Err(AllpError::InvalidInput(
                "staged binary failed verbose build-identity verification".to_owned(),
            ));
        }
        let verbose = String::from_utf8_lossy(&verbose.stdout);
        if diagnostic_value(&verbose, "Commit:") != Some(expected.git_commit.as_str()) {
            return Err(AllpError::InvalidInput(
                "staged binary Git commit does not match the continuous manifest".to_owned(),
            ));
        }
        if diagnostic_value(&verbose, "Build ID:") != Some(expected.build_id.as_str()) {
            return Err(AllpError::InvalidInput(
                "staged binary build ID does not match the continuous manifest".to_owned(),
            ));
        }
        if diagnostic_value(&verbose, "Target:") != Some(expected.target.as_str()) {
            return Err(AllpError::InvalidInput(
                "staged binary target does not match the selected continuous asset".to_owned(),
            ));
        }
        if diagnostic_value(&verbose, "Channel:") != Some("continuous") {
            return Err(AllpError::InvalidInput(
                "staged binary is not compiled for the continuous channel".to_owned(),
            ));
        }
        if diagnostic_value(&verbose, "Official build:") != Some("yes") {
            return Err(AllpError::InvalidInput(
                "staged binary is not marked as an official CI build".to_owned(),
            ));
        }
    }
    Ok(())
}

fn version_output_with_transient_retry(path: &Path) -> std::io::Result<Output> {
    let deadline = Instant::now() + TRANSIENT_FS_RETRY_TIMEOUT;
    loop {
        match Command::new(path).arg("--version").output() {
            Ok(output) => return Ok(output),
            Err(error) if transient_filesystem_error(&error) && Instant::now() < deadline => {
                std::thread::sleep(TRANSIENT_FS_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
}

fn version_output_with_transient_retry_verbose(path: &Path) -> std::io::Result<Output> {
    let deadline = Instant::now() + TRANSIENT_FS_RETRY_TIMEOUT;
    loop {
        match Command::new(path).args(["--version", "--verbose"]).output() {
            Ok(output) => return Ok(output),
            Err(error) if transient_filesystem_error(&error) && Instant::now() < deadline => {
                std::thread::sleep(TRANSIENT_FS_RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
}

fn diagnostic_value<'a>(output: &'a str, label: &str) -> Option<&'a str> {
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if line.trim() == label {
            return lines
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty());
        }
    }
    None
}

fn create_staging_directory(root: &Path, version: Version) -> AllpResult<PathBuf> {
    fs::create_dir_all(root)?;
    for attempt in 0..100u32 {
        let path = root.join(format!(
            ".allp-update-{version}-{}-{attempt}",
            std::process::id()
        ));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        match builder.create(&path) {
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

    #[test]
    fn internal_identity_requires_a_full_commit_and_preserves_all_arguments() {
        assert!(ExpectedBuildIdentity::from_internal_args(
            Some(&"a".repeat(41)),
            Some("123.1"),
            Some("x86_64-unknown-linux-gnu"),
        )
        .is_err());
        let identity = ExpectedBuildIdentity::from_internal_args(
            Some(&"b".repeat(64)),
            Some("123.1"),
            Some("x86_64-unknown-linux-gnu"),
        )
        .expect("full SHA-256 commit identity should be accepted")
        .expect("complete internal identity should be present");
        let binary = ExpectedBinary::from_internal_args(&"c".repeat(64), 123)
            .expect("complete binary evidence should be accepted");
        assert_eq!(
            replacement_verification_arguments(&binary, Some(&identity)),
            vec![
                OsString::from("--binary-sha256"),
                OsString::from("c".repeat(64)),
                OsString::from("--binary-size"),
                OsString::from("123"),
                OsString::from("--commit"),
                OsString::from("b".repeat(64)),
                OsString::from("--build-id"),
                OsString::from("123.1"),
                OsString::from("--target"),
                OsString::from("x86_64-unknown-linux-gnu"),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn staging_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let root = replacement_fixture("staging-permissions");
        let staging = create_staging_directory(&root, Version::new(0, 3, 5))
            .expect("staging directory should be created");
        assert_eq!(
            fs::metadata(&staging).unwrap().permissions().mode() & 0o777,
            0o700
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
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
        let binary = ExpectedBinary::from_path(&path).unwrap();
        let error = verify_staged_binary(&path, "0.3.4", &binary, None)
            .expect_err("mismatched binary must fail");
        assert!(error.to_string().contains("version mismatch"));
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn same_size_staged_tamper_is_rejected_before_execution_or_copy() {
        use std::os::unix::fs::PermissionsExt;
        let root = replacement_fixture("same-size-tamper");
        let destination = root.join("allp");
        let staged = root.join("staged-allp");
        let marker = root.join("executed");
        write_version_script(&destination, "0.3.3", 0o755);
        let original = format!(
            "#!/bin/sh\n/usr/bin/touch '{}'\nprintf 'allp 0.3.4\\n'\n# A\n",
            marker.display()
        );
        fs::write(&staged, &original).unwrap();
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).unwrap();
        let expected_binary = ExpectedBinary::from_path(&staged).unwrap();
        fs::write(&staged, original.replace("# A", "# B")).unwrap();

        let error = replace_binary_atomically_verified(
            &staged,
            &destination,
            "0.3.4",
            &expected_binary,
            None,
        )
        .expect_err("same-size staged tampering must fail SHA-256 verification");

        assert!(error.to_string().contains("SHA-256 mismatch"));
        assert!(!marker.exists(), "tampered staged binary must not execute");
        let output = Command::new(&destination)
            .arg("--version")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).contains("0.3.3"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn continuous_build_identity_mismatch_is_rejected_before_replacement() {
        use std::os::unix::fs::PermissionsExt;
        let root = replacement_fixture("identity-mismatch");
        let path = root.join("allp");
        fs::write(
            &path,
            b"#!/bin/sh\nif [ \"$2\" = \"--verbose\" ]; then printf 'Allp 0.3.5.2\\n\\nChannel:\\n  continuous\\n\\nCommit:\\n  bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\\n\\nBuild ID:\\n  123.1\\n\\nTarget:\\n  x86_64-unknown-linux-gnu\\n\\nOfficial build:\\n  yes\\n'; else printf 'allp 0.3.5.2\\n'; fi\n",
        )
        .expect("fixture should be written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("fixture should be executable");
        let expected = ExpectedBuildIdentity {
            git_commit: "a".repeat(40),
            build_id: "123.1".to_owned(),
            target: "x86_64-unknown-linux-gnu".to_owned(),
        };
        let binary = ExpectedBinary::from_path(&path).unwrap();
        let error = verify_staged_binary(&path, "0.3.5.2", &binary, Some(&expected))
            .expect_err("wrong compiled commit must fail");
        assert!(error.to_string().contains("Git commit"));
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[cfg(unix)]
    #[test]
    fn continuous_staged_binary_requires_full_provenance() {
        use std::os::unix::fs::PermissionsExt;
        let root = replacement_fixture("full-provenance");
        let path = root.join("allp");
        fs::write(
            &path,
            b"#!/bin/sh\nif [ \"$2\" = \"--verbose\" ]; then printf 'Allp 0.3.5.2\\n\\nChannel:\\n  continuous\\n\\nCommit:\\n  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\n\\nBuild ID:\\n  123.1\\n\\nTarget:\\n  x86_64-unknown-linux-gnu\\n\\nOfficial build:\\n  yes\\n'; else printf 'allp 0.3.5.2\\n'; fi\n",
        )
        .expect("fixture should be written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("fixture should be executable");
        let expected = ExpectedBuildIdentity {
            git_commit: "a".repeat(40),
            build_id: "123.1".to_owned(),
            target: "x86_64-unknown-linux-gnu".to_owned(),
        };
        let binary = ExpectedBinary::from_path(&path).unwrap();
        verify_staged_binary(&path, "0.3.5.2", &binary, Some(&expected))
            .expect("all provenance fields should match");

        let wrong_target = ExpectedBuildIdentity {
            target: "aarch64-unknown-linux-gnu".to_owned(),
            ..expected
        };
        let error = verify_staged_binary(&path, "0.3.5.2", &binary, Some(&wrong_target))
            .expect_err("wrong target must fail");
        assert!(error.to_string().contains("target"));
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
            display_version: "0.3.4".to_owned(),
            expected_binary: expected_binary_fixture(),
            expected_identity: None,
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
                "--binary-sha256",
                &"d".repeat(64),
                "--binary-size",
                "42",
            ]
        );
    }

    #[test]
    fn elevated_continuous_replacement_preserves_identity_expectations() {
        let mut platform = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        platform.os = OperatingSystem::Linux;
        platform.current_executable = PathBuf::from("/usr/local/bin/allp");
        platform.executable_writable = false;
        let staged = StagedRelease {
            version: Version::new(0, 3, 5),
            display_version: "0.3.5.2".to_owned(),
            expected_binary: expected_binary_fixture(),
            expected_identity: Some(ExpectedBuildIdentity {
                git_commit: "a".repeat(40),
                build_id: "123.1".to_owned(),
                target: "x86_64-unknown-linux-gnu".to_owned(),
            }),
            binary_path: PathBuf::from("/tmp/allp-staged"),
            staging_dir: PathBuf::from("/tmp/allp-staging"),
        };
        let ReplacementOutcome::RequiresElevation { command } =
            apply_replacement(&staged, &platform).expect("plan should be created")
        else {
            panic!("non-writable path should require elevation");
        };
        let arguments = command
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["--commit", &"a".repeat(40)]));
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["--build-id", "123.1"]));
        assert!(arguments
            .windows(2)
            .any(|pair| { pair == ["--target", "x86_64-unknown-linux-gnu"] }));
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["--binary-sha256", &"d".repeat(64)]));
        assert!(arguments
            .windows(2)
            .any(|pair| pair == ["--binary-size", "42"]));
    }

    #[test]
    fn windows_deferred_command_preserves_binary_and_build_expectations() {
        let mut platform = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        platform.os = OperatingSystem::Windows;
        platform.current_executable = PathBuf::from(r"C:\Program Files\Allp\allp.exe");
        let staged = StagedRelease {
            version: Version::new(0, 3, 5),
            display_version: "0.3.5.2".to_owned(),
            expected_binary: expected_binary_fixture(),
            expected_identity: Some(ExpectedBuildIdentity {
                git_commit: "a".repeat(40),
                build_id: "123.1".to_owned(),
                target: "x86_64-pc-windows-msvc".to_owned(),
            }),
            binary_path: PathBuf::from(r"C:\Temp\allp.exe"),
            staging_dir: PathBuf::from(r"C:\Temp\allp-update"),
        };
        let command = deferred_replacement_command(
            Path::new(r"C:\Temp\allp-update\allp-replace-helper.exe"),
            &staged,
            &platform,
            &[OsString::from("update")],
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for expected in [
            ["--binary-sha256", &"d".repeat(64)],
            ["--binary-size", "42"],
            ["--commit", &"a".repeat(40)],
            ["--build-id", "123.1"],
            ["--target", "x86_64-pc-windows-msvc"],
        ] {
            assert!(arguments.windows(2).any(|pair| pair == expected));
        }
    }

    #[test]
    fn windows_replacement_is_deferred() {
        let mut platform = PlatformContext::detect(&RuntimePrivilegeContext::NormalUser);
        platform.os = OperatingSystem::Windows;
        let staged = StagedRelease {
            version: Version::new(0, 3, 4),
            display_version: "0.3.4".to_owned(),
            expected_binary: expected_binary_fixture(),
            expected_identity: None,
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

    fn expected_binary_fixture() -> ExpectedBinary {
        ExpectedBinary {
            sha256: "d".repeat(64),
            size: 42,
        }
    }
}
