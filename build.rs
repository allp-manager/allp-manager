use std::{env, path::Path, process::Command};

fn main() {
    for name in [
        "ALLP_BASE_VERSION",
        "ALLP_BUILD_REVISION",
        "ALLP_GIT_SHA",
        "ALLP_BUILD_ID",
        "ALLP_BUILD_TIMESTAMP",
        "ALLP_BUILD_CHANNEL",
        "ALLP_BUILD_OFFICIAL",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }

    let package_version = required_env("CARGO_PKG_VERSION");
    let base_version = env::var("ALLP_BASE_VERSION").unwrap_or_else(|_| package_version.clone());
    if base_version != package_version {
        panic!(
            "ALLP_BASE_VERSION ({base_version}) must match Cargo package version ({package_version})"
        );
    }

    // Revision 1 is the repository's requested migration identity. CI always overrides this
    // with its monotonic workflow run number. A local build remains explicitly non-official.
    let revision = env::var("ALLP_BUILD_REVISION").unwrap_or_else(|_| "1".to_owned());
    let parsed_revision = revision
        .parse::<u64>()
        .expect("ALLP_BUILD_REVISION must be an unsigned integer");
    let official = env::var("ALLP_BUILD_OFFICIAL")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let channel = env::var("ALLP_BUILD_CHANNEL").unwrap_or_else(|_| {
        if official {
            "continuous".to_owned()
        } else {
            "development".to_owned()
        }
    });
    if official && channel == "continuous" && parsed_revision == 0 {
        panic!("official continuous builds require a positive ALLP_BUILD_REVISION");
    }
    if official && matches!(channel.as_str(), "stable" | "prerelease") && parsed_revision != 0 {
        panic!("official tagged releases require ALLP_BUILD_REVISION=0");
    }

    let git_sha = env::var("ALLP_GIT_SHA")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(git_head)
        .unwrap_or_else(|| "unknown".to_owned());
    if official
        && (!matches!(git_sha.len(), 40 | 64)
            || !git_sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        panic!("official builds require ALLP_GIT_SHA to contain a full hexadecimal commit ID");
    }

    let build_id = env::var("ALLP_BUILD_ID").unwrap_or_else(|_| "local".to_owned());
    let built_at = env::var("ALLP_BUILD_TIMESTAMP").unwrap_or_default();
    let target = required_env("TARGET");
    let display_version =
        if matches!(channel.as_str(), "stable" | "prerelease") && parsed_revision == 0 {
            base_version.clone()
        } else {
            format!("{base_version}.{parsed_revision}")
        };

    emit("ALLP_BASE_VERSION", &base_version);
    emit("ALLP_BUILD_REVISION", &revision);
    emit("ALLP_GIT_SHA", &git_sha);
    emit("ALLP_BUILD_ID", &build_id);
    emit("ALLP_BUILD_TIMESTAMP", &built_at);
    emit("ALLP_BUILD_CHANNEL", &channel);
    emit("ALLP_BUILD_OFFICIAL", if official { "1" } else { "0" });
    emit("ALLP_BUILD_TARGET", &target);
    emit("ALLP_DISPLAY_VERSION", &display_version);
}

fn required_env(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| panic!("Cargo did not provide required {name}"))
}

fn emit(name: &str, value: &str) {
    if value.contains('\n') || value.contains('\r') {
        panic!("{name} must not contain a newline");
    }
    println!("cargo:rustc-env={name}={value}");
}

fn git_head() -> Option<String> {
    // A source archive may be unpacked inside an unrelated checkout.
    if !Path::new(".git").exists() {
        return None;
    }
    for name in ["HEAD", "packed-refs"] {
        if let Some(path) = git_output(&["rev-parse", "--git-path", name]) {
            if Path::new(&path).exists() {
                println!("cargo:rerun-if-changed={path}");
            }
        }
    }
    if let Some(branch) = git_output(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git_output(&["rev-parse", "--git-path", &branch]) {
            // A packed branch gains a loose ref on its next commit.
            if let Some(watched) = Path::new(&path).ancestors().find(|path| path.exists()) {
                println!("cargo:rerun-if-changed={}", watched.display());
            }
        }
    }
    git_output(&["rev-parse", "--verify", "HEAD"])
}

fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}
