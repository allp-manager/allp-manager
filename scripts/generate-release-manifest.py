#!/usr/bin/env python3
"""Generate or verify Allp's target-specific self-update release manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
from datetime import datetime, timezone

SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")


def fail(message: str) -> None:
    raise SystemExit(f"error: {message}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def target_fields(target: str) -> tuple[str, str, str | None, str]:
    architecture = target.split("-", 1)[0]
    if target.endswith("-unknown-linux-gnu"):
        return "linux", architecture, "glibc", "allp"
    if target.endswith("-unknown-linux-musl"):
        return "linux", architecture, "musl", "allp"
    if target.endswith("-apple-darwin"):
        return "macos", architecture, None, "allp"
    if target.endswith("-pc-windows-msvc"):
        return "windows", architecture, None, "allp.exe"
    fail(f"unsupported release target in asset name: {target}")


def checksum_from_file(path: Path, expected_name: str) -> str:
    fields = path.read_text(encoding="utf-8").strip().split()
    if len(fields) < 2 or fields[1].lstrip("*") != expected_name:
        fail(f"checksum file {path} does not name {expected_name}")
    value = fields[0].lower()
    if not re.fullmatch(r"[0-9a-f]{64}", value):
        fail(f"checksum file {path} does not contain a SHA-256 value")
    return value


def assets_for(version: str, dist: Path) -> list[dict[str, object]]:
    pattern = re.compile(
        rf"^allp-v{re.escape(version)}-(?P<target>.+)\.(?P<extension>tar\.gz|zip)$"
    )
    assets: list[dict[str, object]] = []
    for archive in sorted(dist.iterdir()):
        match = pattern.match(archive.name)
        if not match or match.group("target") == "source":
            continue
        target = match.group("target")
        os_name, architecture, libc, binary = target_fields(target)
        checksum_path = archive.with_name(f"{archive.name}.sha256")
        if not checksum_path.is_file():
            fail(f"missing checksum for {archive.name}")
        expected = checksum_from_file(checksum_path, archive.name)
        actual = sha256(archive)
        if actual != expected:
            fail(f"checksum mismatch for {archive.name}")
        assets.append(
            {
                "target": target,
                "os": os_name,
                "architecture": architecture,
                "libc": libc,
                "archive": archive.name,
                "binary": binary,
                "sha256": actual,
                "size": archive.stat().st_size,
            }
        )
    if not assets:
        fail("no target-specific binary assets were found")
    return assets


def generate(args: argparse.Namespace) -> None:
    if not SEMVER.fullmatch(args.version) or not SEMVER.fullmatch(
        args.minimum_updater_version
    ):
        fail("version values must be strict three-part semantic versions")
    dist = Path(args.dist)
    manifest = {
        "schema_version": 1,
        "version": args.version,
        "tag": f"v{args.version}",
        "channel": "stable",
        "published_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "minimum_updater_version": args.minimum_updater_version,
        "assets": assets_for(args.version, dist),
    }
    output = Path(args.output)
    output.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def verify(args: argparse.Namespace) -> None:
    manifest_path = Path(args.verify)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if manifest.get("schema_version") != 1:
        fail("manifest schema_version must be 1")
    version = manifest.get("version", "")
    if not isinstance(version, str) or not SEMVER.fullmatch(version):
        fail("manifest version is invalid")
    if manifest.get("tag") != f"v{version}":
        fail("manifest tag does not match version")
    expected = assets_for(version, Path(args.dist))
    if manifest.get("assets") != expected:
        fail("manifest assets do not match files in dist")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version")
    parser.add_argument("--minimum-updater-version")
    parser.add_argument("--dist", required=True)
    parser.add_argument("--output")
    parser.add_argument("--verify")
    args = parser.parse_args()
    if args.verify:
        verify(args)
    elif args.version and args.minimum_updater_version and args.output:
        generate(args)
    else:
        parser.error("generation requires --version, --minimum-updater-version, and --output")


if __name__ == "__main__":
    main()
