#![cfg(target_os = "linux")]
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "allp-migration-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(path.join("config/allp/profiles")).unwrap();
        let fixture = Self { path };
        fixture.executable("sudo", "if [ \"$1\" = -n ]; then shift; fi\nif [ \"$1\" = -v ]; then exit 0; fi\nif [ \"$1\" = -- ]; then shift; fi\nexec \"$@\"");
        fixture
    }
    fn executable(&self, name: &str, body: &str) {
        let path = self.path.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fn profile(&self, name: &str, packages: &str) {
        fs::write(
            self.profile_path(name),
            format!("version = 1\nname = \"{name}\"\n{packages}"),
        )
        .unwrap();
    }
    fn profile_path(&self, name: &str) -> PathBuf {
        self.path.join(format!("config/allp/profiles/{name}.toml"))
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_allp"))
            .args(args)
            .current_dir(&self.path)
            .env("PATH", &self.path)
            .env("XDG_CONFIG_HOME", self.path.join("config"))
            .env("XDG_STATE_HOME", self.path.join("state"))
            .env("XDG_CACHE_HOME", self.path.join("cache"))
            .env("ALLP_DISABLE_STANDARD_PATHS", "1")
            .env("ALLP_SELF_UPDATE_TEST_OFFLINE", "1")
            .env("ALLP_SNAPD_SOCKET", self.path.join("missing.socket"))
            .env("NO_COLOR", "1")
            .env("ALLP_TEST_SUDO_EXECUTABLE", self.path.join("sudo"))
            .output()
            .unwrap()
    }
    fn pacman(&self, failure: bool) {
        self.executable(
            "pacman",
            &format!(
                r#"case "$1" in
 -Q) printf 'git 1.0\nexample-bin 2.0\n' ;;
 -Qm) printf 'example-bin 2.0\n' ;;
 -Syu) printf 'pacman\n' >> '{}'; exit {} ;;
 -Ss) printf 'extra/git 1.0\n    Version control\n' ;;
esac"#,
                self.path.join("executed").display(),
                if failure { 42 } else { 0 }
            ),
        );
    }
    fn helper(&self, name: &str) {
        self.executable(
            name,
            &format!(
                r#"case "$1" in
 -Ssa) printf 'aur/example-bin 2.1 (+10 1.0)\n    Example app\n' ;;
 -Sia) printf 'Name : example-bin\nVersion : 2.1\nDescription : Example app\n' ;;
 -Sua|-Sa) printf '{}\n' >> '{}' ;;
