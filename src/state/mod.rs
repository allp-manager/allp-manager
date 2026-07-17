use crate::domain::{AllpError, AllpResult};
use serde::{de::DeserializeOwned, Serialize};
use std::{fs, io::Write, path::Path};

pub fn read_json<T: DeserializeOwned>(path: &Path) -> AllpResult<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| AllpError::Parse {
                backend: "Allp state".to_owned(),
                message: error.to_string(),
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn write_json_atomically<T: Serialize>(path: &Path, value: &T) -> AllpResult<()> {
    let parent = path.parent().ok_or_else(|| {
        AllpError::InvalidInput(format!("state path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state");
    let temporary = allocate_temporary_path(parent, file_name, "tmp")?;
    let result = (|| -> AllpResult<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        serde_json::to_writer_pretty(&mut file, value).map_err(|error| AllpError::Parse {
            backend: "Allp state".to_owned(),
            message: error.to_string(),
        })?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        replace_state_file(&temporary, path, parent, file_name)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn allocate_temporary_path(
    parent: &Path,
    file_name: &str,
    label: &str,
) -> AllpResult<std::path::PathBuf> {
    for attempt in 0..100u32 {
        let path = parent.join(format!(
            ".{file_name}.{label}-{}-{attempt}",
            std::process::id()
        ));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(AllpError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique state staging path",
    )))
}

fn replace_state_file(
    temporary: &Path,
    destination: &Path,
    parent: &Path,
    file_name: &str,
) -> AllpResult<()> {
    if !destination.exists() {
        return fs::rename(temporary, destination).map_err(Into::into);
    }
    let backup = allocate_temporary_path(parent, file_name, "rollback")?;
    fs::rename(destination, &backup)?;
    if let Err(error) = fs::rename(temporary, destination) {
        let rollback = fs::rename(&backup, destination);
        return match rollback {
            Ok(()) => Err(error.into()),
            Err(rollback_error) => Err(AllpError::Io(std::io::Error::other(format!(
                "state replacement failed ({error}); rollback also failed: {rollback_error}"
            )))),
        };
    }
    fs::remove_file(backup)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
    struct Fixture {
        value: u8,
    }

    #[test]
    fn repeated_atomic_writes_replace_previous_state() {
        let root = std::env::temp_dir().join(format!(
            "allp-state-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = root.join("state.json");
        write_json_atomically(&path, &Fixture { value: 1 }).expect("first write should work");
        write_json_atomically(&path, &Fixture { value: 2 }).expect("replacement should work");
        assert_eq!(
            read_json::<Fixture>(&path).expect("state should read"),
            Some(Fixture { value: 2 })
        );
        let names = fs::read_dir(&root)
            .expect("state directory should exist")
            .map(|entry| entry.expect("directory entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![std::ffi::OsString::from("state.json")]);
        fs::remove_dir_all(root).expect("state fixture should be removed");
    }
}
