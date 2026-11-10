#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/allp-release-assets.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

version="9.8.7"
archive="$fixture/allp-v${version}-x86_64-unknown-linux-gnu.tar.gz"
printf '%s\n' 'fixture binary archive' >"$archive"
for target in aarch64-linux-android x86_64-linux-android; do
    printf '%s\n' 'fixture Android archive' >"$fixture/allp-v${version}-${target}.tar.gz"
done
(
    cd "$fixture"
    for archive in *.tar.gz; do sha256sum "$archive" >"$archive.sha256"; done
)

python3 "$root/scripts/generate-release-manifest.py" \
    --version "$version" \
    --minimum-updater-version "0.3.3" \
    --dist "$fixture" \
    --output "$fixture/allp-release-manifest.json"
python3 "$root/scripts/generate-release-manifest.py" \
    --verify "$fixture/allp-release-manifest.json" \
    --dist "$fixture"

python3 - "$fixture/allp-release-manifest.json" <<'PY'
import json
import sys

manifest = json.load(open(sys.argv[1], encoding="utf-8"))
assert manifest["version"] == "9.8.7"
assert manifest["channel"] == "stable"
assets = {asset["target"]: asset for asset in manifest["assets"]}
assert assets["x86_64-unknown-linux-gnu"]["libc"] == "glibc"
for target in ("aarch64-linux-android", "x86_64-linux-android"):
    assert assets[target]["os"] == "android"
    assert assets[target]["libc"] == "bionic"
    assert assets[target]["binary"] == "allp"
assert all(len(asset["sha256"]) == 64 for asset in assets.values())
PY

python3 "$root/scripts/generate-release-manifest.py" \
    --version "$version" --channel prerelease \
    --minimum-updater-version "0.3.3" \
    --dist "$fixture" --output "$fixture/allp-release-manifest.json"
python3 "$root/scripts/generate-release-manifest.py" \
    --verify "$fixture/allp-release-manifest.json" --dist "$fixture"
python3 -c 'import json,sys; assert json.load(open(sys.argv[1]))["channel"] == "prerelease"' \
    "$fixture/allp-release-manifest.json"

printf '%s\n' 'Release asset workflow tests passed.'
