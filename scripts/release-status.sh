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
        printf 'Planned tag: %s already exists\n' "$tag"
    else
        printf 'Planned tag: %s is available\n' "$tag"
    fi

    printf 'Source archive path: %s\n' "$(release_archive_path "$planned_version")"
    printf 'Checksum path: %s\n' "$(release_checksum_path "$planned_version")"
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
