#!/usr/bin/env bash
set -euo pipefail

root="$(mktemp -d)"
trap 'rm -rf -- "$root"' EXIT
dist="$root/dist"
mkdir -p "$dist"
asset="allp-0.3.5.1-x86_64-unknown-linux-gnu.tar.gz"
printf '%s' 'continuous fixture' >"$dist/$asset"
(cd "$dist" && sha256sum "$asset" >"$asset.sha256")

python3 scripts/generate-continuous-manifest.py \
  --base-version 0.3.5 \
  --build-revision 1 \
  --git-commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  --build-id 123.1 \
  --workflow-run-id 123 \
  --workflow-run-number 1 \
  --built-at 2026-08-08T00:00:00Z \
  --dist "$dist" \
  --output "$dist/allp-continuous-manifest.json"

python3 scripts/generate-continuous-manifest.py \
  --verify "$dist/allp-continuous-manifest.json" \
  --dist "$dist"

if python3 scripts/generate-continuous-manifest.py \
  --base-version 0.3.5 \
  --build-revision 1 \
  --git-commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  --build-id 123.1 \
  --workflow-run-id 123 \
  --workflow-run-number 1 \
  --built-at 2026-08-08T00:00:00Z \
  --dist "$dist" \
  --output "$dist/invalid-commit-manifest.json" 2>/dev/null; then
  echo "41-character Git commit unexpectedly accepted" >&2
  exit 1
fi

printf '%s' 'tampered' >>"$dist/$asset"
if python3 scripts/generate-continuous-manifest.py \
  --verify "$dist/allp-continuous-manifest.json" \
  --dist "$dist" 2>/dev/null; then
  echo "tampered asset unexpectedly verified" >&2
  exit 1
fi

echo "continuous manifest tests passed"
