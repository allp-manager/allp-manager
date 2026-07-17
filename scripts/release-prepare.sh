#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/release-common.sh
source "$SCRIPT_DIR/release-common.sh"

CARGO_BIN="${CARGO:-cargo}"
MAKE_BIN="${MAKE:-make}"

package_name() {
    awk '
        $0 == "[package]" { in_package = 1; next }
        /^\[/ && $0 != "[package]" { in_package = 0 }
        in_package && $1 == "name" && $2 == "=" {
            gsub(/"/, "", $3)
            print $3
            exit
        }
    ' Cargo.toml
}

write_cargo_version() {
    local target="$1"
    local tmp="Cargo.toml.release-tmp"

    awk -v target="$target" '
        $0 == "[package]" { in_package = 1; print; next }
        /^\[/ && $0 != "[package]" { in_package = 0 }
        in_package && !done && $1 == "version" && $2 == "=" {
            print "version = \"" target "\""
            done = 1
            next
        }
        { print }
        END {
            if (!done) {
                exit 2
            }
        }
    ' Cargo.toml >"$tmp" || {
        rm -f "$tmp"
        release_die "could not update package version in Cargo.toml"
    }

    mv "$tmp" Cargo.toml
}

replace_version_in_docs() {
    local current="$1"
    local target="$2"
    local file tmp

    for file in README.md README.fa.md; do
        [ -f "$file" ] || continue
        tmp="$file.release-tmp"
        awk -v current="$current" -v target="$target" '
            {
                gsub(current, target)
                print
            }
        ' "$file" >"$tmp"
        mv "$tmp" "$file"
    done
}

changelog_section() {
    local version="$1"
    local today="$2"

    cat <<EOF
## [$version] - $today

### Release Title

Allp v$version

### Added

- _Add release-specific additions before committing._

### Changed

- _Add release-specific changes before committing._

### Fixed

- _Add release-specific fixes before committing._

### Known Limitations

- _Move any release-relevant limitations here before committing._
EOF
}

update_changelog() {
    local version="$1"
    local today="$2"
    local tmp="CHANGELOG.md.release-tmp"

    if [ ! -f CHANGELOG.md ]; then
        {
            printf '# Changelog\n\n'
            printf 'All notable changes to Allp will be documented in this file.\n\n'
            printf '## [Unreleased]\n\n'
            changelog_section "$version" "$today"
            printf '\n'
        } >CHANGELOG.md
        return
    fi

    if grep -q "^## \\[$version\\]" CHANGELOG.md; then
        return
    fi

    awk -v version="$version" -v today="$today" '
        function print_section() {
            print "## [" version "] - " today
            print ""
            print "### Release Title"
            print ""
            print "Allp v" version
            print ""
            print "### Added"
            print ""
            print "- _Add release-specific additions before committing._"
            print ""
            print "### Changed"
            print ""
            print "- _Add release-specific changes before committing._"
            print ""
            print "### Fixed"
            print ""
            print "- _Add release-specific fixes before committing._"
            print ""
            print "### Known Limitations"
            print ""
            print "- _Move any release-relevant limitations here before committing._"
            print ""
        }
        {
            print
            if (!inserted && $0 ~ /^## \[Unreleased\]/) {
                print ""
                print_section()
                inserted = 1
            }
        }
        END {
            if (!inserted) {
                print ""
                print_section()
            }
        }
    ' CHANGELOG.md >"$tmp"
    mv "$tmp" CHANGELOG.md
}

write_release_notes_draft() {
    local version="$1"
    local tag="v$version"
    local notes
    local archive
    local checksum
    local previous_tag
    local range
    local commits

    notes="$(release_draft_notes_path "$version")"
    archive="$(release_archive_base "$version")"
    checksum="$(release_checksum_base "$version")"
    mkdir -p "$(dirname "$notes")"

    previous_tag="$(release_latest_tag)"
    if [ -n "$previous_tag" ]; then
        range="$previous_tag..HEAD"
    else
        range="HEAD"
    fi

    commits="$(git log --oneline --no-merges "$range" 2>/dev/null || true)"
    if [ -z "$commits" ]; then
        commits="- _No committed changes found yet; update this draft before committing if needed._"
    fi

    cat >"$notes" <<EOF
# Allp $tag

Local source release notes for Allp $tag.

## Changelog

See \`CHANGELOG.md\` section \`[$version]\`.

## Recent Commits

$commits

## Local Release Output

- Source archive: \`dist/$archive\`
- SHA-256 file: \`dist/$checksum\`
- Finalized notes: \`dist/RELEASE_NOTES_v$version.md\`

The archive is generated from the exact annotated tag \`$tag\` after the release commit.

## Checksum

SHA256: _generated during finalization_
EOF
}

write_release_title() {
    local version="$1"
    local title

    title="$(release_title_path "$version")"
    mkdir -p "$(dirname "$title")"
    printf 'Allp v%s — Modular Backend Recovery and Secure Self-Update\n' "$version" >"$title"
}

write_marker() {
    local version="$1"
    local bump="$2"
    local marker

    marker="$(release_marker_path)"
    mkdir -p "$(dirname "$marker")"
    {
        printf 'VERSION=%s\n' "$version"
        printf 'TAG=v%s\n' "$version"
        printf 'BUMP=%s\n' "$bump"
        printf 'PREPARED_HEAD=%s\n' "$(git rev-parse HEAD)"
        printf 'PREPARED_AT=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    } >"$marker"
}

main() {
    local current
    local target
    local bump="${BUMP:-patch}"
    local explicit="${VERSION:-}"
    local comparison
    local today
    local name
    local resume="false"

    release_cd_root

    [ -f Cargo.toml ] || release_die "Cargo.toml is missing"
    [ -f Makefile ] || release_die "Makefile is missing"

    if release_unresolved_conflicts; then
        release_die "unresolved merge conflicts must be resolved before preparing a release"
    fi

    name="$(package_name)"
    [ "$name" = "allp" ] || release_die "expected package name allp, found ${name:-<missing>}"

    current="$(release_current_version)"
    release_validate_semver "$current"

    if [ -n "$explicit" ]; then
        target="$explicit"
        bump="explicit"
    else
        target="$(release_bump_version "$current" "$bump")"
    fi

    release_validate_semver "$target"
    comparison="$(release_semver_cmp "$target" "$current")"
    if [ "$comparison" = "-1" ]; then
        release_die "target version $target must be greater than current version $current"
    fi
    if [ "$comparison" = "0" ]; then
        [ -n "$explicit" ] || release_die "target version $target must be greater than current version $current"
        [ -s "$(release_title_path "$target")" ] || release_die "cannot resume v$target without its release title"
        [ -s "$(release_draft_notes_path "$target")" ] || release_die "cannot resume v$target without its release notes"
        grep -q "^## \[$target\]" CHANGELOG.md || release_die "cannot resume v$target without its changelog section"
        resume="true"
    fi

    if release_tag_exists "v$target"; then
        release_die "tag v$target already exists"
    fi

    today="$(date '+%Y-%m-%d')"

    if [ "$resume" = "true" ]; then
        printf 'Resuming Allp v%s release preparation after an interrupted quality gate\n' "$target"
    else
        printf 'Preparing Allp v%s from v%s\n' "$target" "$current"
        write_cargo_version "$target"
        "$CARGO_BIN" check --all-targets
        replace_version_in_docs "$current" "$target"
        update_changelog "$target" "$today"
        write_release_title "$target"
        write_release_notes_draft "$target"
    fi

    printf '%s\n' 'Running full quality gate before writing release marker...'
    "$MAKE_BIN" quality

    write_marker "$target" "$bump"

    printf '\nPrepared local release v%s.\n' "$target"
    printf 'Commit the prepared files with this subject:\n\n'
    printf '  release: Allp v%s\n\n' "$target"
    printf '%s\n' 'After that commit, the post-commit hook will create the local tag and dist/ files.'
}

main "$@"
