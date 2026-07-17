#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/release-common.sh
source "$SCRIPT_DIR/release-common.sh"

main() {
    local version
    local tag
    local branch
    local head
    local tag_target
    local subject

    release_cd_root

    version="$(release_current_version)"
    release_validate_semver "$version"
    tag="v$version"
    subject="$(git log -1 --pretty=%s)"

    [[ "$subject" == release:* ]] || release_die "latest commit subject must begin with release:"
    release_tag_exists "$tag" || release_die "expected annotated tag $tag is missing"

    if [ "$(git cat-file -t "$tag")" != "tag" ]; then
        release_die "$tag exists but is not an annotated tag"
    fi

    head="$(git rev-parse HEAD)"
    tag_target="$(git rev-list -n 1 "$tag")"
    [ "$tag_target" = "$head" ] || release_die "$tag does not point to the current commit"

    branch="$(git branch --show-current)"
    [ -n "$branch" ] || release_die "current branch could not be determined"

    git push origin "$branch"
    git push origin "$tag"

    printf 'Pushed branch %s and tag %s.\n' "$branch" "$tag"
    printf 'GitHub Actions will create the GitHub Release from the pushed tag.\n'
}

main "$@"