esac"#,
                name,
                self.path.join("executed").display()
            ),
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn out(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
fn json(output: &Output) -> Value {
    assert!(output.status.success(), "{}", out(output));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn conversion_preserves_sources_and_reports_every_unmapped_item() {
    let f = Fixture::new();
    f.profile("work", "[[packages]]\nbackend = 'APT'\npackage = 'python3-pip'\nversion = '24.0'\n[[packages]]\nbackend = 'apt'\npackage = 'debian-only-tool'\n[[packages]]\nbackend = 'flatpak'\npackage = 'org.mozilla.firefox'\n[packages.flatpak]\nscope = 'system'\nbranch = 'stable'\nremote = 'custom'\n[[packages]]\nbackend = 'snap'\npackage = 'code'\n");
    let original = fs::read(f.profile_path("work")).unwrap();
    let result = json(&f.run(&["profile", "convert", "work", "--to", "arch", "--json"]));
    let report = &result["results"];
    assert_eq!(report["entries"].as_array().unwrap().len(), 4);
    assert_eq!(report["entries"][0]["target"]["package"], "python-pip");
    assert!(report["entries"][0]["target"].get("version").is_none());
    assert_eq!(report["entries"][1]["status"], "unmapped");
    assert_eq!(
        report["entries"][2]["target"]["flatpak"]["remote"],
        "custom"
    );
    assert_eq!(
        report["prerequisites"][0]["automatic_install_supported"],
        false
    );
    assert_eq!(result["complete"], false);
    let converted_profile =
        toml::from_str::<Value>(&fs::read_to_string(f.profile_path("work-arch")).unwrap()).unwrap();
    assert_eq!(converted_profile["conversion"]["omitted_packages"], 1);
    assert_eq!(fs::read(f.profile_path("work")).unwrap(), original);
    let converted = fs::read_to_string(f.profile_path("work-arch")).unwrap();
    assert!(converted.contains("python-pip"));
    assert!(!converted.contains("debian-only-tool"));
    assert!(!f.path.join("executed").exists());
    // Reverse translation is explicit and keeps both earlier inventories.
    let reverse = json(&f.run(&[
        "profile",
        "convert",
        "work-arch",
        "--to",
        "debian",
        "--json",
    ]));
    assert_eq!(
        reverse["results"]["entries"][0]["target"]["package"],
        "python3-pip"
    );
}

#[test]
fn conversion_verify_distinguishes_unavailable_packages_from_failed_queries() {
    let f = Fixture::new();
    f.pacman(false);
    f.profile("work", "[[packages]]\nbackend = 'apt'\npackage = 'git'\n[[packages]]\nbackend = 'apt'\npackage = 'missing-tool'\n");
    let result = json(&f.run(&[
        "profile", "convert", "work", "--to", "arch", "--verify", "--json",
    ]));
    assert_eq!(result["results"]["entries"][0]["status"], "verified");
    assert_eq!(result["results"]["entries"][1]["status"], "unavailable");
    f.executable("pacman", "printf 'database offline' >&2; exit 42");
    let result = json(&f.run(&[
        "profile",
        "convert",
        "work",
        "--to",
        "arch",
        "--name",
        "failed-check",
        "--verify",
        "--json",
    ]));
    assert_eq!(result["results"]["entries"][0]["status"], "check_failed");
    assert!(!f.path.join("executed").exists());
}

#[test]
fn conversion_rejects_destructive_paths_and_unsafe_or_duplicate_mappings() {
    let f = Fixture::new();
    f.profile(
        "work",
        "[[packages]]\nbackend = 'apt'\npackage = 'special'\n",
    );
    for args in [
        vec![
            "profile", "convert", "work", "--to", "arch", "--name", "work",
        ],
        vec![
            "profile",
            "convert",
            "work",
            "--to",
            "arch",
            "--report",
            "config/allp/profiles/work.toml",
        ],
    ] {
        assert_eq!(f.run(&args).status.code(), Some(2));
    }
    fs::write(
        f.path.join("map.toml"),
        "[[mappings]]\nsource_backend='apt'\nsource_package='special'\ntarget_package='-unsafe'\n",
    )
    .unwrap();
    assert_eq!(
        f.run(&["profile", "convert", "work", "--to", "arch", "--map", "map.toml"])
            .status
            .code(),
        Some(2)
    );
    fs::write(f.path.join("map.toml"), "[[mappings]]\nsource_backend='apt'\nsource_package='special'\ntarget_package='arch-special'\n").unwrap();
    let result = json(&f.run(&[
        "profile", "convert", "work", "--to", "arch", "--map", "map.toml", "--json",
    ]));
    assert_eq!(
        result["results"]["entries"][0]["target"]["package"],
        "arch-special"
    );
    assert!(!f.path.join("executed").exists());
}

#[test]
fn destination_verification_retains_apps_that_need_optional_managers_first() {
    let f = Fixture::new();
    f.pacman(false);
    f.profile("apps", "[[packages]]\nbackend='snap'\npackage='code'\n[[packages]]\nbackend='flatpak'\npackage='org.example.App'\n");
    let result = json(&f.run(&[
        "profile", "convert", "apps", "--to", "arch", "--verify", "--json",
    ]));
    assert_eq!(result["results"]["entries"][0]["status"], "preserved");
    assert_eq!(result["results"]["entries"][1]["status"], "preserved");
    assert_eq!(
        result["results"]["prerequisites"].as_array().unwrap().len(),
        2
    );
    assert_eq!(result["results"]["availability_verified"], false);
    let converted = fs::read_to_string(f.profile_path("apps-arch")).unwrap();
    assert!(converted.contains("package = \"code\""));
    assert!(converted.contains("package = \"org.example.App\""));
}

#[test]
fn capture_and_verify_keep_foreign_ownership_even_without_a_helper() {
    let f = Fixture::new();
    f.pacman(false);
    let saved = json(&f.run(&["profile", "save", "work", "--json"]));
    let rows = saved["results"]["packages"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .any(|p| p["backend"] == "aur" && p["package"] == "example-bin"));
    assert_eq!(
        json(&f.run(&["profile", "verify", "work", "--json"]))["results"]["matches"],
        true
    );
    f.helper("yay");
    f.helper("paru");
    assert_eq!(
        json(&f.run(&["profile", "verify", "work", "--json"]))["results"]["matches"],
        true
    );
    assert_eq!(
        json(&f.run(&["list", "--json"]))["results"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!f.path.join("executed").exists());
}

#[test]
fn capture_refuses_a_manager_lost_during_detection() {
    let f = Fixture::new();
    f.pacman(false);
    f.executable("flatpak", "printf 'remote database is broken' >&2; exit 42");
    let result = f.run(&["profile", "save", "work"]);
    assert_eq!(result.status.code(), Some(8), "{}", out(&result));
    assert!(!f.profile_path("work").exists());
}

#[test]
fn aur_search_is_separate_and_installer_selection_is_explicit() {
    let f = Fixture::new();
    f.pacman(false);
    f.helper("yay");
    f.helper("paru");
    let result = json(&f.run(&["search", "example-bin", "--from", "aur", "--json"]));
    let candidate = &result["results"]["candidates"][0];
    assert_eq!(candidate["backend_id"], "aur");
    assert_eq!(candidate["source"], "aur");
    let install = f.run(&["install", "example-bin", "--from", "paru", "--dry-run"]);
    assert!(install.status.success(), "{}", out(&install));
    assert!(out(&install).contains("paru -Sa -- example-bin"));
    assert!(!out(&install).contains("--noconfirm"));
    assert!(!f.path.join("executed").exists());
}

#[test]
fn default_upgrade_runs_one_aur_helper_after_pacman() {
    let f = Fixture::new();
    f.pacman(false);
    f.helper("yay");
    f.helper("paru");
    let result = json(&f.run(&["upgrade", "--dry-run", "--json"]));
    let plans = result["results"].as_array().unwrap();
    assert_eq!(plans.iter().filter(|p| p["backend_id"] == "aur").count(), 1);
    let aur = plans.iter().find(|p| p["backend_id"] == "aur").unwrap();
    assert!(aur["command"].as_str().unwrap().contains("yay -Sua"));
    assert!(!aur["command"].as_str().unwrap().contains("-Syu"));
    assert!(!f.path.join("executed").exists());
    let run = f.run(&["upgrade", "--yes", "--no-tui"]);
    assert!(run.status.success(), "{}", out(&run));
    assert_eq!(
        fs::read_to_string(f.path.join("executed")).unwrap(),
        "pacman\nyay\n"
    );
}

#[test]
fn pacman_failure_blocks_aur_even_with_stale_metadata_override() {
    let f = Fixture::new();
    f.pacman(true);
    f.helper("yay");
    let run = f.run(&["upgrade", "--yes", "--no-tui", "--allow-stale-metadata"]);
    assert_eq!(run.status.code(), Some(8), "{}", out(&run));
    assert!(
        out(&run).contains("required backend operation failed"),
        "{}",
        out(&run)
    );
    assert_eq!(
        fs::read_to_string(f.path.join("executed")).unwrap(),
        "pacman\n"
    );
}

#[test]
fn profile_optional_managers_can_be_skipped_and_bootstrap_dry_run_is_read_only() {
    let f = Fixture::new();
    f.profile(
        "apps",
        "[[packages]]\nbackend='flatpak'\npackage='org.mozilla.firefox'\n",
    );
    let skip = f.run(&[
        "profile",
        "install",
        "apps",
        "--without",
        "flatpak",
        "--yes",
    ]);
    assert!(skip.status.success(), "{}", out(&skip));
    f.executable("apt-get", "printf 'unexpected mutation' >&2; exit 42");
    let preview = f.run(&["profile", "install", "apps", "--dry-run"]);
    assert!(preview.status.success(), "{}", out(&preview));
    assert!(out(&preview).contains("install flatpak"));
    assert!(!f.path.join("executed").exists());
}

#[test]
fn flatpak_remote_setup_requires_opt_in_and_precedes_app_install() {
    let f = Fixture::new();
    f.profile("apps", "[[packages]]\nbackend='flatpak'\npackage='org.example.App'\n[packages.flatpak]\nscope='user'\nbranch='stable'\nremote='flathub'\n");
    f.executable(
        "flatpak",
        &format!(
            r#"case "$1" in
 remotes) if [ -f '{}' ]; then printf 'flathub\thttps://flathub.org/repo/\n'; fi ;;
 remote-add) printf 'remote\n' >> '{}'; /usr/bin/touch '{}' ;;
 search) printf 'org.example.App\tExample\tAn app\t1.0\tstable\tflathub\n' ;;
 install) printf 'app\n' >> '{}' ;;
esac"#,
            f.path.join("configured").display(),
            f.path.join("executed").display(),
            f.path.join("configured").display(),
            f.path.join("executed").display()
        ),
    );
    let denied = f.run(&["profile", "install", "apps", "--yes"]);
    assert_eq!(denied.status.code(), Some(2), "{}", out(&denied));
    assert!(!f.path.join("executed").exists());
    let accepted = f.run(&["profile", "install", "apps", "--yes", "--allow-bootstrap"]);
    assert!(accepted.status.success(), "{}", out(&accepted));
    assert_eq!(
        fs::read_to_string(f.path.join("executed")).unwrap(),
        "remote\napp\n"
    );
}

