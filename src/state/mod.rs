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
    write_file_atomically(path, |file| {
        serde_json::to_writer_pretty(&mut *file, value).map_err(|error| AllpError::Parse {
            backend: "Allp state".to_owned(),
            message: error.to_string(),
        })?;
        file.write_all(b"\n")?;
        Ok(())
    })
}

pub(crate) fn write_file_atomically(
    path: &Path,
    write_contents: impl FnOnce(&mut fs::File) -> AllpResult<()>,
) -> AllpResult<()> {
    let parent = path.parent().ok_or_else(|| {
        AllpError::InvalidInput(format!("state path has no parent: {}", path.display()))
    })?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state");
    let (temporary, mut file) = create_temporary_file(parent, file_name)?;
    let result = (|| -> AllpResult<()> {
        write_contents(&mut file)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    let result = result.and_then(|()| fs::rename(&temporary, path).map_err(Into::into));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn create_temporary_file(
    parent: &Path,
    file_name: &str,
) -> AllpResult<(std::path::PathBuf, fs::File)> {
    for attempt in 0..100u32 {
        let path = parent.join(format!(".{file_name}.tmp-{}-{attempt}", std::process::id()));
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AllpError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique state staging path",
    )))
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
    fn failed_staging_preserves_the_previous_file() {
        let root = std::env::temp_dir().join(format!(
            "allp-state-failure-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = root.join("state.json");
        write_json_atomically(&path, &Fixture { value: 1 }).unwrap();
        let result = write_file_atomically(&path, |file| {
            file.write_all(b"{")?;
            Err(AllpError::InvalidInput("fixture write failed".to_owned()))
        });
        assert!(result.is_err());
        assert_eq!(
            read_json::<Fixture>(&path).unwrap(),
            Some(Fixture { value: 1 })
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
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
