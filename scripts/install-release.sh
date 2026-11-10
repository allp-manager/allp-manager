#!/bin/sh
set -eu

repository="allp-manager/allp-manager"
release_base="https://github.com/${repository}/releases"
api_base="https://api.github.com/repos/${repository}"
install_dir="${ALLP_INSTALL_DIR:-${HOME}/.local/bin}"
requested_version="latest"
version_given=false
sync_existing=false
explicit_installs=""

usage() {
    printf '%s\n' \
        'Usage: sh install-allp.sh [x.y.z|latest] [--sync-existing] [--replace-existing /path/allp]' \
        '  --sync-existing  Replace writable standalone Allp copies, keeping backups.' \
        '                   Never invokes sudo or changes package-manager-owned paths.' \
        '  --replace-existing /path/allp  Explicitly include a custom standalone copy (repeatable).' \
        '  ALLP_INSTALL_DIR Select the destination (default: ~/.local/bin; root: /usr/local/bin).'
}
while [ "$#" -gt 0 ]; do
    argument="$1"
    shift
    case "$argument" in
        --sync-existing) sync_existing=true ;;
        --replace-existing)
            if [ "$#" = 0 ]; then
                printf 'error: --replace-existing requires an absolute /path/allp\n' >&2
                exit 1
            fi
            case "$1" in
                *'
'*) printf 'error: installation paths cannot contain newlines\n' >&2; exit 1 ;;
                /*/allp) ;;
                *) printf 'error: --replace-existing requires an absolute /path/allp\n' >&2; exit 1 ;;
            esac
            explicit_installs="${explicit_installs}$1
"
            shift
            ;;
        --help|-h) usage; exit 0 ;;
        -*) printf 'error: unknown option: %s\n' "$argument" >&2; exit 1 ;;
        *)
            if [ "$version_given" = true ]; then
                printf 'error: only one release version can be supplied\n' >&2
                exit 1
            fi
            requested_version="$argument"
            version_given=true
            ;;
    esac
done

for command in curl tar install mktemp uname sed id cmp grep dirname cp mv rm stat; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'error: required command not found: %s\n' "$command" >&2
        exit 1
    fi
done

if [ "$(id -u)" = 0 ] && [ -z "${ALLP_INSTALL_DIR:-}" ]; then
    install_dir="/usr/local/bin"
fi
case "$install_dir" in
    /*) ;;
    *) printf 'error: ALLP_INSTALL_DIR must be an absolute path\n' >&2; exit 1 ;;
esac
install_dir="${install_dir%/}"

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
transaction_state=preparing
cleanup() {
    if [ "$transaction_state" = committing ]; then
        while IFS= read -r transaction; do
            # A failed rename has not changed the destination.
            [ ! -f "$transaction/new" ] || continue
            destination="$(sed -n '1p' "$transaction/destination")"
            if [ -f "$transaction/previous" ]; then
                if ! mv -f -- "$transaction/previous" "$destination"; then
                    : > "$transaction/keep-backup"
                    printf 'error: restore %s manually from %s/previous\n' "$destination" "$transaction" >&2
                fi
            else
                rm -f -- "$destination"
            fi
        done < "$temporary_dir/committed"
    fi
    if [ "$transaction_state" != complete ] && [ -f "$temporary_dir/transactions" ]; then
        while IFS= read -r transaction; do
            [ -f "$transaction/keep-backup" ] || rm -rf -- "$transaction"
        done < "$temporary_dir/transactions"
    fi
    rm -rf -- "$temporary_dir"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

latest_stable_tag_version() {
    # GitHub can mark an unrelated or continuous release as latest. Only look
    # at semantic release tags and check their publication status explicitly.
    command -v python3 >/dev/null 2>&1 || return 1
    curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 --max-time 20 \
        --output "$temporary_dir/refs.json" "$api_base/git/matching-refs/tags/v" || return 1
    python3 - "$temporary_dir/refs.json" > "$temporary_dir/versions" <<'PY' || return 1
import json
import re
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    refs = json.load(stream)
pattern = re.compile(r"refs/tags/v((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))")
matches = (pattern.fullmatch(ref.get("ref", "")) for ref in refs)
versions = {match[1] for match in matches if match}
for version in sorted(versions, key=lambda value: tuple(map(int, value.split("."))), reverse=True):
    print(version)
PY
    while IFS= read -r candidate_version; do
        curl --fail --silent --location --proto '=https' --tlsv1.2 --max-time 20 \
            --output "$temporary_dir/release.json" "$api_base/releases/tags/v$candidate_version" ||
            continue
        if python3 - "$temporary_dir/release.json" "v$candidate_version" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    release = json.load(stream)
valid = (
    release.get("tag_name") == sys.argv[2]
    and release.get("draft") is False
    and release.get("prerelease") is False
    and any(asset.get("name") == "allp-release-manifest.json" for asset in release.get("assets", []))
)
sys.exit(0 if valid else 1)
PY
        then
            printf '%s\n' "$candidate_version"
            return 0
        fi
    done < "$temporary_dir/versions"
    return 1
}

if [ "$requested_version" = "latest" ]; then
    requested_version=""
    if curl --fail --silent --location --proto '=https' --tlsv1.2 --max-time 20 \
        --output "$temporary_dir/manifest.json" "$release_base/latest/download/allp-release-manifest.json"; then
        requested_version="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)".*/\1/p' "$temporary_dir/manifest.json" | sed -n '1p')"
    fi
    if [ -z "$requested_version" ] && ! requested_version="$(latest_stable_tag_version)"; then
        printf 'error: could not find a stable release automatically; pass a published x.y.z version (fallback lookup uses Python 3)\n' >&2
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
if [ -L "$temporary_dir/allp" ] || [ ! -f "$temporary_dir/allp" ]; then
    printf 'error: release binary must be a regular file, not a link\n' >&2
    exit 1
