#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/release-common.sh
source "$SCRIPT_DIR/release-common.sh"

target_version_for_artifact() {
    local marker_version

    marker_version="$(release_marker_value VERSION)"
    if [ -n "$marker_version" ]; then
        printf '%s\n' "$marker_version"
    else
        release_current_version
    fi
}

require_release_marker() {
    local marker

    marker="$(release_marker_path)"
    [ -f "$marker" ] || release_die "release marker is missing; run make release-prepare first"
}

require_release_commit_message() {
    local subject

    subject="$(git log -1 --pretty=%s)"
    [[ "$subject" == release:* ]] || release_die "latest commit subject must begin with release:"
}

require_clean_release_files() {
    local version="$1"
    local draft
    local title
    local status

    draft="$(release_draft_notes_path "$version")"
    title="$(release_title_path "$version")"
    status="$(git status --porcelain -- Cargo.toml Cargo.lock CHANGELOG.md README.md README.fa.md "$draft" "$title")"
    if [ -n "$status" ]; then
        printf '%s\n' "$status" >&2
        release_die "release files have uncommitted changes"
    fi
}

require_commit_contains_release_files() {
    local version="$1"
    local draft
    local title
    local files

    draft="$(release_draft_notes_path "$version")"
    title="$(release_title_path "$version")"
    files="$(git show --format= --name-only HEAD)"

    printf '%s\n' "$files" | grep -qx 'Cargo.toml' || release_die "release commit must include Cargo.toml"
    printf '%s\n' "$files" | grep -qx 'CHANGELOG.md' || release_die "release commit must include CHANGELOG.md"
    printf '%s\n' "$files" | grep -qx "$draft" || release_die "release commit must include $draft"
    printf '%s\n' "$files" | grep -qx "$title" || release_die "release commit must include $title"
}

require_lockfile_matches_version() {
    local version="$1"

    [ -f Cargo.lock ] || return 0
    if grep -q 'name = "allp"' Cargo.lock && ! grep -A2 'name = "allp"' Cargo.lock | grep -q "version = \"$version\""; then
        release_die "Cargo.lock does not contain allp version $version"
    fi
}

require_artifacts_absent() {
    local version="$1"
    local archive
    local checksum
    local notes

    archive="$(release_archive_path "$version")"
    checksum="$(release_checksum_path "$version")"
    notes="$(release_dist_notes_path "$version")"

    [ ! -e "$archive" ] || release_die "$archive already exists"
    [ ! -e "$checksum" ] || release_die "$checksum already exists"
    [ ! -e "$notes" ] || release_die "$notes already exists"
}

validate_archive() {
    local archive="$1"
    local prefix="$2"
    local bad_paths
    local required_path

    if ! tar -tzf "$archive" | awk -v prefix="$prefix" 'index($0, prefix) != 1 { bad = 1 } END { exit bad }'; then
        release_die "archive entries do not all use prefix $prefix"
    fi

    bad_paths="$(tar -tzf "$archive" | grep -E '(^|/)(\.git|target|dist|\.release-state)(/|$)|(^|/)\.env($|[./])' || true)"
    if [ -n "$bad_paths" ]; then
        printf '%s\n' "$bad_paths" >&2
        release_die "archive contains ignored or local-only paths"
    fi

    for required_path in Cargo.toml Cargo.lock README.md README.fa.md CHANGELOG.md LICENSE src/ docs/ scripts/ tests/; do
        if ! tar -tzf "$archive" | awk -v prefix="$prefix" -v required="$required_path" '$0 == prefix required || index($0, prefix required) == 1 { found = 1 } END { exit !found }'; then
            release_die "archive is missing required path $required_path"
        fi
    done
}

create_archive() {
    local version="$1"
    local tag="v$version"
    local archive
    local prefix

    archive="$(release_archive_path "$version")"
    prefix="$(release_archive_prefix "$version")"

    release_tag_exists "$tag" || release_die "tag $tag does not exist"
    [ ! -e "$archive" ] || release_die "$archive already exists"

    mkdir -p "${DIST_DIR:-dist}"
    git archive --format=tar.gz --prefix="$prefix" --output="$archive" "$tag"
    validate_archive "$archive" "$prefix"

    printf 'Created %s\n' "$archive"
}

