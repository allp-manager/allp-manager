//! Conservative inventory translation. Repository availability is separate from
//! package-name mapping; a failed lookup must never mean "does not exist".
use super::{
    profile_path, read_profile, validate_profile_name, write_profile, Profile, ProfilePackage,
};
use crate::{
    domain::{AllpError, AllpResult},
    operations::OperationContext,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    Debian,
    Ubuntu,
    Arch,
    Fedora,
}

impl Target {
    pub fn backend(self) -> &'static str {
        match self {
            Self::Debian | Self::Ubuntu => "apt",
            Self::Arch => "pacman",
            Self::Fedora => "dnf",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Debian => "debian",
            Self::Ubuntu => "ubuntu",
            Self::Arch => "arch",
            Self::Fedora => "fedora",
        }
    }
    fn install_command(self, package: &str) -> String {
        let verb = match self {
            Self::Arch => "pacman -S",
            Self::Fedora => "dnf install",
            _ => "apt-get install",
        };
        // This is review text, never executed. Quote it just like native plans.
        format!("sudo {verb} -- '{}'", package.replace('\'', "'\\''"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub source_backend: String,
    pub source_package: String,
    pub target_package: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingFile {
    mappings: Vec<Mapping>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Preserved,
    Mapped,
    Verified,
    Unmapped,
    Unavailable,
    CheckFailed,
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub source: ProfilePackage,
    pub target: Option<ProfilePackage>,
    pub status: Status,
    pub message: String,
    pub command: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Prerequisite {
    pub backend: String,
    pub packages: usize,
    pub automatic_install_supported: bool,
    pub guidance: String,
}

#[derive(Debug, Serialize)]
pub struct ConversionReport {
    pub version: u32,
    pub source_profile: Profile,
    pub target: Target,
    pub output_profile: String,
    pub profile_path: PathBuf,
    pub report_path: PathBuf,
    pub complete: bool,
    pub availability_checked: bool,
    pub availability_verified: bool,
    pub entries: Vec<Entry>,
    pub prerequisites: Vec<Prerequisite>,
    pub limitations: Vec<String>,
}

// Only explicit equivalences are translated offline. Dependencies, kernels,
// distribution metapackages and unknown IDs require a destination-side check
// or a user-supplied mapping instead of guessing from fuzzy search results.
const SAME_NAME: &[&str] = &[
    "git",
    "curl",
    "wget",
    "vim",
    "neovim",
    "nano",
    "htop",
    "btop",
    "tmux",
    "screen",
    "bash",
    "zsh",
    "fish",
    "jq",
    "ripgrep",
    "tree",
    "unzip",
    "zip",
    "tar",
    "gzip",
    "bzip2",
    "make",
    "cmake",
    "ninja-build",
    "gcc",
    "gdb",
    "rsync",
    "ffmpeg",
    "mpv",
    "vlc",
    "gimp",
    "inkscape",
    "keepassxc",
    "flatpak",
    "nodejs",
    "npm",
    "python3",
    "firefox",
    "thunderbird",
];

fn native_backend(backend: &str) -> bool {
    matches!(
        backend.to_ascii_lowercase().as_str(),
        "apt"
            | "pacman"
            | "aur"
            | "dnf"
            | "zypper"
            | "apk"
            | "rpm-ostree"
            | "termux"
            | "winget"
            | "xbps"
            | "eopkg"
            | "portage"
            | "swupd"
    )
}

fn mapped_id(source: &ProfilePackage, target: Target) -> Option<String> {
    if source.backend.eq_ignore_ascii_case(target.backend()) {
        return Some(source.package.clone());
    }
    if source.backend.eq_ignore_ascii_case("aur") && target == Target::Arch {
        return Some(source.package.clone());
    }
    // These mappings describe Debian/Ubuntu, Arch and Fedora only.
    let source_backend = source.backend.to_ascii_lowercase();
    if !matches!(source_backend.as_str(), "apt" | "pacman" | "dnf") {
        return None;
    }
    let package = source.package.as_str();
    let group = [
        ("python3", "python", "python3"),
        ("python3-pip", "python-pip", "python3-pip"),
        ("python3-venv", "python", "python3"),
        ("fd-find", "fd", "fd-find"),
        ("pkg-config", "pkgconf", "pkgconf-pkg-config"),
        ("ninja-build", "ninja", "ninja-build"),
    ]
    .into_iter()
    .find(|(apt, arch, dnf)| {
        package
            == match source_backend.as_str() {
                "apt" => *apt,
                "pacman" => *arch,
                _ => *dnf,
            }
    });
    if let Some((apt, arch, dnf)) = group {
        return Some(
            match target.backend() {
                "apt" => apt,
                "pacman" => arch,
                _ => dnf,
            }
            .to_owned(),
        );
    }
    SAME_NAME.contains(&package).then(|| package.to_owned())
}

pub fn convert(
    context: &OperationContext<'_>,
    name: &str,
    target: Target,
    output_name: Option<&str>,
    report_path: Option<&Path>,
    mapping_path: Option<&Path>,
    verify: bool,
) -> AllpResult<ConversionReport> {
    let source = read_profile(context.config_dir, name)?;
    let output_name = output_name
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{name}-{}", target.label()));
    validate_profile_name(&output_name)?;
    if output_name == source.name {
        return Err(AllpError::InvalidInput(
            "conversion must use a different profile name; the source inventory is preserved"
                .to_owned(),
        ));
    }
    let output_path = profile_path(context.config_dir, &output_name)?;
    if output_path.exists() {
        return Err(AllpError::InvalidInput(format!(
            "profile '{output_name}' already exists; choose a new --name"
        )));
    }
    let report_path = report_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output_path.with_extension("conversion.json"));
    for protected in [
        output_path.clone(),
        profile_path(context.config_dir, name)?,
        mapping_path.unwrap_or(&output_path).to_path_buf(),
    ] {
        if absolute_path(&report_path)? == absolute_path(&protected)? {
            return Err(AllpError::InvalidInput(
                "conversion report must not overwrite a profile or mapping file".to_owned(),
            ));
        }
    }
    let mappings = load_mappings(mapping_path)?;
    if verify && context.backends.get(target.backend()).is_none() {
        return Err(AllpError::InvalidInput(format!("--verify requires the destination backend '{}' on this system; convert without --verify to make an offline plan, or run on the destination", target.backend())));
    }
    let mut entries = Vec::new();
    let mut packages = Vec::new();
    for original in &source.packages {
        let native = native_backend(&original.backend);
        let custom = mappings.iter().find(|m| {
            m.source_backend.eq_ignore_ascii_case(&original.backend)
                && m.source_package == original.package
        });
        let mapped = if !native {
            Some(original.package.clone())
        } else {
            custom
                .map(|m| m.target_package.clone())
                .or_else(|| mapped_id(original, target))
        };
        // Unknown native names are only considered when checked against an
        // exact destination result. No fuzzy package is silently substituted.
        let candidate_id = mapped
            .clone()
            .or_else(|| verify.then(|| original.package.clone()));
        let mut entry = Entry {
            source: original.clone(), target: None, status: Status::Unmapped,
            message: format!("No known equivalent in {}; review or provide --map. This does not prove absence from its repositories.", target.label()),
            command: None,
        };
        if let Some(id) = candidate_id {
            let mut converted = original.clone();
            converted.backend.make_ascii_lowercase();
            if let Some(installer) = &mut converted.installer {
                installer.make_ascii_lowercase();
            }
            if native {
                converted.backend =
                    if original.backend.eq_ignore_ascii_case("aur") && target == Target::Arch {
                        "aur"
                    } else {
                        target.backend()
                    }
                    .to_owned();
                converted.package = id;
                converted.installer = None;
                converted.flatpak = None;
                if !original.backend.eq_ignore_ascii_case(&converted.backend) {
                    converted.version = None;
                }
                entry.command = Some(if converted.backend == "aur" {
                    format!(
                        "allp install '{}' --from aur",
                        converted.package.replace('\'', "'\\''")
                    )
                } else {
                    target.install_command(&converted.package)
                });
                entry.status = Status::Mapped;
                entry.message =
                    "Package mapping prepared; repository availability has not been checked."
                        .to_owned();
            } else {
                entry.status = Status::Preserved;
                entry.message = "Original installer and installation metadata preserved; destination availability has not been checked.".to_owned();
            }
            if verify && !native && context.backends.get(&converted.backend).is_none() {
                entry.message = format!("Destination manager '{}' is unavailable; the saved package ID is retained. Set up the manager before checking availability.", converted.backend);
            } else if verify {
                match check_package(context, &converted) {
                    Ok(true) => {
                        entry.status = Status::Verified;
                        entry.message =
                            "Exact destination package found; installed versions are not pinned."
                                .to_owned();
                    }
                    Ok(false) => {
                        entry.status = Status::Unavailable;
                        entry.message = "No exact package in the configured destination sources; not included in output profile.".to_owned();
                    }
                    Err(error) => {
                        entry.status = Status::CheckFailed;
                        entry.message = format!(
                            "Availability unknown: {error}; not included in output profile."
                        );
                    }
                }
            }
            if !matches!(entry.status, Status::Unavailable | Status::CheckFailed)
                && !packages.contains(&converted)
            {
                packages.push(converted.clone());
            }
            entry.target = Some(converted);
        }
        entries.push(entry);
    }
    let profile = Profile {
        version: super::PROFILE_VERSION,
        name: output_name.clone(),
        conversion: Some(super::ProfileConversion {
            target,
            source_packages: source.packages.len(),
            omitted_packages: entries
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.status,
                        Status::Unmapped | Status::Unavailable | Status::CheckFailed
                    )
                })
                .count(),
        }),
        packages,
    };
    profile.validate()?;
    let prerequisites = ["snap", "flatpak"].into_iter().filter_map(|backend| {
        let count = profile.packages.iter().filter(|p| p.backend == backend).count();
        (count > 0).then(|| Prerequisite {
            backend: backend.to_owned(), packages: count,
            automatic_install_supported: !(target == Target::Arch && backend == "snap"),
            guidance: if target == Target::Arch && backend == "snap" {
                "Snap on Arch requires snapd from AUR, socket activation and confinement setup. Automatic profile bootstrap of snapd via AUR is not supported; review allp install snapd --from aur first. See https://snapcraft.io/docs/tutorials/install-the-daemon/arch-linux/ ; configure manually or use profile install --without snap.".to_owned()
            } else if backend == "flatpak" {
                "Install Flatpak first, then configure the saved remote and scope. Allp prompts before setup; automation needs --yes --allow-bootstrap. Custom remotes require manual configuration.".to_owned()
            } else {
                "Install snapd first and verify the daemon is usable. Allp prompts before setup; automation needs --yes --allow-bootstrap. Service configuration may require a manual step.".to_owned()
            },
        })
    }).collect();
    let report = ConversionReport {
        version: 1, source_profile: source, target, output_profile: output_name,
        profile_path: output_path, report_path: report_path.clone(),
        complete: entries.iter().all(|e| matches!(e.status, Status::Mapped | Status::Preserved | Status::Verified)),
        availability_checked: verify,
        availability_verified: verify && entries.iter().all(|entry| entry.status == Status::Verified),
        entries, prerequisites,
        limitations: vec![
            "This converts package inventories, not the operating system, files, repositories, service configuration, drivers or application data.".to_owned(),
            "System inventories include dependencies. Review mappings before applying them; identical names are not proof of software equivalence.".to_owned(),
            "Versions are observed metadata, not pins. Snap tracking channels are not captured; restore resolves the current store channel and confinement.".to_owned(),
            "The source profile is preserved and embedded in this report, including every unresolved package. Reverse conversion is another --to operation; many-to-one mappings are not a lossless OS backup.".to_owned(),
        ],
    };
    let json =
        serde_json::to_vec_pretty(&report).map_err(|e| AllpError::InvalidInput(e.to_string()))?;
    crate::state::write_file_atomically(&report_path, |file| {
        file.write_all(&json)?;
        file.write_all(b"\n")?;
        Ok(())
    })?;
    write_profile(context.config_dir, &profile)?;
    if context.renderer.json() {
        context.renderer.render_json_envelope(
            "profile_convert",
            report.complete,
            &report,
            &[] as &[String],
        );
    } else {
        println!(
            "Converted profile '{}' -> '{}' ({})",
            name,
            profile.name,
            target.label()
        );
        for entry in &report.entries {
            println!(
                "  {}:{} -> {} [{:?}] · {}",
                entry.source.backend,
                entry.source.package,
                entry
                    .target
                    .as_ref()
                    .map(|p| format!("{}:{}", p.backend, p.package))
                    .unwrap_or_else(|| "unresolved".to_owned()),
                entry.status,
                entry.message
            );
            if let Some(command) = &entry.command {
                println!("    Command changes to: {command}");
            }
        }
        for prerequisite in &report.prerequisites {
            println!(
                "  {}: {} app(s). {}",
                prerequisite.backend, prerequisite.packages, prerequisite.guidance
            );
        }
        println!(
            "Profile: {}\nReport: {}",
            report.profile_path.display(),
            report.report_path.display()
        );
        if !verify {
            context.renderer.warn("Offline conversion: availability was not checked. Use --verify on the destination and review unresolved entries in the report.");
        }
        if !report.complete {
            context.renderer.warn("Conversion is incomplete; unresolved entries remain in the report and source profile.");
        }
    }
    Ok(report)
}

