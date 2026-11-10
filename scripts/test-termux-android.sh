#!/usr/bin/env bash
# Run only on a disposable AOSP emulator. It uses a pinned official Termux rootfs.
set -euo pipefail

binary="$(realpath -- "$1")"
test -x "$binary"
report="$2"
expected_version="$3"
expected_commit="$4"
mkdir -p "$report"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
prefix=/data/data/com.termux/files/usr
bootstrap=bootstrap-2026.11.08-r1%2Bapt.android-7
curl --fail --silent --show-error --location --retry 3 \
    "https://github.com/termux/termux-packages/releases/download/$bootstrap/bootstrap-x86_64.zip" \
    -o "$work/bootstrap.zip"
printf '%s  %s\n' 41453bd2ec3f9690e2474085fbfe8d3406476b3cdd9627502d0ae4fb0d3eecb0 "$work/bootstrap.zip" | sha256sum -c -
unzip -q "$work/bootstrap.zip" -d "$work/prefix"
(
    cd "$work/prefix"
    while IFS='←' read -r target link; do
        # The bootstrap uses a Unicode delimiter, which Bash IFS can read in UTF-8.
        [[ -n "$target" && "$link" == ./* ]]
        ln -s -- "$target" "$link"
    done < SYMLINKS.txt
    rm SYMLINKS.txt
    install -m 0755 "$binary" bin/allp
    tar -cf "$work/prefix.tar" .
)

# Setup is the only step that requires the emulator's ADB root shell.
[[ "$(adb shell getprop ro.kernel.qemu | tr -d '\r')" == 1 ]]
adb root
adb wait-for-device
adb shell 'test "$(id -u)" = 0 && setenforce 0'
adb push "$work/prefix.tar" /data/local/tmp/allp-termux-prefix.tar
# The app creates its cache directory outside the extracted package prefix.
adb shell "mkdir -p '$prefix' /data/data/com.termux/cache/apt/archives/partial && tar -xf /data/local/tmp/allp-termux-prefix.tar -C '$prefix' && chown -R 2000:2000 /data/data/com.termux && chmod 0755 /data/data/com.termux /data/data/com.termux/files"
adb shell "export PREFIX='$prefix'; export PATH='$prefix/bin:/system/bin'; '$prefix/bin/allp' doctor --json" > "$report/root-doctor.json"
adb shell "export PREFIX='$prefix'; export PATH='$prefix/bin:/system/bin'; '$prefix/bin/allp' detect --json" > "$report/root-detect.json"

adb unroot
adb wait-for-device
adb shell /system/bin/sh -s <<'ANDROID' | tee "$report/native-lifecycle.log"
set -eu
test "$(id -u)" = 2000
export PREFIX=/data/data/com.termux/files/usr
test_home=/data/data/com.termux/files/home
export XDG_CACHE_HOME="$test_home/cache"
export XDG_CONFIG_HOME="$test_home/config"
export XDG_STATE_HOME="$test_home/state"
export TMPDIR="$PREFIX/tmp"
export PATH="$PREFIX/bin:/system/bin"
export TERMUX_VERSION=emulator-smoke
export TERMUX_APP_PACKAGE_MANAGER=apt
export DEBIAN_FRONTEND=noninteractive
mkdir -p "$test_home"
cd "$test_home"
allp --version
allp update --from termux --skip-self-update --yes --no-tui --no-color
allp search tree --from termux --exact --json > search.json
allp upgrade --from pkg --dry-run --json > upgrade-plan.json
printf 'y\n' | allp install tree --from termux --yes --no-interactive --no-tui --no-color
tree --version
allp info tree --from termux --json > info.json
allp list --from termux --json > inventory-installed.json
allp upgrade --from termux --yes --no-tui --no-color
printf 'y\n' | allp remove tree --from termux --yes --no-interactive --no-tui --no-color
allp list --from termux --json > inventory-removed.json
allp doctor --json > user-doctor.json
allp detect --json > user-detect.json
allp --version --verbose > identity.txt
ANDROID
for file in search.json info.json upgrade-plan.json inventory-installed.json inventory-removed.json user-doctor.json user-detect.json identity.txt; do
    adb pull "/data/data/com.termux/files/home/$file" "$report/$file"
done
python3 - "$report" "$expected_version" "$expected_commit" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
def load(name):
    return json.loads((root / f"{name}.json").read_text())

for name, is_root in [("root-doctor", True), ("user-doctor", False)]:
    platform = load(name)["platform"]
    assert platform["os"] == "android" and platform["libc"] == "bionic", platform
    assert platform["distribution"]["id"] == "termux" and platform["is_root"] == is_root, platform
    assert platform["current_user"]["uid"] == (0 if is_root else 2000), platform

def termux_entry(name):
    report = load(name)
    entries = report.get("entries", report.get("results"))
    if isinstance(entries, dict):
        entries = entries.get("entries")
    return next(entry for entry in entries if entry["backend_id"] == "termux")

root_entry = termux_entry("root-detect")
assert root_entry["state"] == "found_but_unavailable" and "root shell" in root_entry["message"], root_entry
user_entry = termux_entry("user-detect")
assert user_entry["state"] == "ready", user_entry

search = load("search")
assert search["complete"] and any(row["package_id"] == "tree" for row in search["results"]["candidates"]), search
assert load("info")["results"]["package_id"] == "tree"
plan = load("upgrade-plan")
assert plan["complete"] and any("full-upgrade" in item["command"] for item in plan["plans"]), plan
assert all("sudo" not in item["command"] for item in plan["plans"]), plan
assert any(row["package_id"] == "tree" for row in load("inventory-installed")["results"])
assert all(row["package_id"] != "tree" for row in load("inventory-removed")["results"])
identity = (root / "identity.txt").read_text()
assert f"Allp {sys.argv[2]}" in identity and sys.argv[3] in identity and "x86_64-linux-android" in identity, identity
print("Android identity, root rejection, native update/search/info/install/list/upgrade/remove passed")
PY