fi

# Do not execute an old binary to identify it, especially when running as root.
# Only the destination and two standalone installation locations may be replaced.
# Other PATH hits are reported, never automatically modified.
: > "$temporary_dir/candidates"
: > "$temporary_dir/targets"
: > "$temporary_dir/transactions"
: > "$temporary_dir/committed"
printf '%s' "$explicit_installs" > "$temporary_dir/explicit"
add_candidate() {
    case "$1" in
        *'
'*) printf 'error: installation paths cannot contain newlines\n' >&2; exit 1 ;;
    esac
    if ! grep -Fxq -- "$1" "$temporary_dir/candidates"; then
        printf '%s\n' "$1" >> "$temporary_dir/candidates"
    fi
}
add_candidate "$install_dir/allp"
add_candidate "${HOME}/.local/bin/allp"
add_candidate '/usr/local/bin/allp'
while IFS= read -r candidate; do
    if [ ! -e "$candidate" ] && [ ! -L "$candidate" ]; then
        printf 'error: explicitly selected installation does not exist: %s\n' "$candidate" >&2
        exit 1
    fi
    add_candidate "$candidate"
done < "$temporary_dir/explicit"
remaining_path="${PATH}:"
while [ -n "$remaining_path" ]; do
    path_directory="${remaining_path%%:*}"
    remaining_path="${remaining_path#*:}"
    [ -n "$path_directory" ] || path_directory="$(pwd)"
    case "$path_directory" in
        /*) ;;
        *) path_directory="$(pwd)/$path_directory" ;;
    esac
    add_candidate "${path_directory%/}/allp"
done

blocked=false
while IFS= read -r candidate; do
    if [ "$candidate" = "$install_dir/allp" ]; then
        printf '%s\n' "$candidate" >> "$temporary_dir/targets"
        continue
    fi
    if [ ! -e "$candidate" ] && [ ! -L "$candidate" ]; then
        continue
    fi
    if [ -f "$candidate" ] && cmp -s "$temporary_dir/allp" "$candidate"; then
        continue
    fi
    if grep -Fxq -- "$candidate" "$temporary_dir/explicit"; then
        printf '%s\n' "$candidate" >> "$temporary_dir/targets"
        continue
    fi
    case "$candidate" in
        "${HOME}/.local/bin/allp"|/usr/local/bin/allp)
            if [ "$sync_existing" = true ]; then
                printf '%s\n' "$candidate" >> "$temporary_dir/targets"
            else
                printf 'error: another Allp copy could run instead: %s\nRe-run with --sync-existing to include standalone copies.\n' "$candidate" >&2
                blocked=true
            fi
            ;;
        *)
            printf 'error: another PATH installation exists: %s\nUpdate it through its owning installer, or remove that directory from PATH. It will not be modified.\n' "$candidate" >&2
            blocked=true
            ;;
    esac
done < "$temporary_dir/candidates"

owner_uid() {
    if [ "$os_target" = linux ]; then stat -c '%u' -- "$1";
    else stat -f '%u' "$1"; fi
}
directory_mode() {
    if [ "$os_target" = linux ]; then stat -c '%a' -- "$1";
    else stat -f '%Lp' "$1"; fi
}
# Preflight every destination before replacing any binary. Never elevate implicitly.
while IFS= read -r destination; do
    directory="$(dirname "$destination")"
    case "$directory" in
        /bin|/sbin|/usr/bin|/usr/sbin|/opt/homebrew/bin|/home/linuxbrew/.linuxbrew/bin)
            printf 'error: refusing a package-manager installation path: %s\n' "$destination" >&2
            blocked=true
            ;;
    esac
    if { command -v dpkg-query >/dev/null 2>&1 && dpkg-query -S "$destination" >/dev/null 2>&1; } ||
        { command -v rpm >/dev/null 2>&1 && rpm -qf "$destination" >/dev/null 2>&1; } ||
        { command -v pacman >/dev/null 2>&1 && pacman -Qo "$destination" >/dev/null 2>&1; }; then
        printf 'error: destination is owned by a native package; use that package manager: %s\n' "$destination" >&2
        blocked=true
    fi
    if [ -L "$destination" ] || { [ -e "$destination" ] && [ ! -f "$destination" ]; }; then
        printf 'error: refusing a symlink or non-regular destination: %s\n' "$destination" >&2
        blocked=true
    fi
    if [ -e "$destination" ] && [ "$(owner_uid "$destination")" != "$(id -u)" ]; then
        printf 'error: destination belongs to another user: %s\nUpdate that installation as its owner first, then retry. No sudo command is run automatically.\n' "$destination" >&2
        blocked=true
    fi
    ancestor="$directory"
    while :; do
        if [ -L "$ancestor" ]; then
            printf 'error: refusing a symlink installation directory: %s\n' "$ancestor" >&2
            blocked=true
        fi
        if [ -d "$ancestor" ]; then
            if [ ! -w "$ancestor" ]; then
                printf 'error: installation directory is not writable: %s\nUpdate the system copy separately as administrator, then retry.\n' "$ancestor" >&2
                blocked=true
            fi
            break
        fi
        ancestor="$(dirname "$ancestor")"
    done
    checked_ancestor="$ancestor"
    while [ "$checked_ancestor" != / ]; do
        if [ -L "$checked_ancestor" ]; then
            printf 'error: refusing a symlink installation directory: %s\n' "$checked_ancestor" >&2
            blocked=true
        fi
        checked_ancestor="$(dirname "$checked_ancestor")"
    done
    # A root installer must never stage a privileged binary in user-controlled directories.
    if [ "$(id -u)" = 0 ]; then
        while :; do
            if [ -L "$ancestor" ] || [ "$(owner_uid "$ancestor")" != 0 ] ||
                [ "$((0$(directory_mode "$ancestor") & 022))" != 0 ]; then
                printf 'error: root installation requires trusted root-owned directories: %s\n' "$ancestor" >&2
                blocked=true
            fi
            [ "$ancestor" != / ] || break
            ancestor="$(dirname "$ancestor")"
        done
    fi
done < "$temporary_dir/targets"
if [ "$blocked" = true ]; then
    printf 'Installation stopped before replacing any Allp binary.\n' >&2
    exit 1
fi

# Stage and back up all selected copies before committing same-directory renames.
while IFS= read -r destination; do
    mkdir -p "$(dirname "$destination")"
    transaction="$(mktemp -d "${destination}.backup.XXXXXX")"
    printf '%s\n' "$transaction" >> "$temporary_dir/transactions"
    printf '%s\n' "$destination" > "$transaction/destination"
    if [ -f "$destination" ]; then
        cp -p -- "$destination" "$transaction/previous"
    fi
    install -m 0755 "$temporary_dir/allp" "$transaction/new"
done < "$temporary_dir/targets"
transaction_state=committing
while IFS= read -r transaction; do
    destination="$(sed -n '1p' "$transaction/destination")"
    printf '%s\n' "$transaction" >> "$temporary_dir/committed"
    mv -f -- "$transaction/new" "$destination"
    if ! cmp -s "$temporary_dir/allp" "$destination"; then
        printf 'error: installed binary verification failed: %s\n' "$destination" >&2
        exit 1
    fi
done < "$temporary_dir/transactions"
transaction_state=complete
while IFS= read -r transaction; do
    destination="$(sed -n '1p' "$transaction/destination")"
    if [ -f "$transaction/previous" ]; then
        printf 'Replaced %s; backup: %s/previous\n' "$destination" "$transaction"
    else
        rm -rf -- "$transaction"
    fi
done < "$temporary_dir/transactions"

printf 'Installed Allp %s to %s/allp\n' "$requested_version" "$install_dir"
case ":${PATH}:" in
    *":${install_dir}:"*) ;;
    *) printf 'Add %s to PATH, then run: allp --version\n' "$install_dir" ;;
esac
