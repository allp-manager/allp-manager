use crate::domain::{AllpError, AllpResult};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

/// Resolves a helper used by the self-updater.
///
/// An elevated updater never trusts the first executable in its inherited PATH. Fixed system
/// locations are preferred, every candidate is canonicalized, and the selected file plus every
/// ancestor must be root-owned and not writable by group or other users.
pub fn resolve_self_update_helper(name: &str) -> AllpResult<PathBuf> {
    resolve_helper_from(
        name,
        effective_root(),
        env::var_os("PATH").as_deref(),
        &fixed_helper_directories(),
    )
}

fn resolve_helper_from(
    name: &str,
    elevated: bool,
    path: Option<&std::ffi::OsStr>,
    fixed_directories: &[PathBuf],
) -> AllpResult<PathBuf> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(AllpError::InvalidInput(format!(
            "invalid self-update helper name: {name}"
        )));
    }

    let path_directories = path
        .into_iter()
        .flat_map(env::split_paths)
        .filter(|directory| directory.is_absolute())
        .collect::<Vec<_>>();
    let directories = if elevated {
        fixed_directories
            .iter()
            .chain(path_directories.iter())
            .collect::<Vec<_>>()
    } else {
        path_directories
            .iter()
            .chain(fixed_directories.iter())
            .collect::<Vec<_>>()
    };

    let mut seen = BTreeSet::new();
    let mut rejected = Vec::new();
    for directory in directories {
        for candidate in helper_candidates(directory, name) {
            let Ok(resolved) = fs::canonicalize(&candidate) else {
                continue;
            };
            if !seen.insert(resolved.clone()) {
                continue;
            }
            let validation = if elevated {
                validate_elevated_helper(&resolved)
            } else {
                validate_regular_helper(&resolved)
            };
            match validation {
                Ok(()) => return Ok(resolved),
                Err(error) => rejected.push(format!("{}: {error}", candidate.display())),
            }
        }
    }

    let detail = if rejected.is_empty() {
        "no candidate exists in the trusted system locations or PATH".to_owned()
    } else {
        format!("rejected candidate(s): {}", rejected.join("; "))
    };
    Err(AllpError::BackendNotDetected(format!(
        "trusted {name} helper required for self-update was not found; {detail}"
    )))
}

fn validate_regular_helper(path: &Path) -> AllpResult<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AllpError::InvalidInput(format!(
            "helper is not a regular file: {}",
            path.display()
        )));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(AllpError::InvalidInput(format!(
            "helper is not executable: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_elevated_helper(path: &Path) -> AllpResult<()> {
    validate_regular_helper(path)?;
    #[cfg(unix)]
    {
        let metadata = fs::metadata(path)?;
        if metadata.uid() != 0 || metadata.permissions().mode() & 0o022 != 0 {
            return Err(AllpError::InvalidInput(format!(
                "elevated helper is not root-owned and non-writable: {}",
                path.display()
            )));
        }
        let mut ancestor = path.parent();
        while let Some(directory) = ancestor {
            let metadata = fs::metadata(directory)?;
            if !metadata.is_dir()
                || metadata.uid() != 0
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(AllpError::InvalidInput(format!(
                    "elevated helper ancestor is not root-owned and non-writable: {}",
                    directory.display()
                )));
            }
            ancestor = directory.parent();
        }
    }
    Ok(())
}

fn helper_candidates(directory: &Path, name: &str) -> Vec<PathBuf> {
    let candidates = vec![directory.join(name)];
    #[cfg(windows)]
    {
        let mut candidates = candidates;
        if Path::new(name).extension().is_none() {
            candidates.extend(
                ["exe", "cmd", "bat"]
                    .map(|extension| directory.join(format!("{name}.{extension}"))),
            );
        }
        candidates
    }
    #[cfg(not(windows))]
    {
        candidates
    }
}

fn fixed_helper_directories() -> Vec<PathBuf> {
    #[cfg(unix)]
    {
        vec![
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/sbin"),
            PathBuf::from("/sbin"),
        ]
    }
    #[cfg(windows)]
    {
        env::var_os("SystemRoot")
            .map(PathBuf::from)
            .map(|root| vec![root.join("System32")])
            .unwrap_or_default()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Vec::new()
    }
}

#[cfg(unix)]
fn effective_root() -> bool {
    extern "C" {
        fn geteuid() -> u32;
    }
    // SAFETY: `geteuid` has no arguments, does not retain pointers, and is available on Unix.
    unsafe { geteuid() == 0 }
}

#[cfg(not(unix))]
fn effective_root() -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fixture(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "allp-helper-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&root).expect("helper fixture should be created");
        root
    }

    #[cfg(unix)]
    #[test]
    fn normal_user_lookup_can_use_a_canonical_path_candidate() {
        let root = fixture("normal");
        let helper = root.join("curl");
        fs::write(&helper, b"#!/bin/sh\n").expect("helper should be written");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        let path = root.as_os_str();
        assert_eq!(
            resolve_helper_from("curl", false, Some(path), &[]).unwrap(),
            fs::canonicalize(&helper).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn elevated_lookup_rejects_a_writable_path_hit() {
        let root = fixture("elevated-reject");
        let helper = root.join("tar");
        fs::write(&helper, b"#!/bin/sh\n").expect("helper should be written");
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o777)).unwrap();
        let error = resolve_helper_from("tar", true, Some(root.as_os_str()), &[])
            .expect_err("elevated lookup must reject a writable PATH executable");
        assert!(error.to_string().contains("rejected candidate"));
        fs::remove_dir_all(root).unwrap();
    }
}
