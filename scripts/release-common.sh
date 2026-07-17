#!/usr/bin/env bash
set -euo pipefail

release_die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

release_repo_root() {
    git rev-parse --show-toplevel 2>/dev/null || release_die "not inside a Git repository"
}

release_cd_root() {
    local root
    root="$(release_repo_root)"
    cd "$root"
}

release_current_version() {
    awk '
        $0 == "[package]" { in_package = 1; next }
        /^\[/ && $0 != "[package]" { in_package = 0 }
        in_package && $1 == "version" && $2 == "=" {
            gsub(/"/, "", $3)
            print $3
            exit
        }
    ' Cargo.toml
}

release_validate_semver() {
    local version="$1"
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || release_die "invalid semantic version: $version"
}

release_semver_cmp() {
    local left="$1"
    local right="$2"
    local left_major left_minor left_patch right_major right_minor right_patch

    release_validate_semver "$left"
    release_validate_semver "$right"

    IFS=. read -r left_major left_minor left_patch <<<"$left"
    IFS=. read -r right_major right_minor right_patch <<<"$right"

    if ((10#$left_major > 10#$right_major)); then printf '1\n'; return; fi
    if ((10#$left_major < 10#$right_major)); then printf -- '-1\n'; return; fi
    if ((10#$left_minor > 10#$right_minor)); then printf '1\n'; return; fi
    if ((10#$left_minor < 10#$right_minor)); then printf -- '-1\n'; return; fi
    if ((10#$left_patch > 10#$right_patch)); then printf '1\n'; return; fi
    if ((10#$left_patch < 10#$right_patch)); then printf -- '-1\n'; return; fi
    printf '0\n'
}

release_bump_version() {
    local current="$1"
    local bump="$2"
    local major minor patch

    release_validate_semver "$current"
    IFS=. read -r major minor patch <<<"$current"

    case "$bump" in
        patch)
            patch=$((10#$patch + 1))
            ;;
        minor)
            minor=$((10#$minor + 1))
            patch=0
            ;;
        major)
            major=$((10#$major + 1))
            minor=0
            patch=0
            ;;
        *)
            release_die "BUMP must be patch, minor, or major"
            ;;
    esac

    printf '%s.%s.%s\n' "$major" "$minor" "$patch"
}

release_marker_path() {
    printf '.release-state/release.env\n'
}

release_marker_value() {
    local key="$1"
    local marker

    marker="$(release_marker_path)"
    [ -f "$marker" ] || return 0
    sed -n "s/^${key}=//p" "$marker" | head -n 1
}

release_tag_exists() {
    local tag="$1"
    git rev-parse -q --verify "refs/tags/$tag" >/dev/null
}

release_latest_tag() {
    git describe --tags --abbrev=0 2>/dev/null || true
}

release_unresolved_conflicts() {
    [ -n "$(git diff --name-only --diff-filter=U)" ]
}

release_archive_base() {
    local version="$1"
    printf '%s-v%s-source.tar.gz\n' "${RELEASE_PREFIX:-allp}" "$version"
}

release_archive_prefix() {
    local version="$1"
    printf '%s-v%s/\n' "${RELEASE_PREFIX:-allp}" "$version"
}

release_archive_path() {
    local version="$1"
    printf '%s/%s\n' "${DIST_DIR:-dist}" "$(release_archive_base "$version")"
}

release_checksum_base() {
    local version="$1"
    printf '%s.sha256\n' "$(release_archive_base "$version")"
}

release_checksum_path() {
    local version="$1"
    printf '%s/%s\n' "${DIST_DIR:-dist}" "$(release_checksum_base "$version")"
}

release_draft_notes_path() {
    local version="$1"
    printf 'release/RELEASE_NOTES_v%s.md\n' "$version"
}

release_dist_notes_path() {
    local version="$1"
    printf '%s/RELEASE_NOTES_v%s.md\n' "${DIST_DIR:-dist}" "$version"
}
