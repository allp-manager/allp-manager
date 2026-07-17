#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/release-common.sh
source "$SCRIPT_DIR/release-common.sh"

print_hooks_status() {
    local hooks_path

    hooks_path="$(git config --get core.hooksPath || true)"
    if [ "$hooks_path" = ".githooks" ]; then
        printf 'Git hooks: enabled (%s)\n' "$hooks_path"
    elif [ -n "$hooks_path" ]; then
        printf 'Git hooks: configured to %s, not .githooks\n' "$hooks_path"
    else
        printf 'Git hooks: not configured for this repository\n'
    fi
}

main() {
    local mode="${1:-}"
    local current
    local marker_version
    local latest_tag
    local planned_version
    local tag
    local status
    local title
    local notes
    local archive
    local checksum
    local manifest
    local binary_count
    local checksum_count

    release_cd_root

    if [ "$mode" = "--hooks-only" ]; then
        print_hooks_status
        return
    fi

    current="$(release_current_version)"
    marker_version="$(release_marker_value VERSION)"
    latest_tag="$(release_latest_tag)"
    planned_version="${marker_version:-$current}"
    tag="v$planned_version"
    status="$(git status --porcelain)"
    title="$(release_title_path "$planned_version")"
    notes="$(release_draft_notes_path "$planned_version")"
    archive="$(release_archive_path "$planned_version")"
    checksum="$(release_checksum_path "$planned_version")"
    manifest="$(release_manifest_path)"
    if [ -d "${DIST_DIR:-dist}" ]; then
        binary_count="$(find "${DIST_DIR:-dist}" -maxdepth 1 -type f \( -name "allp-v${planned_version}-*-unknown-linux-*.tar.gz" -o -name "allp-v${planned_version}-*-apple-darwin.tar.gz" -o -name "allp-v${planned_version}-*-pc-windows-msvc.zip" \) | wc -l | tr -d ' ')"
        checksum_count="$(find "${DIST_DIR:-dist}" -maxdepth 1 -type f -name "allp-v${planned_version}-*.sha256" | wc -l | tr -d ' ')"
    else
        binary_count=0
        checksum_count=0
    fi

    printf 'Allp release status\n'
    printf 'Current Cargo.toml version: %s\n' "$current"
    print_hooks_status

    if [ -n "$marker_version" ]; then
        printf 'Release marker: ready for v%s\n' "$marker_version"
    else
        printf 'Release marker: none\n'
    fi

    if [ -n "$latest_tag" ]; then
        printf 'Latest local tag: %s\n' "$latest_tag"
    else
        printf 'Latest local tag: none\n'
    fi

    if release_tag_exists "$tag"; then
        printf 'Expected tag: %s exists\n' "$tag"
        if [ "$(git rev-list -n 1 "$tag")" = "$(git rev-parse HEAD)" ]; then
            printf 'Tag points to current commit: yes\n'
        else
            printf 'Tag points to current commit: no\n'
        fi
    else
        printf 'Expected tag: %s is available\n' "$tag"
        printf 'Tag points to current commit: no\n'
    fi

    if [ -f "$title" ]; then
        printf 'Release title file: %s exists\n' "$title"
    else
        printf 'Release title file: %s missing\n' "$title"
    fi
    if [ -f "$notes" ]; then
        printf 'Release notes file: %s exists\n' "$notes"
    else
        printf 'Release notes file: %s missing\n' "$notes"
    fi
    if [ -f "$archive" ]; then
        printf 'Archive status: %s exists\n' "$archive"
    else
        printf 'Archive status: %s missing\n' "$archive"
    fi
    if [ -f "$checksum" ]; then
        printf 'Checksum status: %s exists\n' "$checksum"
    else
        printf 'Checksum status: %s missing\n' "$checksum"
    fi
    printf 'Source archive path: %s\n' "$archive"
    printf 'Checksum path: %s\n' "$checksum"
    printf 'Target binary assets: %s\n' "$binary_count"
    printf 'Release checksums: %s\n' "$checksum_count"
    if [ -f "$manifest" ]; then
        printf 'Release manifest: %s exists\n' "$manifest"
    else
        printf 'Release manifest: %s missing\n' "$manifest"
    fi
    printf 'Final release notes path: %s\n' "$(release_dist_notes_path "$planned_version")"

    if [ -n "$status" ]; then
        printf 'Working tree: has local changes\n'
    else
        printf 'Working tree: clean\n'
    fi

    if [ -n "$marker_version" ]; then
        printf 'Next step: commit prepared files with subject "release: Allp v%s".\n' "$marker_version"
    else
        printf 'Next step: run make release-prepare BUMP=patch when you are ready.\n'
    fi
}

main "$@"
