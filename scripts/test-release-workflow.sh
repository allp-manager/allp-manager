#!/usr/bin/env bash
set -euo pipefail

SOURCE_ROOT="$(git rev-parse --show-toplevel)"
TMP_ROOT="$(mktemp -d)"

cleanup() {
    rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

pass() {
    printf 'ok - %s\n' "$*"
}

make_fixture() {
    local name="$1"
    local dir="$TMP_ROOT/$name"

    mkdir -p "$dir/scripts" "$dir/.githooks" "$dir/src" "$dir/release"
    cp "$SOURCE_ROOT/scripts/release-common.sh" "$dir/scripts/"
    cp "$SOURCE_ROOT/scripts/release-prepare.sh" "$dir/scripts/"
    cp "$SOURCE_ROOT/scripts/release-finalize.sh" "$dir/scripts/"
    cp "$SOURCE_ROOT/scripts/release-status.sh" "$dir/scripts/"
    cp "$SOURCE_ROOT/.githooks/post-commit" "$dir/.githooks/"
    chmod +x "$dir/scripts/"*.sh "$dir/.githooks/post-commit"

    cat >"$dir/Cargo.toml" <<'EOF'
[package]
name = "allp"
version = "0.1.0"
edition = "2021"
EOF

    cat >"$dir/src/main.rs" <<'EOF'
fn main() {
    println!("allp 0.1.0");
}
EOF

    cat >"$dir/CHANGELOG.md" <<'EOF'
# Changelog

All notable changes to Allp will be documented in this file.

## [Unreleased]
EOF

    cat >"$dir/README.md" <<'EOF'
# Allp

Current version: **0.1.0**
EOF

    cat >"$dir/README.fa.md" <<'EOF'
# Allp

نسخه فعلی: **0.1.0**
EOF

    cat >"$dir/.gitignore" <<'EOF'
/target/
/dist/
/.release-state/
EOF

    cat >"$dir/Makefile" <<'EOF'
SHELL := /bin/sh
BASH ?= bash
BUMP ?= patch
VERSION ?=
DIST_DIR ?= dist
RELEASE_PREFIX ?= allp

.PHONY: quality release-prepare hooks-install release-status

quality:
	cargo check --all-targets

release-prepare:
	BUMP="$(BUMP)" VERSION="$(VERSION)" DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-prepare.sh

hooks-install:
	git config core.hooksPath .githooks

release-status:
	DIST_DIR="$(DIST_DIR)" RELEASE_PREFIX="$(RELEASE_PREFIX)" $(BASH) scripts/release-status.sh
EOF

    (
        cd "$dir"
        git init -q
        git config user.email "allp-tests@example.invalid"
        git config user.name "Allp Release Tests"
        cargo generate-lockfile >/dev/null
        git add -A
        git commit -q -m "chore: initial fixture"
    )

    printf '%s\n' "$dir"
}

assert_no_release_output() {
    [ -z "$(git tag)" ] || fail "unexpected tag: $(git tag)"
    [ ! -d dist ] || [ -z "$(find dist -type f -print -quit)" ] || fail "unexpected dist output"
}

test_ordinary_commit_skips_release() {
    local repo

    repo="$(make_fixture ordinary)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        printf '\nordinary change\n' >>README.md
        git add README.md
        git commit -q -m "fix: improve Snap parsing"
        assert_no_release_output
    )
    pass "ordinary commit did not tag or build artifacts"
}

test_release_subject_without_marker_skips_release() {
    local repo

    repo="$(make_fixture no-marker)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        printf '\nmanual release subject without marker\n' >>README.md
        git add README.md
        git commit -q -m "release: Allp v0.1.1"
        assert_no_release_output
    )
    pass "release subject without marker did not tag or build artifacts"
}

