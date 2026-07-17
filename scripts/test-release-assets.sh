#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/allp-release-assets.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

version="9.8.7"
archive="$fixture/allp-v${version}-x86_64-unknown-linux-gnu.tar.gz"
printf '%s\n' 'fixture binary archive' >"$archive"
(
    cd "$fixture"
    sha256sum "$(basename "$archive")" >"$(basename "$archive").sha256"
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
assert manifest["assets"][0]["target"] == "x86_64-unknown-linux-gnu"
assert manifest["assets"][0]["libc"] == "glibc"
assert len(manifest["assets"][0]["sha256"]) == 64
PY

printf '%s\n' 'Release asset workflow tests passed.'
