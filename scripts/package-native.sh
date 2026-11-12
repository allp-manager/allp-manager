#!/usr/bin/env bash
# Package a locally built native Linux ELF. No installation or publication.
set -euo pipefail
if [[ $# != 3 || ! $1 =~ ^(deb|rpm)$ ]]; then
  echo 'Usage: bash scripts/package-native.sh deb|rpm BINARY OUTPUT_DIRECTORY' >&2
  exit 2
fi
format=$1
binary=$(realpath "$2")
mkdir -p "$3"
output=$(realpath "$3")
root=$(cd "$(dirname "$0")/.." && pwd)
command -v readelf >/dev/null
test -x "$binary"
machine=$(LC_ALL=C readelf -h "$binary" | sed -n 's/^ *Machine: *//p')
case "$machine" in
  'Advanced Micro Devices X86-64') deb_arch=amd64; rpm_arch=x86_64 ;;
  AArch64) deb_arch=arm64; rpm_arch=aarch64 ;;
  *) echo "Unsupported Linux ELF architecture: $machine" >&2; exit 2 ;;
esac
identity=$("$binary" --version)
version=${identity#allp }
if [[ $identity != "allp $version" || ! $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?$ ]]; then
  echo "Unsupported binary version: $identity" >&2; exit 2
fi
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
if [[ $format == deb ]]; then
  command -v dpkg-deb >/dev/null
  command -v dpkg-shlibdeps >/dev/null
  mkdir -p "$stage/root/DEBIAN" "$stage/debian"
  install -Dm755 "$binary" "$stage/root/usr/bin/allp"
  install -Dm644 "$root/LICENSE" "$stage/root/usr/share/doc/allp/copyright"
  install -Dm644 "$root/README.md" "$stage/root/usr/share/doc/allp/README.md"
  install -Dm644 "$root/README.fa.md" "$stage/root/usr/share/doc/allp/README.fa.md"
  # dpkg-shlibdeps needs a source control file, even for binary-only packaging.
  printf 'Source: allp\nSection: utils\nPriority: optional\nMaintainer: Ali <Im_wrench@proton.me>\n\nPackage: allp\nArchitecture: any\nDescription: Native package-manager orchestrator\n' > "$stage/debian/control"
  dependencies=$(cd "$stage" && dpkg-shlibdeps -O -e"$stage/root/usr/bin/allp" | sed -n 's/^shlibs:Depends=//p')
  if [[ -z $dependencies ]]; then
    # A static ELF has no interpreter and needs no shared-library dependencies.
    if readelf -l "$binary" | grep -q INTERP; then
      echo 'Could not determine shared-library dependencies; refusing an incomplete package.' >&2
      exit 1
    fi
    dependencies=''
  fi
  sed -e "s/@VERSION@/$version/g" -e "s/@ARCH@/$deb_arch/g" -e "s/@DEPENDS@/$dependencies/g" \
    "$root/packaging/debian/control.in" > "$stage/root/DEBIAN/control"
  if [[ -z $dependencies ]]; then sed -i '/^Depends: *$/d' "$stage/root/DEBIAN/control"; fi
  dpkg-deb --build --root-owner-group "$stage/root" "$output/allp_${version}-1_${deb_arch}.deb"
else
  command -v rpmbuild >/dev/null
  mkdir -p "$stage/"{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
  install -m755 "$binary" "$stage/SOURCES/allp"
  install -m644 "$root/LICENSE" "$root/README.md" "$root/README.fa.md" "$stage/SOURCES/"
  sed "s/@VERSION@/$version/g" "$root/packaging/rpm/allp.spec.in" > "$stage/SPECS/allp.spec"
  rpmbuild -bb --target "$rpm_arch" --define "_topdir $stage" "$stage/SPECS/allp.spec"
  find "$stage/RPMS" -name '*.rpm' -exec cp -- '{}' "$output/" \;
fi
