#![cfg(target_os = "linux")]

use serde_json::Value;
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_dir(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("allp-{name}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).expect("test temp directory should be created");
    dir
}

fn write_executable(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    fs::write(&path, body).expect("fake executable should be written");
    let mut permissions = fs::metadata(&path)
        .expect("fake executable metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("fake executable should be chmodded");
}

fn run_allp(path: &Path, args: &[&str]) -> Output {
    run_allp_in(path, Path::new(env!("CARGO_MANIFEST_DIR")), args)
}

fn run_allp_in(path: &Path, current_dir: &Path, args: &[&str]) -> Output {
    run_allp_in_with_env(path, current_dir, args, &[])
}

fn run_allp_in_with_env(
    path: &Path,
    current_dir: &Path,
    args: &[&str],
    envs: &[(&str, &Path)],
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_allp"));
    command
        .args(args)
        .current_dir(current_dir)
        .env("PATH", path)
        .env("ALLP_DISABLE_STANDARD_PATHS", "1")
        .env("ALLP_SELF_UPDATE_TEST_OFFLINE", "1")
        .env("ALLP_SNAPD_SOCKET", path.join("missing-snapd.socket"))
        .env("XDG_CACHE_HOME", path.join("xdg-cache"))
        .env("XDG_STATE_HOME", path.join("xdg-state"))
        .env("XDG_CONFIG_HOME", path.join("xdg-config"))
        .env("NO_COLOR", "1");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("allp subprocess should run")
}

fn run_allp_pty(path: &Path, args: &[&str], input: &str) -> Output {
    let command_line = std::iter::once(env!("CARGO_BIN_EXE_allp"))
        .chain(args.iter().copied())
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let mut child = Command::new("/usr/bin/script")
        .args(["-qfec", &command_line, "/dev/null"])
        .env("PATH", path)
        .env("ALLP_DISABLE_STANDARD_PATHS", "1")
        .env("ALLP_SELF_UPDATE_TEST_OFFLINE", "1")
        .env("ALLP_SNAPD_SOCKET", path.join("missing-snapd.socket"))
        .env("XDG_CACHE_HOME", path.join("xdg-cache"))
        .env("XDG_STATE_HOME", path.join("xdg-state"))
        .env("XDG_CONFIG_HOME", path.join("xdg-config"))
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script should start");
    child
        .stdin
        .as_mut()
        .expect("stdin should be piped")
        .write_all(input.as_bytes())
        .expect("test input should be written");
    child
        .wait_with_output()
        .expect("script child should complete")
}

fn run_allp_with_live_self_update(path: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_allp"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", path)
        .env("ALLP_DISABLE_STANDARD_PATHS", "1")
        .env("ALLP_SNAPD_SOCKET", path.join("missing-snapd.socket"))
        .env("XDG_CACHE_HOME", path.join("xdg-cache"))
        .env("XDG_STATE_HOME", path.join("xdg-state"))
        .env("XDG_CONFIG_HOME", path.join("xdg-config"))
        .env("NO_COLOR", "1")
        .output()
        .expect("allp subprocess should run")
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._/:=@+".contains(character))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn normalized_stdout(output: &Output) -> String {
    stdout(output).replace("\r\n", "\n")
}

fn install_fake_sudo_marker(dir: &Path, marker: &Path) {
    let marker = marker.display();
    write_executable(
        dir,
        "sudo",
        &format!(
            r#"#!/bin/sh
printf '%s\n' sudo >> '{marker}'
if [ "$1" = "--" ]; then
  shift
fi
exec "$@"
"#
        ),
    );
}

fn install_fake_apt(dir: &Path, marker: &Path, update_exit: i32, search_exit: i32) {
    let marker = marker.display();
    write_executable(
        dir,
        "apt-get",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "-o" ]; then
  shift 2
fi
if [ "$1" = "update" ]; then
  printf '%s\n' apt-update >> '{marker}'
  exit {update_exit}
fi
if [ "$1" = "install" ] || [ "$1" = "remove" ]; then
  printf '%s\n' "$*" >> '{marker}'
  exit 0
fi
exit 0
"#
        ),
    );

    let mut fuzzy_lines = String::new();
    for index in 0..40 {
        fuzzy_lines.push_str(&format!(
            "printf '%s\\n' 'libtest-requires-git-{index}-perl - weak library match'\n"
        ));
    }

    write_executable(
        dir,
        "apt-cache",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "search" ]; then
  if [ {search_exit} -ne 0 ]; then
    printf '%s\n' 'search failed' >&2
    exit {search_exit}
  fi
  printf '%s\n' 'git - fast version control'
  printf '%s\n' 'git-scm - related source control tools'
  printf '%s\n' 'git-cola - graphical git client'
{fuzzy_lines}  exit 0
fi
if [ "$1" = "policy" ]; then
  printf '%s\n' '  Candidate: 1.0'
  exit 0
fi
if [ "$1" = "show" ]; then
  printf '%s\n' 'Package: git' 'Version: 1.0' 'Architecture: amd64' 'Homepage: https://git-scm.com/' 'Filename: pool/main/g/git.deb' 'Description: fast version control'
  exit 0
fi
exit 0
"#
        ),
    );

    write_executable(
        dir,
        "dpkg-query",
        r#"#!/bin/sh
if [ "$#" -gt 2 ]; then
  exit 1
fi
printf '%s\n' 'git	1.0' 'code	2.0'
"#,
    );
}

fn install_fake_flatpak_bootstrap_provider(dir: &Path, marker: &Path) {
    install_fake_apt(dir, marker, 0, 0);
    install_fake_sudo_marker(dir, marker);
    let marker = marker.display();
    let flatpak = dir.join("flatpak");
    write_executable(
        dir,
        "apt-get",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "install" ] && [ "$2" = "flatpak" ]; then
  printf '%s\n' 'bootstrap-flatpak' >> '{marker}'
  printf '%s\n' '#!/bin/sh' \
    'if [ "$1" = "--version" ]; then printf "%s\\n" "Flatpak 1.14.0"; exit 0; fi' \
    'if [ "$1" = "remotes" ]; then printf "%s\\n" "remotes-probed" >> "{marker}"; exit 0; fi' \
    'if [ "$1" = "remote-add" ]; then printf "%s\\n" "remote-added" >> "{marker}"; exit 0; fi' \
    'exit 0' > '{flatpak}'
  /bin/chmod 755 '{flatpak}'
  exit 0
fi
exit 0
"#,
            flatpak = flatpak.display()
        ),
    );
}

fn install_fake_apt_installed(dir: &Path, marker: &Path) {
    install_fake_apt(dir, marker, 0, 0);
    write_executable(
        dir,
        "dpkg-query",
        r#"#!/bin/sh
if [ "$#" -gt 2 ]; then
  printf '%s\n' 'install ok installed	1.0'
  exit 0
fi
printf '%s\n' 'git	1.0'
"#,
    );
}

fn install_fake_apt_busy(dir: &Path, marker: &Path) {
    let marker = marker.display();
    write_executable(
        dir,
        "apt-get",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "-o" ]; then
  shift 2
fi
if [ "$1" = "install" ]; then
  printf '%s\n' "apt-busy $*" >> '{marker}'
  printf '%s\n' 'E: Could not get lock /var/lib/dpkg/lock-frontend. It is held by process 7515 (packagekitd)' >&2
  printf '%s\n' 'E: Unable to acquire the dpkg frontend lock' >&2
  exit 100
fi
exit 0
"#
        ),
    );
    write_executable(
        dir,
        "apt-cache",
        r#"#!/bin/sh
if [ "$1" = "search" ]; then
  printf '%s\n' 'git - fast version control'
  exit 0
fi
if [ "$1" = "policy" ]; then
  printf '%s\n' '  Candidate: 1.0'
  exit 0
fi
exit 0
"#,
    );
    write_executable(
        dir,
        "dpkg-query",
        r#"#!/bin/sh
if [ "$#" -gt 2 ]; then
  exit 1
fi
printf '%s\n' 'git	1.0'
"#,
    );
}

fn install_fake_apt_phased_upgrade(dir: &Path, marker: &Path) {
    install_fake_apt(dir, marker, 0, 0);
    let marker = marker.display();
    write_executable(
        dir,
        "apt-get",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "-o" ]; then
  shift 2
fi
if [ "$1" = "update" ]; then
  printf '%s\n' "apt update native output"
  printf '%s\n' apt-update >> '{marker}'
  exit 0
fi
if [ "$1" = "upgrade" ]; then
  printf '%s\n' apt-upgrade >> '{marker}'
  printf '%s\n' 'Reading package lists...'
  printf '%s\n' 'Building dependency tree...'
  printf '%s\n' 'Calculating upgrade...'
  printf '%s\n' 'The following upgrades have been deferred due to phasing:'
  printf '%s\n' '  python3-software-properties'
  printf '%s\n' '  software-properties-common'
  printf '%s\n' '  software-properties-gtk'
  printf '%s\n' 'The following packages will be upgraded:'
  printf '%s\n' '  curl'
  printf '%s\n' '1 upgraded, 0 newly installed, 0 to remove and 3 not upgraded.'
  exit 0
fi
if [ "$1" = "install" ] || [ "$1" = "remove" ]; then
  printf '%s\n' "$*" >> '{marker}'
  exit 0
fi
exit 0
"#
        ),
    );
}

fn install_fake_snap(dir: &Path, marker: &Path, refresh_exit: i32) {
    let marker = marker.display();
    write_executable(
        dir,
        "snap",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "find" ]; then
  printf '%s\n' 'Name Version Publisher Notes Summary'
  printf '%s\n' 'git-scm 1.0 publisher - related source control tools'
  printf '%s\n' 'git-cola 2.0 publisher - graphical git client'
  exit 0
fi
if [ "$1" = "version" ]; then
  printf '%s\n' 'snap 2.0' 'snapd 2.0'
  exit 0
fi
if [ "$1" = "refresh" ]; then
  if [ {refresh_exit} -eq 0 ]; then
    printf '%s\n' 'All snaps up to date.'
  fi
  printf '%s\n' snap-refresh >> '{marker}'
  exit {refresh_exit}
fi
if [ "$1" = "list" ]; then
  printf '%s\n' 'Name Version Rev Tracking Publisher Notes'
  printf '%s\n' 'git-scm 1.0 1 latest/stable publisher -'
  exit 0
fi
if [ "$1" = "info" ]; then
  printf '%s\n' 'name: git-scm' 'version: 1.0' 'summary: related source control tools'
  exit 0
fi
exit 0
"#
        ),
    );
}

