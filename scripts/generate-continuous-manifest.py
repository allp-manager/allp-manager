#!/usr/bin/env python3
"""Generate or verify Allp's continuous-build manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path


SEMVER = re.compile(r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$")
SHA = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
CHECKSUM = re.compile(r"^(?P<sha>[0-9a-fA-F]{64})\s+\*?(?P<name>[^/\\]+)$")
WORKFLOW_NAME = "Continuous Build"
WORKFLOW_FILE = ".github/workflows/continuous-build.yml"


def target_metadata(target: str) -> tuple[str, str, str | None, str]:
    mappings = {
        "x86_64-unknown-linux-gnu": ("linux", "x86_64", "glibc", "allp"),
        "aarch64-unknown-linux-gnu": ("linux", "aarch64", "glibc", "allp"),
        "x86_64-apple-darwin": ("macos", "x86_64", None, "allp"),
        "aarch64-apple-darwin": ("macos", "aarch64", None, "allp"),
        "x86_64-pc-windows-msvc": ("windows", "x86_64", None, "allp.exe"),
    }
    try:
        return mappings[target]
    except KeyError as error:
        raise ValueError(f"unsupported continuous target: {target}") from error


def checksum_for(archive: Path) -> str:
    checksum_path = archive.with_name(f"{archive.name}.sha256")
    if not checksum_path.is_file():
        raise ValueError(f"missing checksum: {checksum_path.name}")
    match = CHECKSUM.fullmatch(checksum_path.read_text(encoding="utf-8").strip())
    if match is None or match.group("name") != archive.name:
        raise ValueError(f"malformed checksum: {checksum_path.name}")
    expected = match.group("sha").lower()
    actual = hashlib.sha256(archive.read_bytes()).hexdigest()
    if actual != expected:
        raise ValueError(f"checksum mismatch: {archive.name}")
    return actual


def collect_assets(dist: Path, display_version: str) -> list[dict[str, object]]:
    pattern = re.compile(
        rf"^allp-{re.escape(display_version)}-(?P<target>.+)\.(?:tar\.gz|zip)$"
    )
    assets: list[dict[str, object]] = []
    for archive in sorted(dist.iterdir()):
        if not archive.is_file():
            continue
        match = pattern.fullmatch(archive.name)
        if match is None:
            continue
        target = match.group("target")
        os_name, architecture, libc, binary = target_metadata(target)
        assets.append(
            {
                "target": target,
                "os": os_name,
                "architecture": architecture,
                "libc": libc,
                "filename": archive.name,
                # `archive` keeps compatibility with the hardened stable asset model.
                "archive": archive.name,
                "binary": binary,
                "sha256": checksum_for(archive),
                "size": archive.stat().st_size,
            }
        )
    if not assets:
        raise ValueError(f"no continuous assets found in {dist}")
    return assets


def validate_manifest(manifest: dict[str, object], dist: Path | None = None) -> None:
    if manifest.get("schema_version") != 1 or manifest.get("channel") != "continuous":
        raise ValueError("unsupported continuous manifest identity")
    base = manifest.get("base_version")
    if not isinstance(base, str) or SEMVER.fullmatch(base) is None:
        raise ValueError("invalid base_version")
    revision = manifest.get("build_revision")
    run_number = manifest.get("workflow_run_number")
    if not isinstance(revision, int) or revision <= 0 or revision != run_number:
        raise ValueError("build_revision must equal the positive workflow_run_number")
    if manifest.get("display_version") != f"{base}.{revision}":
        raise ValueError("display_version does not match base_version/build_revision")
    commit = manifest.get("git_commit")
    if not isinstance(commit, str) or SHA.fullmatch(commit) is None:
        raise ValueError("git_commit must be a full hexadecimal commit ID")
    if manifest.get("workflow_name") != WORKFLOW_NAME:
        raise ValueError("unexpected workflow_name")
    if manifest.get("workflow_file") != WORKFLOW_FILE:
        raise ValueError("unexpected workflow_file")
    if not str(manifest.get("workflow_run_id", "")).isdigit():
        raise ValueError("invalid workflow_run_id")
    assets = manifest.get("assets")
    if not isinstance(assets, list) or not assets:
        raise ValueError("continuous manifest has no assets")
    targets: set[str] = set()
    for item in assets:
        if not isinstance(item, dict):
            raise ValueError("invalid asset entry")
        target = item.get("target")
        filename = item.get("filename")
        checksum = item.get("sha256")
        if not isinstance(target, str) or target in targets:
            raise ValueError(f"invalid or duplicate target: {target}")
        targets.add(target)
        if (
            not isinstance(filename, str)
            or Path(filename).name != filename
            or item.get("archive") != filename
        ):
            raise ValueError("unsafe or inconsistent asset filename")
        if not isinstance(checksum, str) or re.fullmatch(r"[0-9a-f]{64}", checksum) is None:
            raise ValueError(f"invalid checksum for {filename}")
        if not isinstance(item.get("size"), int) or item["size"] <= 0:
            raise ValueError(f"invalid size for {filename}")
        if dist is not None:
            archive = dist / filename
            if not archive.is_file() or archive.stat().st_size != item["size"]:
                raise ValueError(f"missing or size-mismatched asset: {filename}")
            if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum:
                raise ValueError(f"checksum mismatch: {filename}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-version")
    parser.add_argument("--build-revision", type=int)
    parser.add_argument("--git-commit")
    parser.add_argument("--build-id")
    parser.add_argument("--workflow-run-id")
    parser.add_argument("--workflow-run-number", type=int)
    parser.add_argument("--built-at")
    parser.add_argument("--minimum-updater-version", default="0.3.5")
    parser.add_argument("--dist", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--verify", type=Path)
    args = parser.parse_args()
    if args.verify is None and any(
        value is None
        for value in (
            args.base_version,
            args.build_revision,
            args.git_commit,
            args.build_id,
            args.workflow_run_id,
            args.workflow_run_number,
            args.built_at,
            args.dist,
            args.output,
        )
    ):
        parser.error("generation requires build identity, --dist, and --output")
    return args


def main() -> None:
    args = parse_args()
    if args.verify is not None:
        manifest = json.loads(args.verify.read_text(encoding="utf-8"))
        validate_manifest(manifest, args.dist)
        return
    if SEMVER.fullmatch(args.base_version) is None:
        raise ValueError("base version must be strict three-component SemVer")
    if SEMVER.fullmatch(args.minimum_updater_version) is None:
        raise ValueError("minimum updater version must be strict SemVer")
    manifest = {
        "schema_version": 1,
        "channel": "continuous",
        "base_version": args.base_version,
        "build_revision": args.build_revision,
        "display_version": f"{args.base_version}.{args.build_revision}",
        "git_commit": args.git_commit,
        "build_id": args.build_id,
        "workflow_run_id": str(args.workflow_run_id),
        "workflow_run_number": args.workflow_run_number,
        "workflow_name": WORKFLOW_NAME,
        "workflow_file": WORKFLOW_FILE,
        "built_at": args.built_at,
        "minimum_updater_version": args.minimum_updater_version,
        "assets": collect_assets(args.dist, f"{args.base_version}.{args.build_revision}"),
    }
    validate_manifest(manifest, args.dist)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    temporary = args.output.with_suffix(f"{args.output.suffix}.tmp")
    temporary.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    temporary.replace(args.output)


if __name__ == "__main__":
    main()
