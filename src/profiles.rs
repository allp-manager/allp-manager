use crate::{
    domain::{AllpError, AllpResult, Capability},
    operations::{self, OperationContext},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const PROFILE_VERSION: u32 = 1;
const PROFILE_DIR: &str = "profiles";
const MAX_NAME_LEN: usize = 64;
const MAX_PACKAGES: usize = 10_000;
const MAX_PACKAGE_ID_LEN: usize = 512;
const MAX_PACKAGE_VERSION_LEN: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub name: String,

    #[serde(default)]
    pub packages: Vec<ProfilePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProfilePackage {
    pub backend: String,
    pub package: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl Profile {
    fn validate(&self) -> AllpResult<()> {
        if self.version != PROFILE_VERSION {
            return Err(AllpError::InvalidInput(format!(
                "unsupported profile version {}; supported version is {PROFILE_VERSION}",
                self.version
            )));
        }

        validate_profile_name(&self.name)?;

        if self.packages.len() > MAX_PACKAGES {
            return Err(AllpError::InvalidInput(format!(
                "profile '{}' contains {} packages; the maximum is {MAX_PACKAGES}",
                self.name,
                self.packages.len()
            )));
        }

        let mut seen = BTreeSet::new();

        for package in &self.packages {
            validate_backend_id(&package.backend)?;
            operations::validate_package_id(&package.package)?;
            validate_package_entry(package)?;

            let key = (
                package.backend.to_ascii_lowercase(),
                package.package.to_owned(),
            );

            if !seen.insert(key) {
                return Err(AllpError::InvalidInput(format!(
                    "profile '{}' contains duplicate package '{}' from backend '{}'",
                    self.name, package.package, package.backend
                )));
            }
        }

        Ok(())
    }
}

pub fn profile_path(config_dir: &Path, name: &str) -> AllpResult<PathBuf> {
    validate_profile_name(name)?;

    Ok(config_dir.join(PROFILE_DIR).join(format!("{name}.toml")))
}

pub fn save_current(context: &OperationContext<'_>, name: &str) -> AllpResult<Profile> {
    validate_profile_name(name)?;

    let report = operations::list::gather(context)?;

    if !report.complete {
        let details = report
            .issues
            .iter()
            .map(|issue| format!("  - {}: {}", issue.backend_name, issue.message))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(AllpError::PartialFailure(format!(
            "cannot save a profile because installed-package discovery was incomplete:\n\
                 {details}\n\
                 Fix these backend errors and retry; no partial profile was written."
        )));
    }

    let mut packages = report
        .packages
        .into_iter()
        .map(|package| ProfilePackage {
            backend: package.backend_id,
            package: package.package_id,
            version: package.version,
        })
        .collect::<Vec<_>>();

    packages.sort_by(|a, b| a.backend.cmp(&b.backend).then(a.package.cmp(&b.package)));

    let profile = Profile {
        version: PROFILE_VERSION,
        name: name.to_owned(),
        packages,
    };

    profile.validate()?;
    write_profile(context.config_dir, &profile)?;

    if context.renderer.json() {
        context
            .renderer
            .render_json_envelope("profile_save", true, &profile, &[] as &[String]);
    } else {
        context.renderer.success_message(&format!(
            "Saved profile '{}' with {} package(s).",
            profile.name,
            profile.packages.len()
        ));
    }

    Ok(profile)
}

pub fn list(config_dir: &Path, renderer: &crate::cli::Renderer) -> AllpResult<Vec<String>> {
    let directory = config_dir.join(PROFILE_DIR);

    let mut names = Vec::new();

    if directory.exists() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|v| v.to_str()) != Some("toml") {
                continue;
            }

            if let Some(stem) = path.file_stem().and_then(|v| v.to_str()) {
                if validate_profile_name(stem).is_ok() {
                    names.push(stem.to_owned());
                }
            }
        }
    }

    names.sort();

    if renderer.json() {
        renderer.render_json_envelope("profile_list", true, &names, &[] as &[String]);
    } else if names.is_empty() {
        renderer.info_message("No profiles found.");
    } else {
        println!("Profiles");

        for name in &names {
            println!("  {name}");
        }
    }

    Ok(names)
}