fn install_fake_snap_catalog(dir: &Path, marker: &Path) {
    let marker = marker.display();
    write_executable(
        dir,
        "snap",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "version" ]; then
  printf '%s\n' 'snap 2.0' 'snapd 2.0'
  exit 0
fi
if [ "$1" = "find" ]; then
  printf '%s\n' 'Name Version Publisher Notes Summary'
  case "$2" in
    pycharm) printf '%s\n' 'pycharm 2026.1.4 JetBrains** - Python IDE' ;;
    strict-app) printf '%s\n' 'strict-app 1.0 Example - strict app' ;;
    stale-snap) printf '%s\n' 'stale-snap 1.0 Example - stale result' ;;
    edge-only) printf '%s\n' 'edge-only 2.0 Example - edge only app' ;;
    wrong-arch) printf '%s\n' 'wrong-arch 1.0 Example - unsupported architecture' ;;
    installed-app) printf '%s\n' 'installed-app 1.0 Example - installed app' ;;
    display-title) printf '%s\n' 'display-title 1.0 Example - display title app' ;;
    multi-stable) printf '%s\n' 'multi-stable 1.0 Example - multiple stable tracks' ;;
    warn-stderr) printf '%s\n' 'warn-stderr 1.0 Example - warning app' ;;
    fail-stderr) printf '%s\n' 'fail-stderr 1.0 Example - native stderr failure' ;;
    fail-stdout) printf '%s\n' 'fail-stdout 1.0 Example - native stdout failure' ;;
    parse-bad) printf '%s\n' 'parse-bad 1.0 Example - malformed metadata' ;;
    signal-kill) printf '%s\n' 'signal-kill 1.0 Example - signal termination' ;;
    *) printf '%s\n' "$2 1.0 Example - generic snap" ;;
  esac
  exit 0
fi
if [ "$1" = "info" ]; then
  case "$2" in
    pycharm)
      printf '%s\n' \
        'name: pycharm' \
        'title: PyCharm' \
        'summary: Python IDE' \
        'publisher: JetBrains**' \
        'version: 2026.1.4' \
        'confinement: classic' \
        'architectures: amd64, arm64' \
        'channels:' \
        '  latest/stable: 2026.1.4 2026-06-01 (123) 800MB classic'
      exit 0
      ;;
    strict-app)
      printf '%s\n' \
        'name: strict-app' \
        'title: Strict App' \
        'summary: Strictly confined app' \
        'publisher: Example' \
        'version: 1.0' \
        'confinement: strict' \
        'architectures: amd64' \
        'channels:' \
        '  latest/stable: 1.0 2026-06-01 (10) 50MB strict'
      exit 0
      ;;
    stale-snap)
      printf '%s\n' 'error: snap "stale-snap" not found' >&2
      exit 1
      ;;
    edge-only)
      printf '%s\n' \
        'name: edge-only' \
        'title: Edge Only' \
        'summary: No stable release' \
        'publisher: Example' \
        'version: 2.0' \
        'confinement: strict' \
        'architectures: amd64' \
        'channels:' \
        '  latest/edge: 2.0 2026-06-01 (20) 50MB strict'
      exit 0
      ;;
    wrong-arch)
      printf '%s\n' \
        'name: wrong-arch' \
        'title: Wrong Arch' \
        'summary: Unsupported architecture' \
        'publisher: Example' \
        'version: 1.0' \
        'confinement: strict' \
        'architectures: riscv64' \
        'channels:' \
        '  latest/stable: 1.0 2026-06-01 (30) 50MB strict'
      exit 0
      ;;
    installed-app)
      printf '%s\n' \
        'name: installed-app' \
        'title: Installed App' \
        'summary: Already installed app' \
        'publisher: Example' \
        'version: 1.1' \
        'confinement: strict' \
        'architectures: amd64' \
        'channels:' \
        '  latest/stable: 1.1 2026-06-01 (11) 50MB strict'
      exit 0
      ;;
    display-title)
      printf '%s\n' \
        'name: canonical-title' \
        'title: Display Title' \
        'summary: Canonical name differs from display title' \
        'publisher: Example' \
        'version: 1.0' \
        'confinement: strict' \
        'architectures: amd64' \
        'channels:' \
        '  latest/stable: 1.0 2026-06-01 (12) 50MB strict'
      exit 0
      ;;
    multi-stable)
      printf '%s\n' \
        'name: multi-stable' \
        'title: Multi Stable' \
        'summary: Multiple stable tracks' \
        'publisher: Example' \
        'version: 1.0' \
        'confinement: strict' \
        'architectures: amd64' \
        'channels:' \
        '  1/stable: 1.0 2026-06-01 (13) 50MB strict' \
        '  2/stable: 2.0 2026-06-01 (14) 50MB strict'
      exit 0
      ;;
    warn-stderr)
      printf '%s\n' 'warning: using cached Snap metadata' >&2
      printf '%s\n' \
        'name: warn-stderr' \
        'title: Warning App' \
        'summary: Valid metadata with warning' \
        'publisher: Example' \
        'version: 1.0' \
        'confinement: strict' \
        'architectures: amd64' \
        'channels:' \
        '  latest/stable: 1.0 2026-06-01 (15) 50MB strict'
      exit 0
      ;;
    fail-stderr)
      printf '%s\n' 'native stderr validation failure' >&2
      exit 2
      ;;
    fail-stdout)
      printf '%s\n' 'native stdout validation failure'
      exit 3
      ;;
    parse-bad)
      printf '%s\n' 'this is not snap metadata'
      exit 0
      ;;
    signal-kill)
      kill -TERM $$
      ;;
  esac
fi
if [ "$1" = "list" ]; then
  if [ "$2" = "installed-app" ]; then
    printf '%s\n' 'Name Version Rev Tracking Publisher Notes'
    printf '%s\n' 'installed-app 1.0 10 latest/beta Example -'
    exit 0
  fi
  printf '%s\n' 'error: no matching snaps installed' >&2
  exit 1
fi
if [ "$1" = "install" ]; then
  printf '%s\n' "$*" >> '{marker}'
  exit 0
fi
if [ "$1" = "refresh" ]; then
  exit 0
fi
exit 1
"#
        ),
    );
}

fn install_fake_snap_retry_search(dir: &Path, marker: &Path) {
    let marker = marker.display();
    write_executable(
        dir,
        "snap",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "version" ]; then
  printf '%s\n' 'snap 2.0' 'snapd 2.0'
  exit 0
fi
if [ "$1" = "find" ]; then
  printf '%s\n' find >> '{marker}'
  count=$(wc -l < '{marker}')
  printf '%s\n' 'Name Version Publisher Notes Summary'
  if [ "$count" -eq 1 ]; then
    printf '%s\n' 'pycharm-stale 1.0 Example - first search result'
  else
    printf '%s\n' 'pycharm-fresh 1.0 Example - second search result'
  fi
  exit 0
fi
if [ "$1" = "info" ]; then
  printf '%s\n' 'native stderr validation failure' >&2
  exit 2
fi
if [ "$1" = "list" ]; then
  printf '%s\n' 'error: no matching snaps installed' >&2
  exit 1
fi
exit 1
"#
        ),
    );
}

fn install_fake_flatpak(dir: &Path) {
    write_executable(
        dir,
        "flatpak",
        r#"#!/bin/sh
if [ "$1" = "list" ]; then
  printf '%s\n' 'com.visualstudio.Code	Visual Studio Code	1.0	stable	flathub	user'
  exit 0
fi
if [ "$1" = "remotes" ]; then
  printf '%s\n' 'Name	URL'
  printf '%s\n' 'flathub	https://flathub.org/repo/'
  exit 0
fi
if [ "$1" = "search" ]; then
  printf '%s\n' 'Application	Name	Description	Version	Branch	Remotes'
  if [ "$2" = "telegram" ]; then
    printf '%s\n' 'org.telegram.desktop	Telegram Desktop	Fast messaging app	5.0	stable	flathub'
  else
    printf '%s\n' 'com.visualstudio.Code	Visual Studio Code	Code editor	1.0	stable	flathub'
  fi
  exit 0
fi
if [ "$1" = "info" ]; then
  printf '%s\n' 'Name: Visual Studio Code' 'Ref: com.visualstudio.Code' 'Version: 1.0'
  exit 0
fi
if [ "$1" = "update" ]; then
  printf '%s\n' 'Looking for updates...'
  printf '%s\n' 'Nothing to do.'
  exit 0
fi
exit 0
"#,
    );
}

fn install_fake_flatpak_no_remotes(dir: &Path) {
    write_executable(
        dir,
        "flatpak",
        r#"#!/bin/sh
if [ "$1" = "remotes" ]; then
  printf '%s\n' 'Name	URL'
  exit 0
fi
if [ "$1" = "search" ]; then
  printf '%s\n' 'search should not run without remotes' >&2
  exit 2
fi
exit 0
"#,
    );
}

fn install_fake_flatpak_search_failure(dir: &Path) {
    write_executable(
        dir,
        "flatpak",
        r#"#!/bin/sh
if [ "$1" = "remotes" ]; then
  printf '%s\n' 'Name	URL'
  printf '%s\n' 'flathub	https://flathub.org/repo/'
  exit 0
fi
if [ "$1" = "search" ]; then
  printf '%s\n' 'remote flathub is temporarily unavailable' >&2
  exit 1
fi
exit 0
"#,
    );
}

fn install_fake_flatpak_with_marker(dir: &Path, marker: &Path) {
    let marker = marker.display();
    write_executable(
        dir,
        "flatpak",
        &format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> '{marker}'
if [ "$1" = "remotes" ]; then
  printf '%s\n' 'Name	URL'
  printf '%s\n' 'flathub	https://flathub.org/repo/'
  exit 0
fi
if [ "$1" = "search" ]; then
  printf '%s\n' 'Application	Name	Description	Version	Branch	Remotes'
  printf '%s\n' 'org.telegram.desktop	Telegram Desktop	Fast messaging app	5.0	stable	flathub'
  exit 0
fi
exit 0
"#
        ),
    );
}

fn install_fake_linux_family_commands(dir: &Path) {
    for name in ["zypper", "apk", "emerge", "eopkg", "swupd"] {
        write_executable(
            dir,
            name,
            r#"#!/bin/sh
printf '%s\n' 'git - fake package'
exit 0
"#,
        );
    }
    for name in ["xbps-query", "xbps-install", "xbps-remove"] {
        write_executable(
            dir,
            name,
            r#"#!/bin/sh
printf '%s\n' 'git - fake package'
exit 0
"#,
        );
    }
}

fn install_fake_brew(dir: &Path) {
    write_executable(
        dir,
        "brew",
        r#"#!/bin/sh
if [ "$1" = "search" ]; then
  printf '%s\n' 'git'
  exit 0
fi
if [ "$1" = "list" ]; then
  printf '%s\n' 'git 2.0'
  exit 0
fi
if [ "$1" = "info" ]; then
  printf '%s\n' 'git: stable 2.0' 'distributed revision control'
  exit 0
fi
if [ "$1" = "install" ] || [ "$1" = "uninstall" ]; then
  exit 0
fi
if [ "$1" = "update" ] || [ "$1" = "upgrade" ]; then
  exit 0
fi
exit 0
"#,
    );
}

