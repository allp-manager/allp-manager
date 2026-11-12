#!/usr/bin/env python3
"""Turn checksum-verified, published Stable GNU/Linux assets into DEB packages."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

REPO = "allp-manager/allp-manager"
TARGETS = {"amd64": ("x86_64-unknown-linux-gnu", "Advanced Micro Devices X86-64"),
           "arm64": ("aarch64-unknown-linux-gnu", "AArch64")}
LIBRARIES = {"libc.so.6", "libm.so.6", "libdl.so.2", "librt.so.1", "libpthread.so.0", "libgcc_s.so.1"}


def capture(args):
    return subprocess.check_output(args, text=True, timeout=180, env={**os.environ, "LC_ALL": "C"})


def download(tag, name, destination):
    subprocess.run(["gh", "release", "download", tag, "--repo", REPO, "--pattern", name, "--dir", str(destination)], check=True, timeout=180)
    subprocess.run(["gh", "release", "download", tag, "--repo", REPO, "--pattern", name + ".sha256", "--dir", str(destination)], check=True, timeout=180)
    path = destination / name
    checksum = (destination / (name + ".sha256")).read_text().split()
    if len(checksum) != 2 or checksum[1].lstrip("*") != name or not re.fullmatch(r"[a-fA-F0-9]{64}", checksum[0]):
        raise ValueError(f"invalid published checksum: {name}")
    if hashlib.sha256(path.read_bytes()).hexdigest() != checksum[0].lower():
        raise ValueError(f"published checksum mismatch: {name}")
    return path, checksum[0].lower()


def main(args):
    if args.tag:
        if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", args.tag):
            raise ValueError("a Stable vX.Y.Z tag is required")
        release = json.loads(capture(["gh", "api", f"repos/{REPO}/releases/tags/{args.tag}"]))
    else:
        releases = json.loads(capture(["gh", "api", f"repos/{REPO}/releases?per_page=100"]))
        stable = [r for r in releases if not r["draft"] and not r["prerelease"] and re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", r["tag_name"])]
        release = max(stable, key=lambda r: tuple(map(int, r["tag_name"][1:].split("."))))
    tag = release["tag_name"]
    if release["draft"] or release["prerelease"] or not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+", tag):
        raise ValueError("only published Stable releases may enter this archive")
    version = tag[1:]
    output = args.output.resolve()
    if output.exists():
        raise ValueError("output already exists")
    output.parent.mkdir(parents=True, exist_ok=True)
    provenance = {"release": release["html_url"], "tag": tag, "assets": {}}
    with tempfile.TemporaryDirectory(prefix="allp-stable-deb-", dir=output.parent) as temp:
        work = Path(temp)
        stage = work / "packages"
        stage.mkdir()
        source, sha = download(tag, f"allp-{tag}-source.tar.gz", work)
        provenance["assets"][source.name] = sha
        with tarfile.open(source) as archive:
            manifests = [m for m in archive.getmembers() if m.isfile() and PurePosixPath(m.name).name == "Cargo.toml" and len(PurePosixPath(m.name).parts) <= 2]
            if len(manifests) != 1:
                raise ValueError("source archive must contain one top-level Cargo.toml")
            manifest = manifests[0]
            if tomllib.loads(archive.extractfile(manifest).read().decode())["package"]["version"] != version:
                raise ValueError("tag/source version mismatch")
            prefix = PurePosixPath(manifest.name).parent
            docs = {}
            for name in ("LICENSE", "README.md", "README.fa.md"):
                member = archive.getmember(str(prefix / name))
                if not member.isfile() or member.size > 2 * 1024 * 1024:
                    raise ValueError("invalid source documentation member")
                docs[name] = archive.extractfile(member).read()
        for arch, (target, machine) in TARGETS.items():
            filename = f"allp_{version}-1_{arch}.deb"
            if args.existing and (args.existing / "pool/main/a/allp" / filename).is_file():
                continue  # Published versions remain immutable, including packaging bytes.
            asset, sha = download(tag, f"allp-{tag}-{target}.tar.gz", work)
            provenance["assets"][asset.name] = sha
            binary = work / f"allp-{arch}"
            with tarfile.open(asset) as archive:
                members = [m for m in archive.getmembers() if m.isfile() and PurePosixPath(m.name).name == "allp" and len(PurePosixPath(m.name).parts) <= 2]
                if len(members) != 1 or members[0].size > 64 * 1024 * 1024:
                    raise ValueError("binary archive must contain one regular allp executable")
                binary.write_bytes(archive.extractfile(members[0]).read())
            binary.chmod(0o755)
            header = capture(["readelf", "-h", str(binary)])
            if not any(line.strip() == f"Machine:                           {machine}" for line in header.splitlines()):
                actual = next((line.split(":", 1)[1].strip() for line in header.splitlines() if "Machine:" in line), "unknown")
                if actual != machine:
                    raise ValueError(f"asset architecture mismatch: {target}: {actual}")
            dynamic = capture(["readelf", "-d", str(binary)])
            needed = set(re.findall(r"Shared library: \[([^]]+)\]", dynamic))
            if not needed or needed - LIBRARIES:
                raise ValueError(f"unmapped native library dependency: {needed - LIBRARIES}")
            versions = capture(["readelf", "--version-info", str(binary)])
            glibc = re.findall(r"\bGLIBC_([0-9]+(?:\.[0-9]+)+)", versions)
            if not glibc:
                raise ValueError("GNU/Linux asset lacks expected glibc version metadata")
            minimum = max(glibc, key=lambda value: tuple(map(int, value.split("."))))
            dependencies = [f"libc6 (>= {minimum})"]
            if "libgcc_s.so.1" in needed:
                dependencies.append("libgcc-s1 (>= 4.2)")
            package_root = work / f"package-{arch}"
            (package_root / "DEBIAN").mkdir(parents=True)
            (package_root / "usr/bin").mkdir(parents=True)
            shutil.copyfile(binary, package_root / "usr/bin/allp")
            (package_root / "usr/bin/allp").chmod(0o755)
            doc_root = package_root / "usr/share/doc/allp"
            doc_root.mkdir(parents=True)
            for name, data in docs.items():
                (doc_root / ("copyright" if name == "LICENSE" else name)).write_bytes(data)
            control = f"Package: allp\nVersion: {version}-1\nSection: utils\nPriority: optional\nArchitecture: {arch}\nMaintainer: Ali <Im_wrench@proton.me>\nDepends: {', '.join(dependencies)}\nHomepage: https://github.com/{REPO}\nDescription: Transparent package-manager orchestrator\n Packaged from the checksum-verified published Stable release.\n"
            (package_root / "DEBIAN/control").write_text(control)
            subprocess.run(["dpkg-deb", "--build", "--root-owner-group", str(package_root), str(stage / filename)], check=True)
        (stage / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")
        stage.rename(output)
    print(f"Prepared {tag} for APT: {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--existing", type=Path)
    try:
        main(parser.parse_args())
    except (ValueError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"Stable DEB preparation failed: {error}\n")
