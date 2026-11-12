#!/usr/bin/env bash
# Inspect and run extracted artifacts; never install onto the host.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
binary=$(realpath "${1:-$root/target/release/allp}")
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
bash "$root/scripts/package-native.sh" deb "$binary" "$stage/packages"
package=$(find "$stage/packages" -name '*.deb' -print -quit)
test -n "$package"
test "$(dpkg-deb --field "$package" Package)" = allp
test -n "$(dpkg-deb --field "$package" Architecture)"
if readelf -l "$binary" | grep -q INTERP; then
  test -n "$(dpkg-deb --field "$package" Depends)"
fi
dpkg-deb --extract "$package" "$stage/extracted"
cmp "$binary" "$stage/extracted/usr/bin/allp"
test "$("$binary" --version)" = "$("$stage/extracted/usr/bin/allp" --version)"
"$stage/extracted/usr/bin/allp" profile --help | grep -q convert
test -s "$stage/extracted/usr/share/doc/allp/copyright"

python3 - "$root" "$stage" <<'PY'
import hashlib
import io
from pathlib import Path
import subprocess
import sys
import tarfile
root, stage = map(Path, sys.argv[1:])
source = stage / 'tagged-source.tar.gz'
with tarfile.open(source, 'w:gz') as archive:
    contents = b'[package]\nname = "allp"\nversion = "1.2.3"\n'
    member = tarfile.TarInfo('allp-manager-1.2.3/Cargo.toml')
    member.size = len(contents)
    archive.addfile(member, io.BytesIO(contents))
formula = stage / 'Formula/allp.rb'
subprocess.run([sys.executable, str(root / 'scripts/generate-homebrew-formula.py'), '--version', '1.2.3', '--source', str(source), '--output', str(formula)], check=True)
text = formula.read_text()
assert hashlib.sha256(source.read_bytes()).hexdigest() in text
assert 'v1.2.3.tar.gz' in text and '@VERSION@' not in text and '@SHA256@' not in text
invalid = subprocess.run([sys.executable, str(root / 'scripts/generate-homebrew-formula.py'), '--version', '1.2.4', '--source', str(source), '--output', str(formula)], capture_output=True)
assert invalid.returncode != 0
PY
printf 'Native package payload and Homebrew checksum tests passed.\n'