fn install_fake_python(dir: &Path, marker: &Path) {
    let marker = marker.display();
    write_executable(
        dir,
        "python3",
        &r#"#!/bin/sh
if [ "$1" = "-m" ] && [ "$2" = "pip" ]; then
  shift 2
  if [ "$1" = "list" ] && [ "$2" = "--outdated" ]; then
    printf '%s\n' '[{"name":"requests","version":"2.31.0","latest_version":"2.32.4"}]'
    exit 0
  fi
  if [ "$1" = "install" ] && [ "$2" = "--upgrade" ]; then
    printf '%s\n' "$*" >> '__MARKER__'
    exit 0
  fi
fi
exit 0
"#
        .replace("__MARKER__", &marker.to_string()),
    );
    write_executable(
        dir,
        "pip",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "index" ]; then
  printf '%s\n' 'openai (1.0.0)'
  printf '%s\n' 'Available versions: 1.0.0'
  exit 0
fi
if [ "$1" = "list" ]; then
  printf '%s\n' 'openai==1.0.0'
  exit 0
fi
if [ "$1" = "show" ]; then
  printf '%s\n' 'Name: openai' 'Version: 1.0.0' 'Summary: OpenAI API client'
  exit 0
fi
if [ "$1" = "install" ] || [ "$1" = "uninstall" ]; then
  printf '%s\n' "$*" >> '{marker}'
  exit 0
fi
exit 0
"#
        ),
    );
    write_executable(
        dir,
        "pipx",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "list" ] && [ "$2" = "--json" ]; then
  printf '%s\n' '{{"venvs":{{"black":{{}}}}}}'
  exit 0
fi
printf '%s\n' "pipx $*" >> '{marker}'
exit 0
"#
        ),
    );
    write_executable(
        dir,
        "uv",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "tool" ] && [ "$2" = "list" ] && [ "$3" = "--json" ]; then
  printf '%s\n' '{{"tools":{{"ruff":{{}}}}}}'
  exit 0
fi
printf '%s\n' "uv $*" >> '{marker}'
exit 0
"#
        ),
    );
}

fn install_fake_python_runtime_only(dir: &Path) {
    write_executable(
        dir,
        "python3",
        r#"#!/bin/sh
if [ "$1" = "-m" ] && [ "$2" = "pip" ]; then
  printf '%s\n' 'No module named pip' >&2
  exit 1
fi
exit 0
"#,
    );
}

fn install_fake_node_up_to_date(dir: &Path) {
    write_executable(
        dir,
        "npm",
        r#"#!/bin/sh
if [ "$1" = "search" ]; then
  printf '%s\n' '[]'
  exit 0
fi
if [ "$1" = "outdated" ]; then
  printf '%s\n' '{}'
  exit 0
fi
if [ "$1" = "--version" ]; then
  printf '%s\n' '11.9.0'
  exit 0
fi
if [ "$1" = "config" ] && [ "$2" = "get" ] && [ "$3" = "prefix" ]; then
  pwd
  exit 0
fi
if [ "$1" = "list" ]; then
  printf '%s\n' '{"dependencies":{}}'
  exit 0
fi
if [ "$1" = "view" ]; then
  printf '%s\n' '{}'
  exit 0
fi
exit 0
"#,
    );
}

fn install_fake_nvm_node_runtime(root: &Path) -> PathBuf {
    let bin = root.join(".nvm/versions/node/v20.11.1/bin");
    fs::create_dir_all(&bin).expect("fake nvm bin should be created");
    install_fake_node_up_to_date(&bin);
    write_executable(
        &bin,
        "node",
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' 'v20.11.1'
  exit 0
fi
exit 0
"#,
    );
    write_executable(
        &bin,
        "corepack",
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' '0.31.0'
  exit 0
fi
exit 0
"#,
    );
    bin
}

fn install_fake_unknown_node_runtime(dir: &Path) {
    install_fake_node_up_to_date(dir);
    write_executable(
        dir,
        "node",
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' 'v22.0.0'
  exit 0
fi
exit 0
"#,
    );
}

fn install_fake_node(dir: &Path, marker: &Path) {
    let marker = marker.display();
    let prefix = dir.display();
    write_executable(
        dir,
        "npm",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "search" ]; then
  if [ "$2" = "homebrew" ] || [ "$2" = "Homebrew" ]; then
    printf '%s\n' '[{{"name":"homebrew","version":"0.0.1","description":"Unrelated npm package"}}]'
    exit 0
  fi
  printf '%s\n' '[{{"name":"typescript","version":"5.0.0","description":"TypeScript language"}}]'
  exit 0
fi
if [ "$1" = "--version" ]; then
  printf '%s\n' '11.9.0'
  exit 0
fi
if [ "$1" = "config" ] && [ "$2" = "get" ] && [ "$3" = "prefix" ]; then
  printf '%s\n' '{prefix}'
  exit 0
fi
if [ "$1" = "list" ]; then
  printf '%s\n' '{{"dependencies":{{"typescript":{{"version":"5.0.0"}}}}}}'
  exit 0
fi
if [ "$1" = "view" ]; then
  printf '%s\n' '{{"name":"typescript","version":"5.0.0","description":"TypeScript language"}}'
  exit 0
fi
if [ "$1" = "outdated" ]; then
  printf '%s\n' '{{"typescript":{{"current":"5.0.0","wanted":"5.1.0","latest":"5.2.0"}}}}'
  exit 1
fi
if [ "$1" = "update" ]; then
  printf '%s\n' "npm $*" >> '{marker}'
  exit 0
fi
if [ "$1" = "install" ] || [ "$1" = "uninstall" ]; then
  printf '%s\n' "npm $*" >> '{marker}'
  exit 0
fi
exit 0
"#
        ),
    );
    write_executable(
        dir,
        "pnpm",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "outdated" ]; then
  printf '%s\n' '{{"typescript":{{"current":"5.0.0","latest":"5.2.0"}}}}'
  exit 1
fi
if [ "$1" = "--version" ]; then
  printf '%s\n' '9.12.0'
  exit 0
fi
printf '%s\n' "pnpm $*" >> '{marker}'
exit 0
"#
        ),
    );
    write_executable(
        dir,
        "yarn",
        &format!(
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  printf '%s\n' '1.22.22'
  exit 0
fi
printf '%s\n' "yarn $*" >> '{marker}'
exit 0
"#
        ),
    );
}

#[test]
fn discovery_is_fresh_and_backend_can_appear_after_path_change() {
    let empty = temp_dir("empty-path");
    let first = run_allp(&empty, &["detect", "--json"]);
    assert!(first.status.success());
    let first_json: Value =
        serde_json::from_slice(&first.stdout).expect("detect JSON should parse");
    assert_eq!(first_json["schema_version"], 1);
    assert!(first_json["results"]
        .as_array()
        .expect("results should be an array")
        .iter()
        .any(|entry| entry["backend_id"] == "apt" && entry["state"] == "not_found"));

    let with_apt = temp_dir("with-apt");
    install_fake_apt(&with_apt, &with_apt.join("marker"), 0, 0);
    let second = run_allp(&with_apt, &["detect", "--json"]);
    assert!(second.status.success());
    let second_json: Value =
        serde_json::from_slice(&second.stdout).expect("detect JSON should parse");
    assert!(second_json["results"]
        .as_array()
        .expect("results should be an array")
        .iter()
        .any(|entry| entry["backend_id"] == "apt" && entry["state"] == "ready"));
}

#[test]
fn discovery_drops_backend_after_path_changes() {
    let dir = temp_dir("remove-apt");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);

    let first = run_allp(&dir, &["detect", "--json"]);
    assert!(first.status.success());
    let first_json: Value =
        serde_json::from_slice(&first.stdout).expect("detect JSON should parse");
    assert!(first_json["results"]
        .as_array()
        .expect("results should be an array")
        .iter()
        .any(|entry| entry["backend_id"] == "apt" && entry["state"] == "ready"));

    fs::remove_file(dir.join("apt-get")).expect("fake apt-get should be removable");
    let second = run_allp(&dir, &["detect", "--json"]);
    assert!(second.status.success());
    let second_json: Value =
        serde_json::from_slice(&second.stdout).expect("detect JSON should parse");
    assert!(second_json["results"]
        .as_array()
        .expect("results should be an array")
        .iter()
        .any(|entry| entry["backend_id"] == "apt" && entry["state"] != "ready"));
}

#[test]
fn detect_normal_is_compact_and_verbose_contains_paths() {
    let dir = temp_dir("detect-output");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);

    let normal = run_allp(&dir, &["detect", "--no-color"]);
    assert!(normal.status.success(), "stderr: {}", stderr(&normal));
    let normal_out = stdout(&normal);
    assert!(normal_out.contains("Package Managers"));
    assert!(normal_out.contains("System Package Managers"));
    assert!(!normal_out.contains("apt-get:"));

    let verbose = run_allp(&dir, &["detect", "--verbose", "--no-color"]);
    assert!(verbose.status.success(), "stderr: {}", stderr(&verbose));
    let verbose_out = stdout(&verbose);
    assert!(verbose_out.contains("Capabilities:"));
    assert!(verbose_out.contains("apt-get"));
}

#[test]
fn unusable_snap_is_not_ready_just_because_binary_exists() {
    let dir = temp_dir("bad-snap");
    write_executable(
        &dir,
        "snap",
        r#"#!/bin/sh
printf '%s\n' 'snapd unavailable' >&2
exit 1
"#,
    );

    let output = run_allp(&dir, &["detect", "--json"]);
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).expect("detect JSON should parse");
    assert!(json["results"]
        .as_array()
        .expect("results should be an array")
        .iter()
        .any(|entry| entry["backend_id"] == "snap" && entry["state"] != "ready"));
}