pub fn show(config_dir: &Path, name: &str, renderer: &crate::cli::Renderer) -> AllpResult<Profile> {
    let profile = read_profile(config_dir, name)?;

    if renderer.json() {
        renderer.render_json_envelope("profile_show", true, &profile, &[] as &[String]);
    } else {
        println!("Profile: {}", profile.name);
        println!("Version: {}", profile.version);
        println!("Packages: {}", profile.packages.len());

        for package in &profile.packages {
            match &package.version {
                Some(version) => {
                    println!("  {}:{} ({version})", package.backend, package.package);
                }
                None => {
                    println!("  {}:{}", package.backend, package.package);
                }
            }
        }
    }

    Ok(profile)
}

pub fn install(context: &OperationContext<'_>, name: &str) -> AllpResult<()> {
    let profile = read_profile(context.config_dir, name)?;

    if context.renderer.json() {
        return Err(AllpError::InvalidInput(
            "--json is not supported for profile install; \
             use --dry-run without --json or install packages individually"
                .to_owned(),
        ));
    }

    if profile.packages.is_empty() {
        context
            .renderer
            .info_message("Profile is empty; nothing to install.");

        return Ok(());
    }

    let unavailable = profile
        .packages
        .iter()
        .filter_map(|package| match context.backends.get(&package.backend) {
            None => Some(format!("{} (not detected)", package.backend)),
            Some(runtime) if !runtime.backend.has_capability(Capability::Install) => {
                Some(format!("{} (install unsupported)", package.backend))
            }
            Some(_) => None,
        })
        .collect::<BTreeSet<_>>();
    if !unavailable.is_empty() {
        return Err(AllpError::InvalidInput(format!(
            "profile '{}' cannot be installed on this system because these backends are unavailable: {}; no packages were installed",
            profile.name,
            unavailable.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }

    println!(
        "Installing profile '{}' ({} package(s))",
        profile.name,
        profile.packages.len()
    );

    for package in &profile.packages {
        context.renderer.info_message(&format!(
            "Profile package: {}:{}{}",
            package.backend,
            package.package,
            package
                .version
                .as_deref()
                .map(|version| format!(" ({version})"))
                .unwrap_or_default()
        ));

        let package_context = context.with_backend_filter(Some(package.backend.as_str()));

        operations::install::run(&package_context, &package.package)?;
    }

    if context.dry_run {
        context.renderer.success_message(&format!(
            "Profile '{}' dry run completed; no package was installed.",
            profile.name
        ));
    } else {
        context.renderer.success_message(&format!(
            "Profile '{}' installation completed.",
            profile.name
        ));
    }

    Ok(())
}

pub fn export(
    config_dir: &Path,
    name: &str,
    destination: &Path,
    renderer: &crate::cli::Renderer,
) -> AllpResult<()> {
    let profile = read_profile(config_dir, name)?;

    write_toml_atomically(destination, &profile)?;

    if renderer.json() {
        renderer.render_json_envelope(
            "profile_export",
            true,
            &serde_json::json!({
                "name": profile.name,
                "path": destination,
            }),
            &[] as &[String],
        );
    } else {
        renderer.success_message(&format!(
            "Exported profile '{}' to {}.",
            name,
            destination.display()
        ));
    }

    Ok(())
}

pub fn import(
    config_dir: &Path,
    source: &Path,
    name_override: Option<&str>,
    renderer: &crate::cli::Renderer,
) -> AllpResult<Profile> {
    let contents = fs::read_to_string(source)?;
    let mut profile: Profile = toml::from_str(&contents).map_err(|error| AllpError::Parse {
        backend: "Allp profile".to_owned(),
        message: error.to_string(),
    })?;

    if let Some(name) = name_override {
        validate_profile_name(name)?;
        profile.name = name.to_owned();
    }

    profile.validate()?;

    write_profile(config_dir, &profile)?;

    if renderer.json() {
        renderer.render_json_envelope("profile_import", true, &profile, &[] as &[String]);
    } else {
        renderer.success_message(&format!(
            "Imported profile '{}' with {} package(s).",
            profile.name,
            profile.packages.len()
        ));
    }

    Ok(profile)
}

fn read_profile(config_dir: &Path, name: &str) -> AllpResult<Profile> {
    let path = profile_path(config_dir, name)?;

    let contents = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AllpError::InvalidInput(format!("profile '{name}' does not exist"))
        } else {
            error.into()
        }
    })?;

    let profile: Profile = toml::from_str(&contents).map_err(|error| AllpError::Parse {
        backend: "Allp profile".to_owned(),
        message: error.to_string(),
    })?;

    profile.validate()?;

    Ok(profile)
}