fn check_package(context: &OperationContext<'_>, package: &ProfilePackage) -> AllpResult<bool> {
    let runtime = context.backend(&package.backend)?;
    let report = runtime
        .backend
        .search(&runtime.commands, context.runner, &package.package)?;
    if !report.issues.is_empty() {
        return Err(AllpError::PartialFailure(
            report
                .issues
                .iter()
                .map(|i| i.message.as_str())
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    Ok(report.candidates.iter().any(|candidate| {
        candidate.package_id == package.package
            && package
                .installer
                .as_ref()
                .is_none_or(|i| candidate.installers.contains(i))
            && package.flatpak.as_ref().is_none_or(|f| {
                candidate.scope.as_deref() == Some(f.scope.label())
                    && f.remote
                        .as_ref()
                        .is_none_or(|r| candidate.source.as_ref() == Some(r))
                    && f.branch
                        .as_ref()
                        .is_none_or(|b| candidate.metadata.get("flatpak.branch") == Some(b))
            })
    }))
}

fn load_mappings(path: Option<&Path>) -> AllpResult<Vec<Mapping>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let parsed: MappingFile = toml::from_str(&fs::read_to_string(path)?)
        .map_err(|e| AllpError::InvalidInput(format!("invalid conversion mapping: {e}")))?;
    let mut seen = BTreeSet::new();
    for mapping in &parsed.mappings {
        for package in [&mapping.source_package, &mapping.target_package] {
            super::validate_package_entry(&ProfilePackage {
                backend: mapping.source_backend.clone(),
                package: package.clone(),
                version: None,
                installer: None,
                flatpak: None,
            })?;
            crate::operations::validate_package_id(package)?;
        }
        if !native_backend(&mapping.source_backend)
            || !seen.insert((&mapping.source_backend, &mapping.source_package))
        {
            return Err(AllpError::InvalidInput(
                "mapping source must be a unique native backend/package pair".to_owned(),
            ));
        }
    }
    Ok(parsed.mappings)
}

fn absolute_path(path: &Path) -> AllpResult<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            part => normalized.push(part.as_os_str()),
        }
    }
    if let (Some(parent), Some(file)) = (normalized.parent(), normalized.file_name()) {
        if parent.exists() {
            return Ok(parent.canonicalize()?.join(file));
        }
    }
    Ok(normalized)
}