#[test]
fn discovery_does_not_validate_or_invoke_sudo() {
    let dir = temp_dir("detect-no-sudo");
    let marker = dir.join("sudo-called");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);
    install_fake_sudo_marker(&dir, &marker);

    let output = run_allp(&dir, &["detect", "--json"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(!marker.exists(), "discovery must not invoke sudo");
}

#[test]
fn expanded_backend_families_are_discovered_from_fake_path() {
    let dir = temp_dir("expanded-detect");
    let marker = dir.join("marker");
    install_fake_linux_family_commands(&dir);
    install_fake_brew(&dir);
    install_fake_python(&dir, &marker);
    install_fake_node(&dir, &marker);

    let output = run_allp(&dir, &["detect", "--json"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let json: Value = serde_json::from_slice(&output.stdout).expect("detect JSON should parse");
    for backend_id in [
        "zypper", "apk", "xbps", "portage", "eopkg", "swupd", "brew", "python", "node",
    ] {
        assert!(
            json["results"]
                .as_array()
                .expect("results should be an array")
                .iter()
                .any(|entry| entry["backend_id"] == backend_id && entry["state"] == "ready"),
            "{backend_id} should be ready in fake PATH"
        );
    }
}

#[test]
fn python_from_pipx_uses_pypi_source_with_pipx_installer() {
    let dir = temp_dir("python-pipx");
    let marker = dir.join("executed");
    install_fake_python(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "openai",
            "--from",
            "pipx",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("PyPI"));
    assert!(out.contains("pipx install openai"));
    assert!(!marker.exists(), "dry run must not execute pipx");
}

#[test]
fn node_from_pnpm_uses_npm_registry_with_pnpm_installer() {
    let dir = temp_dir("node-pnpm");
    let marker = dir.join("executed");
    install_fake_node(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "typescript",
            "--from",
            "pnpm",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("npm registry"));
    assert!(out.contains("pnpm add --global typescript"));
    assert!(!marker.exists(), "dry run must not execute pnpm");
}

#[test]
fn homebrew_install_prefers_official_bootstrap_over_npm_name_collision() {
    let dir = temp_dir("homebrew-identity");
    let marker = dir.join("executed");
    install_fake_node(&dir, &marker);

    let output = run_allp(&dir, &["install", "Homebrew", "--dry-run", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Homebrew official installer"));
    assert!(out.contains("Official installer"));
    assert!(out.contains("Conflicting name"));
    assert!(out.contains("not the Homebrew package manager"));
    assert!(out.contains("Bootstrap Homebrew with the official installer"));
    assert!(out.contains("curl -fsSL"));
    assert!(out.contains("/bin/bash"));
    assert!(!out.contains("npm install --global homebrew"));
    assert!(!marker.exists(), "dry run must not install npm homebrew");
}

#[test]
fn homebrew_bootstrap_is_available_without_detected_backends() {
    let dir = temp_dir("homebrew-no-backends");

    let output = run_allp(&dir, &["install", "homebrew", "--dry-run", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Homebrew official installer"));
    assert!(out.contains("Official installer"));
    assert!(out.contains("Dry run complete"));
}

#[test]
fn explicit_npm_homebrew_collision_is_labeled_and_not_confirmed_by_yes() {
    let dir = temp_dir("npm-homebrew-collision");
    let marker = dir.join("executed");
    install_fake_node(&dir, &marker);

    let dry_run = run_allp(
        &dir,
        &[
            "install",
            "homebrew",
            "--from",
            "npm",
            "--dry-run",
            "--no-color",
        ],
    );
    assert!(dry_run.status.success(), "stderr: {}", stderr(&dry_run));
    let out = stdout(&dry_run);
    assert!(out.contains("Conflicting name"));
    assert!(out.contains("not the Homebrew package manager"));
    assert!(out.contains("npm install --global homebrew"));
    assert!(!marker.exists(), "dry run must not install npm homebrew");

    let real = run_allp(
        &dir,
        &[
            "install",
            "homebrew",
            "--from",
            "npm",
            "--yes",
            "--no-color",
        ],
    );
    assert_eq!(real.status.code(), Some(4));
    assert!(stderr(&real).contains("conflicts with Homebrew"));
    assert!(
        !marker.exists(),
        "--yes must not bypass conflicting-identity confirmation"
    );
}

#[test]
fn cross_ecosystem_exact_matches_require_selection() {
    let dir = temp_dir("cross-ecosystem");
    install_fake_apt(&dir, &dir.join("apt-marker"), 0, 0);
    install_fake_brew(&dir);

    let output = run_allp(
        &dir,
        &[
            "install",
            "git",
            "--dry-run",
            "--no-interactive",
            "--no-color",
        ],
    );

    assert_eq!(output.status.code(), Some(4));
    let err = stderr(&output);
    assert!(err.contains("Multiple install candidates"));
    assert!(err.contains("allp install git --from apt --dry-run"));
    assert!(err.contains("allp install git --from brew --dry-run"));
}

#[test]
fn scope_apps_searches_apps_and_tools_without_developer_ecosystems() {
    let dir = temp_dir("scope-apps");
    let marker = dir.join("marker");
    install_fake_apt(&dir, &marker, 0, 0);
    install_fake_snap(&dir, &marker, 0);
    install_fake_brew(&dir);
    install_fake_python(&dir, &marker);
    install_fake_node(&dir, &marker);

    let output = run_allp(&dir, &["search", "git", "--scope", "apps", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("System Packages"));
    assert!(out.contains("Universal Applications"));
    assert!(out.contains("APT"));
    assert!(out.contains("Snap"));
    assert!(out.contains("Homebrew"));
    assert!(!out.contains("Developer Ecosystems"));
    assert!(!out.contains("PyPI"));
    assert!(!out.contains("npm registry"));
}

#[test]
fn scope_dev_searches_only_python_and_node_sources() {
    let dir = temp_dir("scope-dev");
    let marker = dir.join("marker");
    install_fake_apt(&dir, &marker, 0, 0);
    install_fake_snap(&dir, &marker, 0);
    install_fake_brew(&dir);
    install_fake_python(&dir, &marker);
    install_fake_node(&dir, &marker);

    let output = run_allp(&dir, &["search", "openai", "--scope", "dev", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Developer Ecosystems"));
    assert!(out.contains("Python"));
    assert!(out.contains("PyPI"));
    assert!(!out.contains("System Packages"));
    assert!(!out.contains("Universal Applications"));
    assert!(!out.contains("APT"));
    assert!(!out.contains("Snap"));
    assert!(!out.contains("Homebrew"));
}

#[test]
fn scope_all_uses_required_group_order() {
    let dir = temp_dir("scope-all-order");
    let marker = dir.join("marker");
    install_fake_apt(&dir, &marker, 0, 0);
    install_fake_snap(&dir, &marker, 0);
    install_fake_python(&dir, &marker);

    let output = run_allp(
        &dir,
        &["search", "git", "--scope", "all", "--all", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    let system = out.find("System Packages").expect("system section");
    let universal = out
        .find("Universal Applications")
        .expect("universal section");
    let developer = out.find("Developer Ecosystems").expect("developer section");
    assert!(
        system < universal && universal < developer,
        "sections should be ordered system, universal, developer:\n{out}"
    );
}

#[test]
fn incompatible_scope_and_from_returns_clear_error() {
    let dir = temp_dir("scope-from-error");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);

    let output = run_allp(
        &dir,
        &[
            "search",
            "git",
            "--from",
            "apt",
            "--scope",
            "dev",
            "--no-color",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    let err = stderr(&output);
    assert!(err.contains("outside --scope dev"));
    assert!(err.contains("--scope all"));
}

#[test]
fn apt_search_is_bounded_and_hides_weak_fuzzy_matches_by_default() {
    let dir = temp_dir("bounded-search");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);

    let output = run_allp(&dir, &["search", "git", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("git"));
    assert!(out.contains("git-scm"));
    assert!(out.contains("Related"));
    assert!(!out.contains("libtest-requires-git"));
    assert!(
        out.lines().count() < 80,
        "search output was too large:\n{out}"
    );
}

#[test]
fn snap_related_matches_remain_visible() {
    let dir = temp_dir("snap-related");
    install_fake_snap(&dir, &dir.join("marker"), 0);

    let output = run_allp(&dir, &["search", "git", "--from", "snap", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("git-scm"));
    assert!(out.contains("git-cola"));
    assert!(out.contains("Related"));
}

#[test]
fn snap_search_publisher_verification_is_normalized() {
    let dir = temp_dir("snap-publisher-json");
    install_fake_snap_catalog(&dir, &dir.join("executed"));

    let output = run_allp(&dir, &["search", "pycharm", "--from", "snap", "--json"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let json: Value = serde_json::from_slice(&output.stdout).expect("search JSON should parse");
    let result = &json["results"][0];
    assert_eq!(result["source"], "JetBrains · Verified");
    assert_eq!(result["metadata"]["snap.publisher_name"], "JetBrains");
    assert_eq!(
        result["metadata"]["snap.publisher_verification"],
        "verified"
    );
    assert_eq!(result["metadata"]["snap.availability"], "discovered");
    assert_eq!(result["metadata"]["snap.discovery.query"], "pycharm");
}

#[test]
fn snap_discovery_rows_are_not_rendered_as_installable_exact_packages() {
    let dir = temp_dir("snap-discovery-rendering");
    install_fake_snap_catalog(&dir, &dir.join("executed"));

    let output = run_allp(&dir, &["search", "pycharm", "--from", "snap", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Exact search match"));
    assert!(out.contains("Exact search-name match · availability not yet verified"));
    assert!(out.contains("discovery version: 2026.1.4"));
    assert!(out.contains("publisher: JetBrains ✓"));
    assert!(out.contains("availability: not yet verified"));
    assert!(!out.contains("Exact package name"));
}

#[test]
fn snap_classic_install_plan_uses_info_validation_and_classic_flag() {
    let dir = temp_dir("snap-classic-plan");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "pycharm",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Package: pycharm"));
    assert!(out.contains("Software: PyCharm"));
    assert!(out.contains("Publisher: JetBrains · Verified"));
    assert!(out.contains("Channel: latest/stable"));
    assert!(out.contains("Confinement: Classic"));
    assert!(out.contains("snap install pycharm --classic"));
    assert!(!marker.exists(), "dry run must not execute snap install");
}

#[test]
fn snap_strict_install_plan_does_not_add_classic_flag() {
    let dir = temp_dir("snap-strict-plan");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "strict-app",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("snap install strict-app"));
    assert!(out.contains("Confinement: Strict"));
    assert!(!out.contains("--classic"));
    assert!(!marker.exists(), "dry run must not execute snap install");
}

#[test]
fn snap_info_canonical_name_replaces_search_result_before_planning() {
    let dir = temp_dir("snap-canonical-plan");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "display-title",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Package: canonical-title"));
    assert!(out.contains("Software: Display Title"));
    assert!(out.contains("snap install canonical-title"));
    assert!(!out.contains("snap install display-title"));
    assert!(!marker.exists(), "dry run must not execute snap install");
}

#[test]
fn snap_stale_search_result_is_blocked_before_install() {
    let dir = temp_dir("snap-stale-result");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "stale-snap",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );

    assert_eq!(output.status.code(), Some(7));
    let err = stderr(&output);
    assert!(err.contains("Snap candidate unavailable"));
    assert!(err.contains("Search status: Found"));
    assert!(err.contains("Install status: Unavailable"));
    assert!(err.contains("Resolution command:"));
    assert!(err.contains("snap info stale-snap"));
    assert!(err.contains("Resolution exit code: 1"));
    assert!(err.contains("error: snap \"stale-snap\" not found"));
    assert!(!err.contains("invalid input"));
    assert!(!marker.exists(), "stale result must not execute install");
}

#[test]
fn snap_unavailable_result_does_not_invoke_sudo_or_install() {
    let dir = temp_dir("snap-unavailable-no-sudo");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_snap_catalog(&dir, &marker);
    install_fake_sudo_marker(&dir, &sudo_marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "stale-snap",
            "--from",
            "snap",
            "--yes",
            "--no-color",
        ],
    );

    assert_eq!(output.status.code(), Some(7));
    assert!(stderr(&output).contains("Snap candidate unavailable"));
    assert!(
        !marker.exists(),
        "unavailable snap must not execute install"
    );
    assert!(
        !sudo_marker.exists(),
        "unavailable snap must not request sudo"
    );
}

#[test]
fn scope_input_followed_directly_by_package_input_has_no_stale_blank() {
    let dir = temp_dir("scope-package-no-stale-newline");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp_pty(
        &dir,
        &["install", "pycharm", "--dry-run", "--no-color"],
        "1\n1\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = normalized_stdout(&output);
    assert!(out.contains("Choose a search scope [1-3, 0 to cancel]:"));
    assert!(out.contains("Select a package [1-1, 0 to cancel]:"));
    assert!(!out.contains("Please enter a number between 1 and 1."));
    assert!(out.contains("● Validating Snap package..."));
    assert!(out.contains("snap info pycharm"));
    assert!(out.contains("snap install pycharm --classic"));
    assert!(!marker.exists(), "dry run must not execute snap install");
}

#[test]
fn explicit_blank_package_input_is_reported_once_before_accepting_selection() {
    let dir = temp_dir("scope-package-explicit-blank");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp_pty(
        &dir,
        &["install", "pycharm", "--dry-run", "--no-color"],
        "1\n\n1\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = normalized_stdout(&output);
    assert_eq!(
        out.matches("Please enter a number between 1 and 1.")
            .count(),
        1
    );
    assert!(out.contains("snap install pycharm --classic"));
    assert!(!marker.exists(), "dry run must not execute snap install");
}

#[test]
fn eof_during_package_selection_cancels_without_looping() {
    let dir = temp_dir("scope-package-eof");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp_pty(
        &dir,
        &["install", "pycharm", "--dry-run", "--no-color"],
        "1\n",
    );

    assert_eq!(output.status.code(), Some(9));
    let combined = format!("{}{}", normalized_stdout(&output), stderr(&output));
    assert!(combined.contains("input closed before a selection was made"));
    assert!(!marker.exists(), "EOF must not execute snap install");
}

#[test]
fn snap_interactive_selection_prompts_before_validation_failure() {
    let dir = temp_dir("snap-interactive-validation");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp_pty(
        &dir,
        &[
            "install",
            "stale-snap",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
        "1\n0\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = normalized_stdout(&output);
    assert!(out.contains("Select a package [1-1, 0 to cancel]:"));
    let prompt_index = out
        .find("Select a package [1-1, 0 to cancel]:")
        .expect("selection prompt should be visible");
    let validation_index = out
        .find("● Validating Snap package...")
        .expect("validation stage should be visible");
    assert!(prompt_index < validation_index);
    assert!(out[..validation_index].ends_with('\n'));
    assert!(out.contains("Command:"));
    assert!(out.contains("snap info stale-snap"));
    assert!(out.contains("✖ Snap candidate unavailable"));
    assert!(out.contains("[1] Search again"));
    assert!(out.contains("[2] Try another installer"));
    assert!(out.contains("[3] Show Snap diagnostics"));
    assert!(out.contains("[0] Cancel"));
    assert!(!out.contains("invalid input"));
    assert!(
        !marker.exists(),
        "failed validation must not execute install"
    );
}

#[test]
fn snap_single_result_prompts_and_validates_before_dry_run_plan() {
    let dir = temp_dir("snap-single-result-prompt");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp_pty(
        &dir,
        &[
            "install",
            "pycharm",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
            "-v",
        ],
        "1\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = normalized_stdout(&output);
    assert!(out.contains("Select a package [1-1, 0 to cancel]:"));
    assert!(out.contains("● Validating Snap package..."));
    assert!(out.contains("snap info pycharm"));
    assert!(out.contains("\"info\", \"pycharm\""));
    assert!(out.contains("snap install pycharm --classic"));
    assert!(!marker.exists(), "dry run must not execute snap install");
}

#[test]
fn snap_validation_actions_support_retry_diagnostics_and_cancel() {
    let dir = temp_dir("snap-validation-actions");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let diagnostics = run_allp_pty(
        &dir,
        &[
            "install",
            "stale-snap",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
        "1\n3\n0\n",
    );
    assert!(
        diagnostics.status.success(),
        "stderr: {}",
        stderr(&diagnostics)
    );
    let out = normalized_stdout(&diagnostics);
    assert!(out.contains("error: snap \"stale-snap\" not found"));
    assert!(out.contains("Snap diagnostics"));
    assert!(out.contains("Executable:"));
    assert!(out.contains("Discovery command:"));
    assert!(out.contains("Discovery exit code:"));
    assert!(out.contains("Discovery stdout:"));
    assert!(out.contains("Discovery stderr:"));
    assert!(out.contains("Exact resolution command:"));
    assert!(out.contains("Resolution exit code:"));
    assert!(out.contains("Resolution stdout:"));
    assert!(out.contains("Resolution stderr:"));
    assert!(out.contains("Candidate state:"));
    assert!(out.contains("[1] Retry validation"));
    assert!(out.contains("[2] Search again"));
    assert!(out.contains("[3] Try another installer"));

    let retry = run_allp_pty(
        &dir,
        &[
            "install",
            "stale-snap",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
        "1\n1\n1\n0\n",
    );
    assert!(retry.status.success(), "stderr: {}", stderr(&retry));
    let retry_out = normalized_stdout(&retry);
    assert!(
        retry_out
            .matches("Select a package [1-1, 0 to cancel]:")
            .count()
            >= 2
    );
    assert!(
        !marker.exists(),
        "validation actions must not execute install"
    );
}

#[test]
fn snap_try_another_installer_searches_other_backends_without_silent_selection() {
    let dir = temp_dir("snap-try-another-installer");
    let snap_marker = dir.join("snap-install");
    let flatpak_marker = dir.join("flatpak-commands");
    install_fake_snap_catalog(&dir, &snap_marker);
    install_fake_flatpak_with_marker(&dir, &flatpak_marker);

    let output = run_allp_pty(
        &dir,
        &[
            "install",
            "telegram",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
        "1\n2\n0\n",
    );

    assert_eq!(output.status.code(), Some(2));
    let out = normalized_stdout(&output);
    assert!(out.contains("[2] Try another installer"));
    assert!(out.contains("org.telegram.desktop"));
    assert!(out.contains("Select a package"));
    assert!(
        !snap_marker.exists(),
        "Try another installer must not execute Snap install"
    );
    let flatpak_commands =
        fs::read_to_string(flatpak_marker).expect("flatpak search should be attempted");
    assert!(flatpak_commands.contains("search telegram"));
}

#[test]
fn snap_validation_success_with_warning_stderr_continues_in_verbose_mode() {
    let dir = temp_dir("snap-warning-stderr-success");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "warn-stderr",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
            "-v",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Snap warning:"));
    assert!(out.contains("warning: using cached Snap metadata"));
    assert!(out.contains("snap install warn-stderr"));
    assert!(!marker.exists(), "dry run must not execute snap install");
}

#[test]
fn snap_validation_failure_reports_stderr_stdout_signal_and_parse_errors() {
    let dir = temp_dir("snap-native-failure-detail");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let stderr_failure = run_allp(
        &dir,
        &[
            "install",
            "fail-stderr",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );
    assert_eq!(stderr_failure.status.code(), Some(7));
    let err = stderr(&stderr_failure);
    assert!(err.contains("Snap candidate unavailable"));
    assert!(err.contains("Resolution command:"));
    assert!(err.contains("snap info fail-stderr"));
    assert!(err.contains("Resolution exit code: 2"));
    assert!(err.contains("native stderr validation failure"));

    let stdout_failure = run_allp(
        &dir,
        &[
            "install",
            "fail-stdout",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );
    assert_eq!(stdout_failure.status.code(), Some(7));
    let err = stderr(&stdout_failure);
    assert!(err.contains("Snap candidate unavailable"));
    assert!(err.contains("Resolution exit code: 3"));
    assert!(err.contains("native stdout validation failure"));

    let signal_failure = run_allp(
        &dir,
        &[
            "install",
            "signal-kill",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );
    assert_eq!(signal_failure.status.code(), Some(7));
    let err = stderr(&signal_failure);
    assert!(err.contains("Snap candidate unavailable"));
    assert!(err.contains("Resolution exit code: unavailable"));
    assert!(err.contains("terminated by signal"));

    let parse_failure = run_allp(
        &dir,
        &[
            "install",
            "parse-bad",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );
    assert_eq!(parse_failure.status.code(), Some(10));
    let err = stderr(&parse_failure);
    assert!(err.contains("Snap metadata parsing failed"));
    assert!(err.contains("Raw stdout:"));
    assert!(err.contains("this is not snap metadata"));
    assert!(!err.contains("Snap validation failed"));

    assert!(
        !marker.exists(),
        "failed validation must not execute install"
    );
}

#[test]
fn snap_diagnostics_include_command_exit_code_stdout_and_stderr() {
    let dir = temp_dir("snap-diagnostics-detail");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp_pty(
        &dir,
        &[
            "install",
            "fail-stderr",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
        "1\n3\n0\n",
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = normalized_stdout(&output);
    assert!(out.contains("Snap diagnostics"));
    assert!(out.contains("Executable:"));
    assert!(out.contains("Discovery command:"));
    assert!(out.contains("Exact resolution command:"));
    assert!(out.contains("snap info fail-stderr"));
    assert!(out.contains("Resolution exit code:"));
    assert!(out.contains("2"));
    assert!(out.contains("Resolution stdout:"));
    assert!(out.contains("<empty>"));
    assert!(out.contains("Resolution stderr:"));
    assert!(out.contains("native stderr validation failure"));
    assert!(out.contains("Candidate state:"));
    assert!(out.contains("[1] Retry validation"));
    assert!(out.contains("[2] Search again"));
    assert!(out.contains("[3] Try another installer"));
    assert!(out.contains("[0] Cancel"));
    assert!(!marker.exists(), "diagnostics must not execute install");
}

#[test]
fn snap_search_again_reruns_backend_and_does_not_reuse_cached_results() {
    let dir = temp_dir("snap-search-again-reruns");
    let marker = dir.join("snap-search-count");
    install_fake_snap_retry_search(&dir, &marker);

    let output = run_allp_pty(
        &dir,
        &[
            "install",
            "pycharm",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
        "1\n1\n1\n0\n",
    );

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        normalized_stdout(&output),
        stderr(&output)
    );
    let out = normalized_stdout(&output);
    assert!(out.contains("pycharm-fresh"), "{out}");
    assert_eq!(
        out.matches("Select a package [1-1, 0 to cancel]:").count(),
        2
    );
    let searches = fs::read_to_string(marker).expect("search counter should be written");
    assert_eq!(searches.lines().count(), 2);
}

#[test]
fn snap_edge_only_package_requires_explicit_channel_choice() {
    let dir = temp_dir("snap-edge-only");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "edge-only",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );

    assert_eq!(output.status.code(), Some(4));
    let err = stderr(&output);
    assert!(err.contains("no stable channel"));
    assert!(err.contains("will not silently choose"));
    assert!(
        !marker.exists(),
        "edge-only package must not execute install"
    );
}

#[test]
fn snap_multiple_stable_tracks_require_selection() {
    let dir = temp_dir("snap-multi-stable");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "multi-stable",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );

    assert_eq!(output.status.code(), Some(4));
    assert!(stderr(&output).contains("multiple stable tracks"));
    assert!(
        !marker.exists(),
        "ambiguous channel must not execute install"
    );
}

#[test]
fn snap_unsupported_architecture_blocks_install() {
    let dir = temp_dir("snap-wrong-arch");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "wrong-arch",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );

    assert_eq!(output.status.code(), Some(7));
    assert!(stderr(&output).contains("not available for architecture"));
    assert!(
        !marker.exists(),
        "unsupported architecture must not execute install"
    );
}

#[test]
fn snap_already_installed_does_not_plan_normal_install() {
    let dir = temp_dir("snap-already-installed");
    let marker = dir.join("executed");
    install_fake_snap_catalog(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "installed-app",
            "--from",
            "snap",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("installed-app is already installed"));
    assert!(out.contains("Installed version: 1.0 (latest/beta)"));
    assert!(!out.contains("snap install installed-app"));
    assert!(
        !marker.exists(),
        "already installed package must not execute"
    );
}

#[test]
fn snap_real_execution_uses_central_sudo_and_classic_plan() {
    let dir = temp_dir("snap-real-classic");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_snap_catalog(&dir, &marker);
    install_fake_sudo_marker(&dir, &sudo_marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "pycharm",
            "--from",
            "snap",
            "--yes",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let executed = fs::read_to_string(&marker).expect("fake snap install should execute");
    assert!(executed.contains("install pycharm --classic"));
    assert!(
        sudo_marker.exists(),
        "root-required snap install should use sudo"
    );
    assert!(!stdout(&output).contains("sudo -- sudo"));
    assert!(!stderr(&output).contains("sudo -- sudo"));
}

#[test]
fn small_search_limit_preserves_backend_diversity() {
    let dir = temp_dir("diverse-search");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);
    install_fake_snap(&dir, &dir.join("marker"), 0);

    let output = run_allp(&dir, &["search", "git", "--limit", "5", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("APT"));
    assert!(out.contains("Snap"));
    assert!(out.contains("git-scm"));
}

#[test]
fn exact_search_hides_related_matches() {
    let dir = temp_dir("exact-search");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);

    let output = run_allp(&dir, &["search", "git", "--exact", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("git"));
    assert!(!out.contains("git-scm"));
}

#[test]
fn install_from_backend_dry_run_constructs_plan_without_executing() {
    let dir = temp_dir("install-dry-run");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);

    let output = run_allp(
        &dir,
        &["install", "git", "--from", "apt", "--dry-run", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Action: Install system package"));
    assert!(out.contains("Command:"));
    assert!(out.contains("Privilege: Administrator access required"));
    assert!(out.contains("install -- git"));
    assert!(!marker.exists(), "dry run must not execute apt-get");
}

#[test]
fn apt_install_already_installed_does_not_plan_reinstall() {
    let dir = temp_dir("apt-already-installed");
    let marker = dir.join("executed");
    install_fake_apt_installed(&dir, &marker);

    let output = run_allp(
        &dir,
        &["install", "git", "--from", "apt", "--dry-run", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("git is already installed"));
    assert!(out.contains("Installed version: 1.0"));
    assert!(out.contains("Nothing to install."));
    assert!(!out.contains("Install system package"));
    assert!(
        !marker.exists(),
        "already installed package must not run apt"
    );
}

#[test]
fn apt_lock_contention_is_busy_not_generic_exit_failure() {
    let dir = temp_dir("apt-busy");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_apt_busy(&dir, &marker);
    install_fake_sudo_marker(&dir, &sudo_marker);

    let output = run_allp(
        &dir,
        &["install", "git", "--from", "apt", "--yes", "--no-color"],
    );

    assert_eq!(output.status.code(), Some(11));
    let err = stderr(&output);
    assert!(err.contains("APT is busy"));
    assert!(err.contains("/var/lib/dpkg/lock-frontend"));
    assert!(err.contains("packagekitd"));
    assert!(err.contains("PID: 7515"));
    assert!(err.contains("Do not remove the lock file"));
    assert!(!err.contains("APT command failed with exit code 100"));
}

#[test]
fn non_interactive_install_ambiguity_explains_recovery() {
    let dir = temp_dir("install-ambiguity");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);
    install_fake_snap(&dir, &dir.join("marker"), 0);

    let output = run_allp(&dir, &["install", "git", "--dry-run", "--no-color"]);

    assert_eq!(output.status.code(), Some(4));
    let err = stderr(&output);
    assert!(err.contains("Multiple install candidates"));
    assert!(err.contains("allp install git --from apt --dry-run"));
    assert!(err.contains("allp install git-scm --from snap --dry-run"));
}

#[test]
fn remove_ownership_ambiguity_uses_installed_inventories() {
    let dir = temp_dir("remove-ownership");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);
    install_fake_flatpak(&dir);

    let output = run_allp(&dir, &["remove", "code", "--dry-run", "--no-color"]);

    assert_eq!(output.status.code(), Some(4));
    let err = stderr(&output);
    assert!(err.contains("Multiple installed copies"));
    assert!(err.contains("allp remove code --from apt"));
    assert!(err.contains("allp remove com.visualstudio.Code --from flatpak"));
}

#[test]
fn update_dry_run_json_is_clean_and_executes_zero_commands() {
    let dir = temp_dir("update-dry-run-json");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);

    let output = run_allp(&dir, &["update", "--from", "apt", "--dry-run", "--json"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stderr(&output).is_empty());
    assert!(!marker.exists(), "dry run must not execute native command");
    let json: Value = serde_json::from_slice(&output.stdout).expect("update JSON should parse");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "update");
    assert_eq!(json["results"][0]["status"], "dry_run");
}

#[test]
fn self_update_offline_check_is_structured_and_persists_no_credentials() {
    let dir = temp_dir("self-update-offline");
    let output = run_allp(
        &dir,
        &["self-update", "--offline", "--check-only", "--json"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let json: Value =
        serde_json::from_slice(&output.stdout).expect("self-update JSON should parse");
    assert_eq!(json["status"], "offline");
    let state_path = dir.join("xdg-state/allp/self-update.json");
    let state = fs::read_to_string(state_path).expect("self-update state should be persisted");
    assert!(state.contains("update_channel"));
    assert!(!state.to_ascii_lowercase().contains("token"));
    assert!(!state.to_ascii_lowercase().contains("credential"));
}

#[test]
fn update_skip_self_update_does_not_create_self_update_state() {
    let dir = temp_dir("update-skip-self");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);
    let output = run_allp(
        &dir,
        &[
            "update",
            "--from",
            "apt",
            "--skip-self-update",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("skipped by --skip-self-update"));
    assert!(!dir.join("xdg-state/allp/self-update.json").exists());
    assert!(!marker.exists());
}

#[test]
fn update_self_only_offline_stops_before_backend_updates() {
    let dir = temp_dir("update-self-only");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);
    let output = run_allp(
        &dir,
        &[
            "update",
            "--from",
            "apt",
            "--self-only",
            "--offline",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("offline mode"));
    assert!(!marker.exists(), "--self-only must not update backends");
}

#[test]
fn completed_self_update_guard_skips_second_check_and_continues_update() {
    let dir = temp_dir("update-reexecution-guard");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);
    let output = run_allp_in_with_env(
        &dir,
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &["update", "--from", "apt", "--dry-run", "--no-color"],
        &[("ALLP_SELF_UPDATE_COMPLETED", Path::new("1"))],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("already completed in this process chain"));
    assert!(out.contains("Refresh package metadata"));
    assert!(!marker.exists());
}

#[test]
fn completed_standalone_self_update_guard_does_not_reenter_update_check() {
    let dir = temp_dir("self-update-reexecution-guard");
    let output = run_allp_in_with_env(
        &dir,
        Path::new(env!("CARGO_MANIFEST_DIR")),
        &["self-update", "--no-color"],
        &[
            ("ALLP_SELF_UPDATE_COMPLETED", Path::new("1")),
            ("ALLP_SELF_UPDATE_VERSION", Path::new("0.3.4")),
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("completed in this process chain"));
    let state = fs::read_to_string(dir.join("xdg-state/allp/self-update.json"))
        .expect("deferred completion should update state");
    assert!(state.contains("\"last_successful_version\": \"0.3.4\""));
}

#[test]
fn self_update_network_failure_can_continue_backend_dry_run() {
    let dir = temp_dir("update-network-failure");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);
    write_executable(
        &dir,
        "curl",
        "#!/bin/sh\nprintf '%s\\n' 'network unavailable' >&2\nexit 7\n",
    );
    let output = run_allp_with_live_self_update(
        &dir,
        &["update", "--from", "apt", "--dry-run", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(stderr(&output).contains("Allp self-update check failed"));
    assert!(out.contains("Refresh package metadata"));
    assert!(!marker.exists());
}

#[test]
fn self_only_network_failure_does_not_run_backends() {
    let dir = temp_dir("self-only-network-failure");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);
    write_executable(
        &dir,
        "curl",
        "#!/bin/sh\nprintf '%s\\n' 'network unavailable' >&2\nexit 7\n",
    );
    let output = run_allp_with_live_self_update(
        &dir,
        &["update", "--from", "apt", "--self-only", "--no-color"],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("network unavailable"));
    assert!(!marker.exists(), "failed --self-only must not run backends");
}

#[test]
fn update_dry_run_shows_detected_and_selected_sets() {
    let dir = temp_dir("update-selected");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);
    install_fake_snap(&dir, &marker, 0);

    let output = run_allp(
        &dir,
        &["update", "--from", "apt", "--dry-run", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(!marker.exists(), "dry run must not execute native command");
    let out = stdout(&output);
    assert!(out.contains("Environment Scan"));
    assert!(out.contains("Detected and ready: APT, Snap"));
    assert!(out.contains("Selected for execution: APT"));
    assert!(out.contains("Planned Operation"));
    assert!(out.contains("Privilege: Administrator access required"));
    assert!(out.contains("0 commands executed"));
}

#[test]
fn apt_upgrade_parses_updated_and_phased_deferred_results() {
    let dir = temp_dir("apt-phased-upgrade");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_sudo_marker(&dir, &sudo_marker);
    install_fake_apt_phased_upgrade(&dir, &marker);

    let output = run_allp(&dir, &["upgrade", "--from", "apt", "--yes", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Updated · 1 package"));
    assert!(out.contains("Deferred · 3 phased updates"));
    assert!(!out.contains("Failed"));
    let executed = fs::read_to_string(&marker).expect("fake apt upgrade should execute");
    assert!(executed.contains("apt-upgrade"));
}

#[test]
fn snap_and_flatpak_up_to_date_outputs_are_not_generic_completed() {
    let snap_dir = temp_dir("snap-up-to-date");
    let snap_marker = snap_dir.join("executed");
    let sudo_marker = snap_dir.join("sudo-called");
    install_fake_sudo_marker(&snap_dir, &sudo_marker);
    install_fake_snap(&snap_dir, &snap_marker, 0);

    let snap = run_allp(
        &snap_dir,
        &["update", "--from", "snap", "--yes", "--no-color"],
    );

    assert!(snap.status.success(), "stderr: {}", stderr(&snap));
    let snap_out = stdout(&snap);
    assert!(snap_out.contains("All snaps up to date."));
    assert!(snap_out.contains("Snap"));
    assert!(snap_out.contains("Up to date · all snaps up to date"));
    assert!(!snap_out.contains("Snap           Completed"));

    let flatpak_dir = temp_dir("flatpak-up-to-date");
    install_fake_flatpak(&flatpak_dir);

    let flatpak = run_allp(
        &flatpak_dir,
        &["update", "--from", "flatpak", "--yes", "--no-color"],
    );

    assert!(flatpak.status.success(), "stderr: {}", stderr(&flatpak));
    let flatpak_out = stdout(&flatpak);
    assert!(flatpak_out.contains("Nothing to do."));
    assert!(flatpak_out.contains("Flatpak"));
    assert!(flatpak_out.contains("Up to date · nothing to do"));
    assert!(!flatpak_out.contains("Flatpak        Completed"));
}

#[test]
fn flatpak_missing_executable_bootstrap_is_planned_before_dry_run() {
    let dir = temp_dir("flatpak-bootstrap-plan");
    let marker = dir.join("executed");
    install_fake_flatpak_bootstrap_provider(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "org.telegram.desktop",
            "--from",
            "flatpak",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Install required component flatpak"));
    assert!(out.contains("apt-get install flatpak"));
    assert!(!marker.exists(), "dry run must not bootstrap Flatpak");
}

#[test]
fn yes_without_allow_bootstrap_does_not_install_missing_flatpak() {
    let dir = temp_dir("flatpak-bootstrap-policy");
    let marker = dir.join("executed");
    install_fake_flatpak_bootstrap_provider(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "org.telegram.desktop",
            "--from",
            "flatpak",
            "--yes",
            "--no-interactive",
            "--no-color",
        ],
    );

    assert!(!output.status.success());
    assert!(stderr(&output).contains("--yes --allow-bootstrap"));
    assert!(!marker.exists(), "--yes alone must not bootstrap Flatpak");
}

#[test]
fn approved_flatpak_bootstrap_refreshes_capabilities_without_adding_flathub() {
    let dir = temp_dir("flatpak-bootstrap-refresh");
    let marker = dir.join("executed");
    install_fake_flatpak_bootstrap_provider(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "install",
            "org.telegram.desktop",
            "--from",
            "flatpak",
            "--yes",
            "--allow-bootstrap",
            "--no-interactive",
            "--no-color",
        ],
    );

    assert!(
        !output.status.success(),
        "no remote should leave no package result"
    );
    let actions = fs::read_to_string(&marker).expect("bootstrap actions should be recorded");
    assert!(actions.contains("bootstrap-flatpak"));
    assert!(actions.contains("remotes-probed"));
    assert!(!actions.contains("remote-added"));
    assert!(stderr(&output).contains("was not found"));
}

#[test]
fn flatpak_unavailable_is_reported_in_combined_search_summary() {
    let dir = temp_dir("flatpak-unavailable-summary");
    let marker = dir.join("marker");
    install_fake_apt(&dir, &marker, 0, 0);

    let output = run_allp(
        &dir,
        &["search", "telegram", "--scope", "apps", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Search Summary"));
    assert!(out.contains("Flatpak unavailable · executable not found"));
}

#[test]
fn flatpak_no_configured_remotes_is_reported_without_searching_catalog() {
    let dir = temp_dir("flatpak-no-remotes");
    install_fake_flatpak_no_remotes(&dir);

    let output = run_allp(
        &dir,
        &["search", "telegram", "--from", "flatpak", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Flatpak skipped · no configured remotes"));
    assert!(!out.contains("search should not run without remotes"));
}

#[test]
fn flatpak_native_search_failure_is_reported_without_suppressing_apt() {
    let dir = temp_dir("flatpak-search-failure");
    let marker = dir.join("marker");
    install_fake_apt(&dir, &marker, 0, 0);
    install_fake_flatpak_search_failure(&dir);

    let output = run_allp(
        &dir,
        &["search", "telegram", "--scope", "apps", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("APT"));
    assert!(out.contains("Search Summary"));
    assert!(out.contains("Flatpak search failed · remote flathub is temporarily unavailable"));
}

#[test]
fn flatpak_telegram_result_is_included_in_combined_apps_search() {
    let dir = temp_dir("flatpak-telegram");
    let marker = dir.join("flatpak-commands");
    install_fake_apt(&dir, &dir.join("apt-marker"), 0, 0);
    install_fake_snap(&dir, &dir.join("snap-marker"), 0);
    install_fake_flatpak_with_marker(&dir, &marker);

    let output = run_allp(
        &dir,
        &["search", "telegram", "--scope", "apps", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("org.telegram.desktop"));
    assert!(out.contains("Name: Telegram Desktop"));
    assert!(out.contains("Remote: flathub"));
    assert!(out.contains("Type: universal application"));
    assert!(out.contains("Search Summary"));
    assert!(out.contains("Flatpak  1 result") || out.contains("Flatpak 1 result"));
    let commands = fs::read_to_string(marker).expect("flatpak commands should be recorded");
    assert!(commands.contains("remotes --columns=name,title,url,filter,options"));
    assert!(commands
        .contains("search telegram --columns=application,name,description,version,branch,remotes"));
}

#[test]
fn execution_progress_wraps_native_output_without_heartbeat_in_non_tty() {
    let dir = temp_dir("apt-progress");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_sudo_marker(&dir, &sudo_marker);
    install_fake_apt_phased_upgrade(&dir, &marker);

    let output = run_allp(&dir, &["update", "--from", "apt", "--yes", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    let err = stderr(&output);
    assert!(out.contains("apt update native output"));
    assert!(err.contains("● [1/1] APT update started"));
    assert!(err.contains("✔ [1/1] APT finished in"));
    assert!(err.contains("Result: Completed"));
    assert!(!err.contains("still running"));
}

#[test]
fn update_statuses_are_normalized_and_unavailable_tools_are_hidden_by_default() {
    let dir = temp_dir("normalized-statuses");
    install_fake_python_runtime_only(&dir);
    install_fake_node_up_to_date(&dir);

    let normal = run_allp(
        &dir,
        &["update", "--scope", "dev", "--dry-run", "--no-color"],
    );

    assert!(normal.status.success(), "stderr: {}", stderr(&normal));
    let out = stdout(&normal);
    assert!(out.contains("Selected for execution: none"));
    assert!(out.contains("npm global"));
    assert!(out.contains("Up to date"));
    assert!(out.contains("npm project"));
    assert!(out.contains("Not applicable"));
    assert!(out.contains("pip environment"));
    assert!(out.contains("Protected"));
    assert!(!out.contains("Skipped · Skipped"));
    assert!(!out.contains("pipx tools"));
    assert!(!out.contains("uv tools"));
    assert!(!out.contains("pnpm"));
    assert!(!out.contains("Yarn"));

    let verbose = run_allp(
        &dir,
        &[
            "update",
            "--scope",
            "dev",
            "--dry-run",
            "--verbose",
            "--no-color",
        ],
    );
    assert!(verbose.status.success(), "stderr: {}", stderr(&verbose));
    let verbose_out = stdout(&verbose);
    assert!(verbose_out.contains("pipx tools"));
    assert!(verbose_out.contains("Unavailable"));
    assert!(verbose_out.contains("uv tools"));
    assert!(verbose_out.contains("pnpm"));
    assert!(verbose_out.contains("Yarn"));
}

#[test]
fn update_json_contains_normalized_non_actionable_statuses() {
    let dir = temp_dir("normalized-statuses-json");
    install_fake_python_runtime_only(&dir);
    install_fake_node_up_to_date(&dir);

    let output = run_allp(&dir, &["update", "--scope", "dev", "--dry-run", "--json"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let json: Value = serde_json::from_slice(&output.stdout).expect("JSON should parse");
    let statuses = json["results"]
        .as_array()
        .expect("results should be an array")
        .iter()
        .map(|record| record["status"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(statuses.contains(&"up_to_date"));
    assert!(statuses.contains(&"not_applicable"));
    assert!(statuses.contains(&"protected"));
    assert!(statuses.contains(&"unavailable"));
}

#[test]
fn user_scoped_update_does_not_print_root_privilege_notice() {
    let dir = temp_dir("flatpak-update");
    install_fake_flatpak(&dir);

    let output = run_allp(
        &dir,
        &["update", "--from", "flatpak", "--dry-run", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(!stderr(&output).contains("Root-required"));
    assert!(stdout(&output).contains("Privilege: Current user"));
}

#[test]
fn update_no_interactive_requires_confirmation_before_execution() {
    let dir = temp_dir("update-confirmation");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_sudo_marker(&dir, &sudo_marker);
    install_fake_apt(&dir, &marker, 0, 0);

    let output = run_allp(&dir, &["update", "--from", "apt", "--no-interactive"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        !marker.exists(),
        "update must not execute without confirmation"
    );
    assert!(
        !sudo_marker.exists(),
        "update must not invoke sudo without confirmation"
    );
    assert!(stderr(&output).contains("confirmation required"));
    assert!(stdout(&output).contains("Planned Operation"));
}

#[test]
fn yes_bypasses_only_allp_confirmation_and_does_not_add_native_yes_flags() {
    let dir = temp_dir("yes-install");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_sudo_marker(&dir, &sudo_marker);
    install_fake_apt(&dir, &marker, 0, 0);

    let output = run_allp(
        &dir,
        &["install", "git", "--from", "apt", "--yes", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let executed = fs::read_to_string(&marker).expect("fake apt should execute");
    assert!(executed.contains("install -- git"));
    assert!(!executed.contains(" -y"));
    assert!(!executed.contains("--assumeyes"));
    assert!(sudo_marker.exists(), "root-required child should use sudo");
}

#[test]
fn remove_execution_progress_wraps_native_operation() {
    let dir = temp_dir("remove-progress");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_sudo_marker(&dir, &sudo_marker);
    install_fake_apt(&dir, &marker, 0, 0);

    let output = run_allp(
        &dir,
        &["remove", "git", "--from", "apt", "--yes", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let err = stderr(&output);
    assert!(err.contains("● [1/1] APT remove started"));
    assert!(err.contains("✔ [1/1] APT finished in"));
    let executed = fs::read_to_string(&marker).expect("fake apt remove should execute");
    assert!(executed.contains("remove -- git"));
}

#[test]
fn upgrade_no_interactive_defaults_to_no_without_yes() {
    let dir = temp_dir("upgrade-confirmation");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);

    let output = run_allp(&dir, &["upgrade", "--from", "apt", "--no-color"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(!marker.exists(), "upgrade must not execute without --yes");
    assert!(stderr(&output).contains("confirmation required"));
    assert!(stdout(&output).contains("Planned Operation"));
}

#[test]
fn npm_project_update_plan_inspects_outdated_and_never_uses_npx_update() {
    let dir = temp_dir("npm-project-update");
    let marker = dir.join("executed");
    install_fake_node(&dir, &marker);
    let project = temp_dir("npm-project");
    fs::write(
        project.join("package.json"),
        r#"{"dependencies":{"typescript":"^5.0.0"}}"#,
    )
    .expect("package manifest should be written");
    fs::write(project.join("package-lock.json"), "{}").expect("lockfile should be written");

    let output = run_allp_in(
        &dir,
        &project,
        &[
            "update",
            "--from",
            "npm",
            "--target",
            "project",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(!marker.exists(), "dry run must not execute npm update");
    let out = stdout(&output);
    assert!(out.contains("npm project"));
    assert!(out.contains("npm update"));
    assert!(out.contains("package-lock.json"));
    assert!(!out.contains("npx update"));
}

#[test]
fn npm_global_update_uses_npm_update_global() {
    let dir = temp_dir("npm-global-update");
    let marker = dir.join("executed");
    install_fake_node(&dir, &marker);

    let output = run_allp(
        &dir,
        &[
            "update",
            "--from",
            "npm",
            "--target",
            "global",
            "--dry-run",
            "--no-color",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("npm global"));
    assert!(out.contains("npm update --global"));
    assert!(
        !marker.exists(),
        "dry run must not execute npm global update"
    );
}

#[test]
fn node_runtime_and_cli_components_are_separate_from_package_targets() {
    let root = temp_dir("nvm-node-components");
    let bin = install_fake_nvm_node_runtime(&root);

    let output = run_allp(
        &bin,
        &["update", "--from", "node", "--dry-run", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Node.js runtime"));
    assert!(out.contains("owner: nvm"));
    assert!(out.contains("installed versions: v20.11.1"));
    assert!(out.contains("npm CLI"));
    assert!(out.contains("Corepack"));
    assert!(out.contains("npm global"));
    assert!(out.contains("npm project"));
    assert!(out.contains("Selected for execution: none"));
    assert!(!out.contains("npx update"));
    assert!(!out.contains("Node.js runtime\n   Action"));
    assert!(!out.contains("npm update --global\n   Privilege: Current user\n○ Node.js runtime"));
}

#[test]
fn unknown_node_runtime_ownership_is_protected_and_not_planned() {
    let dir = temp_dir("unknown-node-runtime");
    install_fake_unknown_node_runtime(&dir);

    let output = run_allp(
        &dir,
        &["upgrade", "--from", "node", "--dry-run", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Node.js runtime"));
    assert!(out.contains("Protected"));
    assert!(out.contains("owner: unknown"));
    assert!(out.contains("runtime will not be modified automatically"));
    assert!(out.contains("Selected for execution: none"));
}

#[test]
fn pnpm_and_yarn_upgrade_plans_use_native_version_specific_commands() {
    let dir = temp_dir("node-upgrade-plans");
    let marker = dir.join("executed");
    install_fake_node(&dir, &marker);
    let project = temp_dir("node-project");
    fs::write(
        project.join("package.json"),
        r#"{"packageManager":"yarn@1.22.22"}"#,
    )
    .expect("package manifest should be written");
    fs::write(project.join("pnpm-lock.yaml"), "lockfileVersion: '9'")
        .expect("pnpm lockfile should be written");
    fs::write(project.join("yarn.lock"), "# yarn").expect("yarn lockfile should be written");

    let pnpm = run_allp_in(
        &dir,
        &project,
        &[
            "upgrade",
            "--from",
            "pnpm",
            "--target",
            "project",
            "--dry-run",
            "--no-color",
        ],
    );
    assert!(pnpm.status.success(), "stderr: {}", stderr(&pnpm));
    assert!(stdout(&pnpm).contains("pnpm update --latest"));

    let yarn = run_allp_in(
        &dir,
        &project,
        &[
            "upgrade",
            "--from",
            "yarn",
            "--target",
            "project",
            "--dry-run",
            "--no-color",
        ],
    );
    assert!(yarn.status.success(), "stderr: {}", stderr(&yarn));
    assert!(stdout(&yarn).contains("yarn upgrade --latest"));
    assert!(!marker.exists(), "dry run must not execute pnpm or yarn");
}

#[test]
fn python_update_targets_plan_pip_pipx_and_uv_without_sudo() {
    let dir = temp_dir("python-update-targets");
    let marker = dir.join("executed");
    install_fake_python(&dir, &marker);
    let project = temp_dir("python-project");
    let venv = project.join(".venv");
    fs::create_dir_all(&venv).expect("fake venv should be created");

    let pip = run_allp_in_with_env(
        &dir,
        &project,
        &[
            "update",
            "--from",
            "pip",
            "--target",
            "environment",
            "--dry-run",
            "--no-color",
        ],
        &[("VIRTUAL_ENV", &venv)],
    );
    assert!(pip.status.success(), "stderr: {}", stderr(&pip));
    let pip_out = stdout(&pip);
    assert!(pip_out.contains("pip environment"));
    assert!(pip_out.contains("python3 -m pip install --upgrade requests"));
    assert!(pip_out.contains("Privilege: Current user"));

    let pipx = run_allp(
        &dir,
        &[
            "update",
            "--from",
            "pipx",
            "--target",
            "tools",
            "--dry-run",
            "--no-color",
        ],
    );
    assert!(pipx.status.success(), "stderr: {}", stderr(&pipx));
    assert!(stdout(&pipx).contains("pipx upgrade-all"));

    let uv = run_allp(
        &dir,
        &[
            "upgrade",
            "--from",
            "uv",
            "--target",
            "tools",
            "--dry-run",
            "--no-color",
        ],
    );
    assert!(uv.status.success(), "stderr: {}", stderr(&uv));
    assert!(stdout(&uv).contains("uv tool upgrade --all"));
    assert!(
        !marker.exists(),
        "dry run must not execute pip, pipx, or uv mutations"
    );
}

#[test]
fn dev_scope_update_reports_precise_python_and_node_targets_not_generic_skips() {
    let dir = temp_dir("dev-scope-update");
    let marker = dir.join("executed");
    install_fake_apt(&dir, &marker, 0, 0);
    install_fake_snap(&dir, &marker, 0);
    install_fake_python(&dir, &marker);
    install_fake_node(&dir, &marker);

    let output = run_allp(
        &dir,
        &["update", "--scope", "dev", "--dry-run", "--no-color"],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("pip environment"));
    assert!(out.contains("npm project"));
    assert!(out.contains("npm global"));
    assert!(out.contains("pipx tools"));
    assert!(out.contains("uv tools"));
    assert!(out.contains("Selected for execution:"));
    assert!(!out.contains("Selected for execution: APT"));
    assert!(!out.contains("APT\n   Action"));
    assert!(!out.contains("Snap\n   Action"));
    assert!(!out.contains("Python      Skipped"));
    assert!(!out.contains("Node.js     Skipped"));
    assert!(
        !marker.exists(),
        "dry run must not execute developer updates"
    );
}

#[test]
fn root_required_install_without_confirmation_does_not_invoke_sudo() {
    let dir = temp_dir("partial-update");
    let marker = dir.join("executed");
    let sudo_marker = dir.join("sudo-called");
    install_fake_sudo_marker(&dir, &sudo_marker);
    install_fake_apt(&dir, &marker, 0, 0);

    let output = run_allp(&dir, &["install", "git", "--from", "apt", "--no-color"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        !sudo_marker.exists(),
        "install must not invoke sudo before final confirmation"
    );
    assert!(
        !marker.exists(),
        "install must not invoke native command before final confirmation"
    );
    assert!(stdout(&output).contains("Planned Operation"));
    assert!(stderr(&output).contains("confirmation required"));
}

#[test]
fn json_search_stdout_is_parseable_without_human_logs() {
    let dir = temp_dir("search-json");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);

    let output = run_allp(&dir, &["search", "git", "--json"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stderr(&output).is_empty());
    let json: Value = serde_json::from_slice(&output.stdout).expect("search JSON should parse");
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["command"], "search");
    assert!(json["results"].as_array().expect("results array").len() <= 25);
}

#[test]
fn list_filter_limit_and_no_pager_are_applied() {
    let dir = temp_dir("list-filter");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);

    let output = run_allp(
        &dir,
        &[
            "list",
            "--from",
            "apt",
            "--filter",
            "git",
            "--limit",
            "1",
            "--no-pager",
        ],
    );

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("Installed Packages · APT"));
    assert!(out.contains("git"));
    assert!(!out.contains("code"));
}

#[test]
fn info_default_is_curated_full_and_raw_are_explicit() {
    let dir = temp_dir("info-output");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);

    let default = run_allp(&dir, &["info", "git", "--from", "apt", "--no-color"]);
    assert!(default.status.success(), "stderr: {}", stderr(&default));
    let default_out = stdout(&default);
    assert!(default_out.contains("Package Information"));
    assert!(default_out.contains("Homepage:"));
    assert!(!default_out.contains("Filename:"));

    let full = run_allp(
        &dir,
        &["info", "git", "--from", "apt", "--full", "--no-color"],
    );
    assert!(full.status.success(), "stderr: {}", stderr(&full));
    assert!(stdout(&full).contains("Filename:"));

    let raw = run_allp(
        &dir,
        &["info", "git", "--from", "apt", "--raw", "--no-color"],
    );
    assert!(raw.status.success(), "stderr: {}", stderr(&raw));
    let raw_out = stdout(&raw);
    assert!(raw_out.contains("Package: git"));
    assert!(raw_out.contains("Filename:"));
}

#[test]
fn search_query_is_not_executed_through_a_shell() {
    let dir = temp_dir("no-shell");
    let marker = dir.join("shell-created");
    install_fake_apt(&dir, &dir.join("marker"), 0, 0);
    let injected = format!("git; /usr/bin/touch {}", marker.display());

    let output = run_allp(&dir, &["search", &injected, "--from", "apt", "--no-color"]);

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        !marker.exists(),
        "query argument was interpreted by a shell"
    );
}