test_valid_release_commit_finalizes() {
    local repo
    local tag_commit
    local head_commit
    local checksum_value

    repo="$(make_fixture valid-release)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        make release-prepare BUMP=patch >/dev/null
        git add -A
        git commit -q -m "release: Allp v0.1.1"

        [ "$(git tag --list v0.1.1)" = "v0.1.1" ] || fail "v0.1.1 tag missing"
        tag_commit="$(git rev-list -n 1 v0.1.1)"
        head_commit="$(git rev-parse HEAD)"
        [ "$tag_commit" = "$head_commit" ] || fail "tag does not point at release commit"

        [ -f dist/allp-v0.1.1-source.tar.gz ] || fail "source archive missing"
        [ -f dist/allp-v0.1.1-source.tar.gz.sha256 ] || fail "checksum missing"
        [ -f dist/RELEASE_NOTES_v0.1.1.md ] || fail "final release notes missing"

        tar -tzf dist/allp-v0.1.1-source.tar.gz | awk 'index($0, "allp-v0.1.1/") != 1 { bad = 1 } END { exit bad }'
        ! tar -tzf dist/allp-v0.1.1-source.tar.gz | grep -Eq '(^|/)(\.git|target|dist|\.release-state)(/|$)'
        (cd dist && sha256sum -c allp-v0.1.1-source.tar.gz.sha256 >/dev/null)

        checksum_value="$(awk '{ print $1; exit }' dist/allp-v0.1.1-source.tar.gz.sha256)"
        grep -q 'Allp v0.1.1' dist/RELEASE_NOTES_v0.1.1.md
        grep -q "$checksum_value" dist/RELEASE_NOTES_v0.1.1.md
    )
    pass "valid release commit created local tag and artifacts from tag"
}

test_marker_mismatch_does_not_finalize() {
    local repo

    repo="$(make_fixture mismatch)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        make release-prepare BUMP=patch >/dev/null
        sed -i 's/version = "0.1.1"/version = "0.1.2"/' Cargo.toml
        git add -A
        git commit -m "release: Allp v0.1.2" >/dev/null || true
        assert_no_release_output
    )
    pass "marker/version mismatch did not finalize"
}

test_existing_tag_is_not_overwritten() {
    local repo
    local original_tag_commit
    local current_head

    repo="$(make_fixture existing-tag)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        make release-prepare BUMP=patch >/dev/null
        git tag -a v0.1.1 -m "pre-existing test tag" HEAD
        original_tag_commit="$(git rev-list -n 1 v0.1.1)"
        git add -A
        git commit -m "release: Allp v0.1.1" >/dev/null || true
        current_head="$(git rev-parse HEAD)"
        [ "$(git rev-list -n 1 v0.1.1)" = "$original_tag_commit" ] || fail "existing tag was moved"
        [ "$(git rev-list -n 1 v0.1.1)" != "$current_head" ] || fail "existing tag unexpectedly points at release commit"
        [ ! -d dist ] || [ -z "$(find dist -type f -print -quit)" ] || fail "dist output created despite existing tag"
    )
    pass "existing tag was not overwritten"
}

test_existing_archive_is_not_overwritten() {
    local repo

    repo="$(make_fixture existing-archive)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        make release-prepare BUMP=patch >/dev/null
        git add -A
        git commit -q -m "release: Allp v0.1.1"
        if scripts/release-finalize.sh --archive >/dev/null 2>&1; then
            fail "existing archive was overwritten"
        fi
    )
    pass "existing archive was not overwritten"
}

test_bump_modes_and_invalid_versions() {
    local repo

    repo="$(make_fixture bump-patch)"
    (cd "$repo" && make release-prepare BUMP=patch >/dev/null && grep -q 'version = "0.1.1"' Cargo.toml)

    repo="$(make_fixture bump-minor)"
    (cd "$repo" && make release-prepare BUMP=minor >/dev/null && grep -q 'version = "0.2.0"' Cargo.toml)

    repo="$(make_fixture bump-major)"
    (cd "$repo" && make release-prepare BUMP=major >/dev/null && grep -q 'version = "1.0.0"' Cargo.toml)

    repo="$(make_fixture explicit)"
    (cd "$repo" && make release-prepare VERSION=0.5.0 >/dev/null && grep -q 'version = "0.5.0"' Cargo.toml)

    repo="$(make_fixture downgrade)"
    if (cd "$repo" && make release-prepare VERSION=0.0.9 >/dev/null 2>&1); then
        fail "downgrade was accepted"
    fi

    repo="$(make_fixture invalid-version)"
    if (cd "$repo" && make release-prepare VERSION=0.2 >/dev/null 2>&1); then
        fail "invalid semantic version was accepted"
    fi

    pass "bump modes and invalid version checks"
}

test_release_scripts_do_not_use_sudo() {
    if grep -R '\bsudo\b' "$SOURCE_ROOT/scripts/release-"*.sh "$SOURCE_ROOT/.githooks/post-commit" >/dev/null; then
        fail "release automation must not use sudo"
    fi
    pass "release automation does not use sudo"
}

test_ordinary_commit_skips_release
test_release_subject_without_marker_skips_release
test_valid_release_commit_finalizes
test_marker_mismatch_does_not_finalize
test_existing_tag_is_not_overwritten
test_existing_archive_is_not_overwritten
test_bump_modes_and_invalid_versions
test_release_scripts_do_not_use_sudo

printf 'All release workflow tests passed in temporary repositories.\n'
