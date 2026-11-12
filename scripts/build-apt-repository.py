#!/usr/bin/env python3
"""Build a signed APT archive without installing packages or changing APT sources."""
import argparse
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime
import gzip
import hashlib
import html
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from urllib.parse import urlsplit


def run(args, **kwargs):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, **kwargs).stdout


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(args):
    output = args.output.resolve()
    if output.exists():
        raise ValueError("output already exists; choose a fresh staging directory")
    base = args.base_url.rstrip("/")
    url = urlsplit(base)
    if url.scheme != "https" or not url.netloc or url.username or url.password or url.query or url.fragment:
        raise ValueError("base URL must be a public HTTPS URL without credentials/query/fragment")
    if not re.fullmatch(r"[a-z][a-z0-9-]{0,31}", args.suite):
        raise ValueError("invalid suite")
    if not re.fullmatch(r"(?:[A-Fa-f0-9]{40}|[A-Fa-f0-9]{64})", args.key):
        raise ValueError("use a full signing-key fingerprint")
    if not 1 <= args.valid_days <= 365:
        raise ValueError("validity must be 1-365 days; refresh the archive before expiry")
    packages = [*args.packages]
    if args.previous:
        packages.extend(args.previous.glob("pool/**/*.deb"))
    if not packages:
        raise ValueError("at least one package is required")
    gpg = ["gpg", "--batch", "--homedir", str(args.gnupghome.resolve())]
    public = run([*gpg, "--export-options", "export-minimal", "--export", args.key])
    if not public:
        raise ValueError("signing public key was not found")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".allp-apt-stage-", dir=output.parent) as temp:
        stage = Path(temp) / "archive"
        stage.mkdir()
        pool = stage / "pool/main/a/allp"
        pool.mkdir(parents=True)
        records = {}
        for package in packages:
            if package.is_symlink() or not package.is_file():
                raise ValueError(f"package must be a regular file: {package}")
            fields = run(["dpkg-deb", "--show", "--showformat=${Package}\t${Version}\t${Architecture}", str(package)]).decode().split("\t")
            if len(fields) != 3:
                raise ValueError("invalid package metadata")
            name, version, arch = fields
            if name != "allp" or arch not in ("amd64", "arm64") or not re.fullmatch(r"[0-9][A-Za-z0-9.+:~\-]*", version):
                raise ValueError(f"unsupported package metadata: {fields}")
            filename = f"allp_{version}_{arch}.deb"
            sha = digest(package)
            if filename in records and records[filename]["sha256"] != sha:
                raise ValueError(f"published package versions are immutable: {filename}; bump the package revision")
            records[filename] = {"version": version, "architecture": arch, "sha256": sha}
            shutil.copyfile(package, pool / filename)
        architectures = sorted({record["architecture"] for record in records.values()})
        release_files = []
        for arch in architectures:
            directory = stage / f"dists/{args.suite}/main/binary-{arch}"
            directory.mkdir(parents=True)
            if args.previous:
                old_hashes = args.previous / f"dists/{args.suite}/main/binary-{arch}/by-hash/SHA256"
                if old_hashes.is_dir():
                    for old in old_hashes.iterdir():
                        if old.is_symlink() or not old.is_file() or not re.fullmatch(r"[a-f0-9]{64}", old.name) or digest(old) != old.name:
                            raise ValueError("invalid previous by-hash index")
                        destination = directory / "by-hash/SHA256" / old.name
                        destination.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copyfile(old, destination)
            index = run(["dpkg-scanpackages", "--multiversion", "--arch", arch, "pool", "/dev/null"], cwd=stage)
            for name, contents in [("Packages", index), ("Packages.gz", gzip.compress(index, mtime=0))]:
                path = directory / name
                path.write_bytes(contents)
                release_files.append(path)
                by_hash = directory / "by-hash/SHA256" / digest(path)
                by_hash.parent.mkdir(parents=True, exist_ok=True)
                by_hash.write_bytes(contents)
        now = datetime.now(timezone.utc)
        release_dir = stage / "dists" / args.suite
        release = release_dir / "Release"
        text = (f"Origin: Allp\nLabel: Allp\nSuite: {args.suite}\nCodename: {args.suite}\n"
                f"Date: {format_datetime(now, usegmt=True)}\n"
                f"Valid-Until: {format_datetime(now + timedelta(days=args.valid_days), usegmt=True)}\n"
                f"Architectures: {' '.join(architectures)}\nComponents: main\nAcquire-By-Hash: yes\n"
                "Description: Allp package-manager orchestrator\nSHA256:\n")
        for path in release_files:
            text += f" {digest(path)} {path.stat().st_size} {path.relative_to(release_dir)}\n"
        release.write_text(text)
        (stage / "allp-archive-keyring.gpg").write_bytes(public)
        (stage / "allp-archive-keyring.asc").write_bytes(run([*gpg, "--armor", "--export-options", "export-minimal", "--export", args.key]))
        for mode, name in [("--clearsign", "InRelease"), ("--detach-sign", "Release.gpg")]:
            run([*gpg, "--yes", "--pinentry-mode", "loopback", "--passphrase-fd", "0", "--digest-algo", "SHA256", "--local-user", args.key,
                 "--output", str(release_dir / name), mode, str(release)], input=os.environ.get("ALLP_APT_SIGNING_PASSPHRASE", "").encode())
        run(["gpgv", "--keyring", str(stage / "allp-archive-keyring.gpg"), str(release_dir / "InRelease")])
        sources = (f"Types: deb\nURIs: {base}/\nSuites: {args.suite}\nComponents: main\n"
                   f"Architectures: {' '.join(architectures)}\nSigned-By: /etc/apt/keyrings/allp-archive-keyring.gpg\n")
        (stage / "allp.sources").write_text(sources)
        instructions = f"""# Allp APT repository

Signing-key fingerprint: `{args.key.upper()}`

One-time setup (review the fingerprint before trusting the archive):

```bash
curl -fsSL {base}/allp-archive-keyring.gpg -o /tmp/allp-archive-keyring.gpg
gpg --show-keys --with-fingerprint /tmp/allp-archive-keyring.gpg
sudo install -d -m 0755 /etc/apt/keyrings
sudo install -m 0644 /tmp/allp-archive-keyring.gpg /etc/apt/keyrings/allp-archive-keyring.gpg
curl -fsSL {base}/allp.sources -o /tmp/allp.sources
cat /tmp/allp.sources
sudo install -m 0644 /tmp/allp.sources /etc/apt/sources.list.d/allp.sources
sudo apt update
sudo apt install allp
```

After setup, use `sudo apt install allp` or your normal APT upgrade workflow.
This archive distributes the Allp project's packages; it is separate from the
Debian and Ubuntu official repositories. Native library requirements are checked
by APT. Architectures: {', '.join(architectures)}. Metadata expires after
{args.valid_days} days and must be refreshed before expiry.
"""
        (stage / "README.md").write_text(instructions)
        (stage / "index.html").write_text("<!doctype html><meta charset=utf-8><title>Allp APT repository</title><pre>" + html.escape(instructions) + "</pre>")
        (stage / ".nojekyll").touch()
        (stage / "archive.json").write_text(json.dumps({"fingerprint": args.key.upper(), "suite": args.suite, "packages": records}, indent=2) + "\n")
        for path in stage.rglob("*"):
            path.chmod(0o755 if path.is_dir() else 0o644)
        stage.chmod(0o755)
        stage.rename(output)
    print(output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packages", type=Path, nargs="*", default=[])
    parser.add_argument("--previous", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--key", required=True)
    parser.add_argument("--gnupghome", type=Path, required=True)
    parser.add_argument("--suite", default="stable")
    parser.add_argument("--valid-days", type=int, default=90)
    try:
        build(parser.parse_args())
    except (ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"APT archive build failed: {error}\n")
