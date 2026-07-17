use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn find_executable(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return is_executable(&path).then_some(path);
    }

    if cfg!(debug_assertions) && env::var_os("ALLP_DISABLE_STANDARD_PATHS").is_some() {
        return find_executable_in(name, env::var_os("PATH").as_deref(), &[]);
    }
    find_executable_in(
        name,
        env::var_os("PATH").as_deref(),
        &standard_executable_directories(),
    )
}

fn find_executable_in(
    name: &str,
    path: Option<&std::ffi::OsStr>,
    standard_directories: &[PathBuf],
) -> Option<PathBuf> {
    path.into_iter()
        .flat_map(env::split_paths)
        .chain(standard_directories.iter().cloned())
        .find_map(|directory| executable_in_directory(&directory, name))
}

fn executable_in_directory(directory: &Path, name: &str) -> Option<PathBuf> {
    let candidate = directory.join(name);
    if is_executable(&candidate) {
        return Some(candidate);
    }
    #[cfg(windows)]
    {
        if Path::new(name).extension().is_none() {
            for extension in ["exe", "cmd", "bat"] {
                let candidate = directory.join(format!("{name}.{extension}"));
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn standard_executable_directories() -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        vec![
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            PathBuf::from("/snap/bin"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        let mut paths = Vec::new();
        if let Some(root) = env::var_os("SystemRoot") {
            paths.push(PathBuf::from(root).join("System32"));
        }
        if let Some(files) = env::var_os("ProgramFiles") {
            paths.push(PathBuf::from(files).join("Git").join("cmd"));
        }
        paths
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Vec::new()
    }
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };

    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::find_executable;

    #[test]
    fn finds_a_common_executable_when_present() {
        let result = find_executable("sh");
        assert!(result.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn path_resolution_precedes_standard_path_fallback() {
        use super::find_executable_in;
        use std::os::unix::fs::PermissionsExt;
        use std::{ffi::OsString, fs};
        let root = std::env::temp_dir().join(format!(
            "allp-path-resolution-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path_dir = root.join("path");
        let standard_dir = root.join("standard");
        fs::create_dir_all(&path_dir).expect("PATH fixture should be created");
        fs::create_dir_all(&standard_dir).expect("standard fixture should be created");
        for directory in [&path_dir, &standard_dir] {
            let executable = directory.join("allp-test-tool");
            fs::write(&executable, b"#!/bin/sh\n").expect("fixture should be written");
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
                .expect("fixture should be executable");
        }
        let path = OsString::from(path_dir.as_os_str());
        assert_eq!(
            find_executable_in(
                "allp-test-tool",
                Some(&path),
                std::slice::from_ref(&standard_dir),
            ),
            Some(path_dir.join("allp-test-tool"))
        );
        assert_eq!(
            find_executable_in("allp-test-tool", None, std::slice::from_ref(&standard_dir),),
            Some(standard_dir.join("allp-test-tool"))
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
    }
}
