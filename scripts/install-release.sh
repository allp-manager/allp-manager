#!/bin/sh
set -eu

repository="allp-manager/allp-manager"
release_base="https://github.com/${repository}/releases"
install_dir="${ALLP_INSTALL_DIR:-${HOME}/.local/bin}"
requested_version="${1:-latest}"

for command in curl tar install mktemp uname sed; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'error: required command not found: %s\n' "$command" >&2
        exit 1
    fi
done

case "$(uname -s)" in
    Linux) os_target="linux" ;;
    Darwin) os_target="darwin" ;;
    *)
        printf 'error: this installer supports Linux and macOS; use the release archive directly on other systems\n' >&2
        exit 1
        ;;
esac

case "$(uname -m)" in
    x86_64 | amd64) architecture="x86_64" ;;
    arm64 | aarch64) architecture="aarch64" ;;
    *)
        printf 'error: unsupported architecture: %s\n' "$(uname -m)" >&2
        exit 1
        ;;
esac

target="${architecture}-unknown-linux-gnu"
if [ "$os_target" = "darwin" ]; then
    target="${architecture}-apple-darwin"
fi

temporary_dir="$(mktemp -d "${TMPDIR:-/tmp}/allp-install.XXXXXX")"
cleanup() {
    rm -rf -- "$temporary_dir"
}
trap cleanup EXIT HUP INT TERM

if [ "$requested_version" = "latest" ]; then
    manifest_url="${release_base}/latest/download/allp-release-manifest.json"
    curl --fail --location --proto '=https' --tlsv1.2 \
        --output "${temporary_dir}/manifest.json" "$manifest_url"
    requested_version="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "${temporary_dir}/manifest.json" | sed -n '1p')"
    if [ -z "$requested_version" ]; then
        printf 'error: release manifest did not contain a version\n' >&2
        exit 1
    fi
fi

validated_version="$(
    printf '%s\n' "$requested_version" |
        sed -n '/^[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*$/p'
)"
if [ "$validated_version" != "$requested_version" ]; then
    printf 'error: version must be a numeric x.y.z version, got: %s\n' "$requested_version" >&2
    exit 1
fi

archive="allp-v${requested_version}-${target}.tar.gz"
asset_base="${release_base}/download/v${requested_version}"
curl --fail --location --proto '=https' --tlsv1.2 \
    --output "${temporary_dir}/${archive}" "${asset_base}/${archive}"
curl --fail --location --proto '=https' --tlsv1.2 \
    --output "${temporary_dir}/${archive}.sha256" "${asset_base}/${archive}.sha256"

if command -v sha256sum >/dev/null 2>&1; then
    (cd "$temporary_dir" && sha256sum -c "${archive}.sha256")
elif command -v shasum >/dev/null 2>&1; then
    (cd "$temporary_dir" && shasum -a 256 -c "${archive}.sha256")
else
    printf 'error: sha256sum or shasum is required to verify the release\n' >&2
    exit 1
fi

archive_listing="$(tar -tzf "${temporary_dir}/${archive}")"
if [ "$archive_listing" != "allp" ]; then
    printf 'error: release archive contains unexpected paths\n' >&2
    exit 1
fi
tar -xzf "${temporary_dir}/${archive}" -C "$temporary_dir" allp
mkdir -p "$install_dir"
install -m 0755 "${temporary_dir}/allp" "${install_dir}/allp"

printf 'Installed Allp %s to %s/allp\n' "$requested_version" "$install_dir"
case ":${PATH}:" in
    *":${install_dir}:"*) ;;
    *) printf 'Add %s to PATH, then run: allp --version\n' "$install_dir" ;;
esac
