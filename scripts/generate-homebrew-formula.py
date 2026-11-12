#!/usr/bin/env python3
"""Generate a tap formula using the checksum of an exact tagged source archive."""
import argparse
import hashlib
from pathlib import Path
import re
import tarfile
import tomllib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--version", required=True)
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.version):
    parser.error("version must be a stable X.Y.Z tag")
with tarfile.open(args.source) as archive:
    manifest = archive.extractfile(f"allp-manager-{args.version}/Cargo.toml")
    if manifest is None or tomllib.loads(manifest.read().decode())["package"]["version"] != args.version:
        parser.error("archive Cargo.toml does not match the requested version")
digest = hashlib.sha256(args.source.read_bytes()).hexdigest()
template = Path(__file__).resolve().parents[1] / "packaging/homebrew/allp.rb.in"
formula = template.read_text().replace("@VERSION@", args.version).replace("@SHA256@", digest)
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(formula)
print(args.output)