create_checksum() {
    local version="$1"
    local archive
    local checksum
    local archive_base
    local checksum_base

    archive="$(release_archive_path "$version")"
    checksum="$(release_checksum_path "$version")"
    archive_base="$(release_archive_base "$version")"
    checksum_base="$(release_checksum_base "$version")"

    command -v sha256sum >/dev/null || release_die "sha256sum is required"
    [ -f "$archive" ] || release_die "$archive is missing"
    [ ! -e "$checksum" ] || release_die "$checksum already exists"

    (
        cd "${DIST_DIR:-dist}"
        sha256sum "$archive_base" >"$checksum_base"
        sha256sum -c "$checksum_base"
    )

    printf 'Created %s\n' "$checksum"
}

create_notes() {
    local version="$1"
    local draft
    local notes
    local checksum
    local sha

    draft="$(release_draft_notes_path "$version")"
    notes="$(release_dist_notes_path "$version")"
    checksum="$(release_checksum_path "$version")"

    [ -f "$draft" ] || release_die "$draft is missing"
    [ -f "$checksum" ] || release_die "$checksum is missing"
    [ ! -e "$notes" ] || release_die "$notes already exists"

    sha="$(awk '{ print $1; exit }' "$checksum")"
    sed "s/SHA256: _generated during finalization_/SHA256: $sha/" "$draft" >"$notes"
    if ! grep -q "$sha" "$notes"; then
        {
            printf '\n## Final Checksum\n\n'
            printf 'SHA256: %s\n' "$sha"
        } >>"$notes"
    fi

    printf 'Created %s\n' "$notes"
}

finalize_release() {
    local version
    local current
    local marker_version
    local tag
    local marker

    require_release_marker
    marker="$(release_marker_path)"
    marker_version="$(release_marker_value VERSION)"
    current="$(release_current_version)"

    [ -n "$marker_version" ] || release_die "release marker does not include VERSION"
    release_validate_semver "$marker_version"
    release_validate_semver "$current"
    [ "$marker_version" = "$current" ] || release_die "marker version $marker_version does not match Cargo.toml version $current"

    version="$marker_version"
    tag="v$version"

    if release_unresolved_conflicts; then
        release_die "unresolved merge conflicts must be resolved before finalizing a release"
    fi

    require_release_commit_message
    require_clean_release_files "$version"
    require_commit_contains_release_files "$version"
    require_lockfile_matches_version "$version"

    if release_tag_exists "$tag"; then
        release_die "tag $tag already exists"
    fi

    require_artifacts_absent "$version"

    git tag -a "$tag" -m "Allp $tag" HEAD
    create_archive "$version"
    create_checksum "$version"
    create_notes "$version"
    rm -f "$marker"

    printf '\nFinalized local release %s.\n' "$tag"
    printf 'No push, publish, or upload was performed.\n'
    printf '\nOptional manual commands when you are ready:\n'
    printf '  git push origin %s\n' "$(git branch --show-current 2>/dev/null || printf '<branch>')"
    printf '  git push origin %s\n' "$tag"
}

main() {
    local mode="${1:-}"
    local version

    release_cd_root

    case "$mode" in
        ""|--hook)
            finalize_release
            ;;
        --archive)
            version="$(target_version_for_artifact)"
            release_validate_semver "$version"
            create_archive "$version"
            ;;
        --checksum)
            version="$(target_version_for_artifact)"
            release_validate_semver "$version"
            create_checksum "$version"
            ;;
        --notes)
            version="$(target_version_for_artifact)"
            release_validate_semver "$version"
            create_notes "$version"
            ;;
        *)
            release_die "unknown release-finalize option: $mode"
            ;;
    esac
}

main "$@"