#[test]
fn missing_flatpak_is_installed_and_redetected_before_app_install() {
    let f = Fixture::new();
    f.profile(
        "apps",
        "[[packages]]\nbackend='flatpak'\npackage='org.example.App'\n",
    );
    f.executable(
        "apt-get",
        &format!(
            r#"if [ "$1" = install ] && [ "$2" = flatpak ]; then
 printf 'setup\n' >> '{}'
 /bin/cat > '{}' <<'FLATPAK'
#!/bin/sh
case "$1" in
 remotes) printf 'flathub\thttps://flathub.org/repo/\n' ;;
 search) printf 'org.example.App\tExample\tAn app\t1.0\tstable\tflathub\n' ;;
 install) printf 'app\n' >> '{}' ;;
esac
FLATPAK
 /bin/chmod +x '{}'
fi"#,
            f.path.join("executed").display(),
            f.path.join("flatpak").display(),
            f.path.join("executed").display(),
            f.path.join("flatpak").display()
        ),
    );
    let denied = f.run(&["profile", "install", "apps", "--yes"]);
    assert_eq!(denied.status.code(), Some(2));
    assert!(!f.path.join("executed").exists());
    let accepted = f.run(&["profile", "install", "apps", "--yes", "--allow-bootstrap"]);
    assert!(accepted.status.success(), "{}", out(&accepted));
    assert_eq!(
        fs::read_to_string(f.path.join("executed")).unwrap(),
        "setup\napp\n"
    );
}

#[test]
fn foreign_packages_can_be_removed_without_an_aur_helper() {
    let f = Fixture::new();
    f.pacman(false);
    let removal = f.run(&["remove", "example-bin", "--from", "aur", "--dry-run"]);
    assert!(removal.status.success(), "{}", out(&removal));
    assert!(out(&removal).contains("pacman -R -- example-bin"));
    let upgrade = f.run(&["upgrade", "--from", "aur", "--dry-run"]);
    assert!(upgrade.status.success(), "{}", out(&upgrade));
    assert!(out(&upgrade).contains("install a helper manually"));
    assert!(!f.path.join("executed").exists());
}
