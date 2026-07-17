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

    mkdir -p "$dir/scripts" "$dir/.githooks" "$dir/src" "$dir/release" "$dir/docs" "$dir/tests"
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

    cat >"$dir/LICENSE" <<'EOF'
MIT
EOF

    cat >"$dir/docs/CLI_CONTRACT.md" <<'EOF'
# CLI Contract
EOF

    cat >"$dir/tests/smoke.rs" <<'EOF'
#[test]
fn smoke() {
    assert!(true);
}
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
	git config push.followTags true

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
        [ -f release/RELEASE_TITLE_v0.1.1.txt ] || fail "release title missing"

        tar -tzf dist/allp-v0.1.1-source.tar.gz | awk 'index($0, "allp-v0.1.1/") != 1 { bad = 1 } END { exit bad }'
        ! tar -tzf dist/allp-v0.1.1-source.tar.gz | grep -Eq '(^|/)(\.git|target|dist|\.release-state)(/|$)'
        tar -tzf dist/allp-v0.1.1-source.tar.gz | awk '$0 == "allp-v0.1.1/README.md" { found = 1 } END { exit !found }'
        tar -tzf dist/allp-v0.1.1-source.tar.gz | awk '$0 == "allp-v0.1.1/README.fa.md" { found = 1 } END { exit !found }'
        (cd dist && sha256sum -c allp-v0.1.1-source.tar.gz.sha256 >/dev/null)

        checksum_value="$(awk '{ print $1; exit }' dist/allp-v0.1.1-source.tar.gz.sha256)"
        grep -q 'Allp v0.1.1' dist/RELEASE_NOTES_v0.1.1.md
        grep -q "$checksum_value" dist/RELEASE_NOTES_v0.1.1.md
    )
    pass "valid release commit created local tag and artifacts from tag"
}

test_hooks_install_configures_local_follow_tags() {
    local repo

    repo="$(make_fixture hooks-config)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        [ "$(git config --local --get core.hooksPath)" = ".githooks" ] || fail "local hooks path missing"
        [ "$(git config --local --get push.followTags)" = "true" ] || fail "local push.followTags missing"
    )
    pass "hooks-install configures only repository-local release Git settings"
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

test_missing_release_title_or_notes_fail() {
    local repo

    repo="$(make_fixture missing-title)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        make release-prepare BUMP=patch >/dev/null
        rm -f release/RELEASE_TITLE_v0.1.1.txt
        git add -A
        git commit -m "release: Allp v0.1.1" >/dev/null || true
        assert_no_release_output
    )

    repo="$(make_fixture missing-notes)"
    (
        cd "$repo"
        make hooks-install >/dev/null
        make release-prepare BUMP=patch >/dev/null
        rm -f release/RELEASE_NOTES_v0.1.1.md
        git add -A
        git commit -m "release: Allp v0.1.1" >/dev/null || true
        assert_no_release_output
    )
    pass "missing release title or notes prevented release finalization"
}

test_github_release_workflow_is_tag_only() {
    local workflow="$SOURCE_ROOT/.github/workflows/release.yml"

    [ -f "$workflow" ] || fail "release workflow missing"
    grep -q 'tags:' "$workflow" || fail "release workflow is not tag-triggered"
    grep -q '"v\*\.\*\.\*"' "$workflow" || fail "semantic-version tag trigger missing"
    ! grep -q 'branches:' "$workflow" || fail "release workflow must not trigger on branches"
    grep -q 'contents: write' "$workflow" || fail "release workflow must be able to create releases"
    grep -q 'gh release create' "$workflow" || fail "release workflow does not create GitHub Release"
    pass "GitHub Release workflow is gated on semantic-version tags"
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

test_interrupted_prepare_can_resume_without_overwriting_notes() {
    local repo

    repo="$(make_fixture resume-prepare)"
    (
        cd "$repo"
        sed -i 's/version = "0.1.0"/version = "0.1.1"/' Cargo.toml
        cargo check --all-targets >/dev/null
        sed -i 's/0.1.0/0.1.1/g' README.md README.fa.md
        printf '\n## [0.1.1] - 2026-07-18\n' >>CHANGELOG.md
        printf '%s\n' 'Allp v0.1.1 test title' >release/RELEASE_TITLE_v0.1.1.txt
        printf '%s\n' '# carefully edited notes' >release/RELEASE_NOTES_v0.1.1.md
        make release-prepare VERSION=0.1.1 >/dev/null
        grep -q '^# carefully edited notes$' release/RELEASE_NOTES_v0.1.1.md || fail "resume overwrote release notes"
        grep -q '^VERSION=0.1.1$' .release-state/release.env || fail "resume did not write release marker"
    )
    pass "interrupted preparation resumes without overwriting release notes"
}

test_release_scripts_do_not_use_sudo() {
    if grep -R '\bsudo\b' "$SOURCE_ROOT/scripts/release-"*.sh "$SOURCE_ROOT/.githooks/post-commit" >/dev/null; then
        fail "release automation must not use sudo"
    fi
    pass "release automation does not use sudo"
}

test_ordinary_commit_skips_release
test_release_subject_without_marker_skips_release
test_hooks_install_configures_local_follow_tags
test_valid_release_commit_finalizes
test_marker_mismatch_does_not_finalize
test_existing_tag_is_not_overwritten
test_existing_archive_is_not_overwritten
test_missing_release_title_or_notes_fail
test_bump_modes_and_invalid_versions
test_interrupted_prepare_can_resume_without_overwriting_notes
test_github_release_workflow_is_tag_only
test_release_scripts_do_not_use_sudo

printf 'All release workflow tests passed in temporary repositories.\n'