fn write_profile(config_dir: &Path, profile: &Profile) -> AllpResult<()> {
    let path = profile_path(config_dir, &profile.name)?;

    write_toml_atomically(&path, profile)
}

fn write_toml_atomically(path: &Path, profile: &Profile) -> AllpResult<()> {
    let parent = path.parent().ok_or_else(|| {
        AllpError::InvalidInput(format!("profile path has no parent: {}", path.display()))
    })?;

    fs::create_dir_all(parent)?;

    let contents = toml::to_string_pretty(profile).map_err(|error| AllpError::Parse {
        backend: "Allp profile".to_owned(),
        message: error.to_string(),
    })?;

    let file_name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("profile.toml");

    let temporary = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));

    let result = (|| -> AllpResult<()> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;

        file.write_all(contents.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;

        if !path.exists() {
            fs::rename(&temporary, path)?;
            return Ok(());
        }

        let backup = parent.join(format!(".{file_name}.rollback-{}", std::process::id()));

        fs::rename(path, &backup)?;

        if let Err(error) = fs::rename(&temporary, path) {
            let rollback = fs::rename(&backup, path);

            return match rollback {
                Ok(()) => Err(error.into()),

                Err(rollback_error) => Err(AllpError::Io(std::io::Error::other(format!(
                    "profile replacement failed ({error}); \
                             rollback also failed: {rollback_error}"
                )))),
            };
        }

        fs::remove_file(backup)?;

        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }

    result
}

fn validate_profile_name(name: &str) -> AllpResult<()> {
    if name.is_empty() || name.len() > MAX_NAME_LEN || name == "." || name == ".." {
        return Err(AllpError::InvalidInput(
            "profile name must be 1-64 characters and cannot be '.' or '..'".to_owned(),
        ));
    }

    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AllpError::InvalidInput(
            "profile name may contain only ASCII letters, numbers, \
             '-', '_', and '.'"
                .to_owned(),
        ));
    }

    Ok(())
}

fn validate_backend_id(backend: &str) -> AllpResult<()> {
    if backend.is_empty()
        || backend.len() > 64
        || !backend
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AllpError::InvalidInput(format!(
            "invalid backend identifier '{backend}' in profile"
        )));
    }

    Ok(())
}

fn validate_package_entry(package: &ProfilePackage) -> AllpResult<()> {
    if package.package.len() > MAX_PACKAGE_ID_LEN
        || package.package.chars().any(char::is_whitespace)
        || package.package.chars().any(char::is_control)
    {
        return Err(AllpError::InvalidInput(format!(
            "invalid package identifier for backend '{}' in profile",
            package.backend
        )));
    }

    if let Some(version) = &package.version {
        if version.is_empty()
            || version.len() > MAX_PACKAGE_VERSION_LEN
            || version.chars().any(char::is_control)
        {
            return Err(AllpError::InvalidInput(format!(
                "invalid observed version for package '{}' in profile",
                package.package
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_profile_names() {
        assert!(validate_profile_name("../escape").is_err());

        assert!(validate_profile_name("good-profile_1").is_ok());
    }

    #[test]
    fn rejects_duplicate_backend_package_pairs() {
        let profile = Profile {
            version: PROFILE_VERSION,
            name: "dev".to_owned(),
            packages: vec![
                ProfilePackage {
                    backend: "apt".to_owned(),
                    package: "git".to_owned(),
                    version: None,
                },
                ProfilePackage {
                    backend: "apt".to_owned(),
                    package: "git".to_owned(),
                    version: None,
                },
            ],
        };

        assert!(profile.validate().is_err());
    }

    #[test]
    fn round_trips_toml() {
        let profile = Profile {
            version: PROFILE_VERSION,
            name: "rust-dev".to_owned(),
            packages: vec![ProfilePackage {
                backend: "cargo".to_owned(),
                package: "ripgrep".to_owned(),
                version: Some("14.1.0".to_owned()),
            }],
        };

        let text = toml::to_string_pretty(&profile).expect("profile should serialize");

        let decoded: Profile = toml::from_str(&text).expect("profile should deserialize");

        assert_eq!(profile, decoded);
    }

    #[test]
    fn rejects_control_characters_from_imported_profiles() {
        let profile = Profile {
            version: PROFILE_VERSION,
            name: "unsafe".to_owned(),
            packages: vec![ProfilePackage {
                backend: "apt".to_owned(),
                package: "git\u{1b}[2J".to_owned(),
                version: None,
            }],
        };

        assert!(profile.validate().is_err());
    }
}
