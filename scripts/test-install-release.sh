#!/usr/bin/env bash
set -euo pipefail

source_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/allp-installer-tests.XXXXXX")"
trap 'rm -rf -- "$test_root"' EXIT
export ALLP_TEST_ROOT="$test_root"
host_os="$(/usr/bin/uname -s)"
export ALLP_TEST_OS="$host_os"
native_sha256sum="$(command -v sha256sum || true)"
export ALLP_TEST_SHA256SUM="$native_sha256sum"
write_checksum() {
    if [ -n "$native_sha256sum" ]; then "$native_sha256sum" "$archive";
    else /usr/bin/shasum -a 256 "$archive"; fi
}
mkdir -p "$test_root/helpers" "$test_root/assets" "$test_root/payload"
printf 'new verified Allp fixture\n' > "$test_root/payload/allp"
archive='allp-v9.8.7-x86_64-unknown-linux-gnu.tar.gz'
if [ "$host_os" = Darwin ]; then archive='allp-v9.8.7-x86_64-apple-darwin.tar.gz'; fi
tar -czf "$test_root/assets/$archive" -C "$test_root/payload" allp
(cd "$test_root/assets" && write_checksum > "$archive.sha256")

# The installer sees fixture downloads and never contacts GitHub. Existing host
# installations are treated as already identical; only fixture paths can change.
cat > "$test_root/helpers/curl" <<'SH'
#!/bin/sh
output=''
while [ "$#" -gt 0 ]; do
    case "$1" in
        --output) output="$2"; shift ;;
        https://*) url="$1" ;;
    esac
    shift
done
if [ "${url##*/}" = allp-release-manifest.json ]; then
    printf '{"version":"9.8.7"}\n' > "$output"
else
    /usr/bin/cp "$ALLP_TEST_ROOT/assets/${url##*/}" "$output"
fi
SH
cat > "$test_root/helpers/uname" <<'SH'
#!/bin/sh
case "$1" in -s) printf '%s\n' "$ALLP_TEST_OS";; -m) printf 'x86_64\n';; esac
SH
cat > "$test_root/helpers/sha256sum" <<'SH'
#!/bin/sh
if [ -n "$ALLP_TEST_SHA256SUM" ]; then exec "$ALLP_TEST_SHA256SUM" "$@";
else exec /usr/bin/shasum -a 256 "$@"; fi
SH
cat > "$test_root/helpers/cmp" <<'SH'
#!/bin/sh
for last_argument in "$@"; do :; done
case "$last_argument" in
    "$ALLP_TEST_ROOT"/*)
        if [ "${ALLP_TEST_FAIL_VERIFY:-}" = "$last_argument" ]; then exit 1; fi
        exec /usr/bin/cmp "$@"
        ;;
    *) exit 0 ;;
esac
SH
cat > "$test_root/helpers/mv" <<'SH'
#!/bin/sh
for last_argument in "$@"; do :; done
case "$*" in
    */new*)
        if [ "${ALLP_TEST_FAIL_RENAME:-}" = "$last_argument" ]; then exit 1; fi
        ;;
esac
exec /usr/bin/mv "$@"
SH
cat > "$test_root/helpers/install" <<'SH'
#!/bin/sh
for last_argument in "$@"; do :; done
case "$last_argument" in
    "$ALLP_TEST_ROOT"/*)
        if [ -n "${ALLP_TEST_FAIL_STAGE:-}" ] &&
            [ "${last_argument#"$ALLP_TEST_FAIL_STAGE"}" != "$last_argument" ]; then exit 1; fi
        exec /usr/bin/install "$@"
        ;;
    *) printf 'test refused a write outside the fixture: %s\n' "$last_argument" >&2; exit 1 ;;
esac
SH
cat > "$test_root/helpers/dpkg-query" <<'SH'
#!/bin/sh
for last_argument in "$@"; do :; done
[ "${ALLP_TEST_MANAGED:-}" = "$last_argument" ]
SH
cat > "$test_root/helpers/rpm" <<'SH'
#!/bin/sh
exit 1
SH
cp "$test_root/helpers/rpm" "$test_root/helpers/pacman"
cat > "$test_root/helpers/id" <<'SH'
#!/bin/sh
printf '%s\n' "${ALLP_TEST_UID:-1000}"
SH
cat > "$test_root/helpers/stat" <<'SH'
#!/bin/sh
for last_argument in "$@"; do :; done
if [ "$2" = '%u' ]; then
    if [ "${ALLP_TEST_FOREIGN:-}" = "$last_argument" ]; then printf '0\n';
    else printf '1000\n'; fi
else exec /usr/bin/stat "$@"; fi
SH
chmod +x "$test_root/helpers/"*
export PATH="$test_root/helpers:/usr/bin:/bin"

pass() { printf 'ok - %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
fixture() {
    case_dir="$test_root/$1"
    mkdir -p "$case_dir/main" "$case_dir/other"
    printf 'old primary\n' > "$case_dir/main/allp"
    printf 'old secondary\n' > "$case_dir/other/allp"
    chmod 700 "$case_dir/main/allp"
    export ALLP_INSTALL_DIR="$case_dir/main"
    unset ALLP_TEST_FAIL_RENAME ALLP_TEST_FAIL_VERIFY ALLP_TEST_FAIL_STAGE ALLP_TEST_MANAGED ALLP_TEST_FOREIGN ALLP_TEST_UID
}
run_installer() {
    sh "$source_root/scripts/install-release.sh" "$@" > "$case_dir/output" 2>&1
}
unchanged() {
    [ "$(< "$case_dir/main/allp")" = 'old primary' ] || fail 'primary changed on failure'
    [ "$(< "$case_dir/other/allp")" = 'old secondary' ] || fail 'secondary changed on failure'
}

fixture help
run_installer --help
grep -q -- '--sync-existing' "$case_dir/output"
unchanged
pass 'help performs no installation'

fixture legacy
run_installer 9.8.7
/usr/bin/cmp "$test_root/payload/allp" "$case_dir/main/allp"
grep -q 'backup:' "$case_dir/output"
pass 'legacy exact-version argument works and keeps the old binary'

fixture sync
run_installer --sync-existing 9.8.7 --replace-existing "$case_dir/other/allp"
/usr/bin/cmp "$test_root/payload/allp" "$case_dir/main/allp"
/usr/bin/cmp "$case_dir/main/allp" "$case_dir/other/allp"
grep -q 'old primary' "$case_dir/main/"allp.backup.*/previous
grep -q 'old secondary' "$case_dir/other/"allp.backup.*/previous
run_installer latest --sync-existing --replace-existing "$case_dir/other/allp"
pass 'multiple standalone copies are identical, backed up, and safely reinstallable'

fixture shadow
if PATH="$case_dir/other:$PATH" run_installer 9.8.7; then fail 'unselected PATH copy was ignored'; fi
unchanged
grep -q 'another PATH installation' "$case_dir/output"
pass 'unselected PATH shadow stops installation before replacement'

fixture explicit_shadow
PATH="$case_dir/other:$PATH" run_installer 9.8.7 --replace-existing "$case_dir/other/allp"
/usr/bin/cmp "$case_dir/main/allp" "$case_dir/other/allp"
pass 'a custom PATH copy requires explicit selection'

fixture symlink
mv "$case_dir/other/allp" "$case_dir/other/actual"
ln -s actual "$case_dir/other/allp"
if run_installer 9.8.7 --replace-existing "$case_dir/other/allp"; then fail 'symlink accepted'; fi
unchanged
[ -L "$case_dir/other/allp" ] || fail 'symlink was modified'
pass 'symlink destinations and their targets are untouched'

fixture managed
export ALLP_TEST_MANAGED="$case_dir/other/allp"
if run_installer 9.8.7 --replace-existing "$case_dir/other/allp"; then fail 'package-owned target accepted'; fi
unchanged
pass 'native-package-owned binaries are not overwritten'

fixture foreign_owner
export ALLP_TEST_FOREIGN="$case_dir/other/allp"
if run_installer 9.8.7 --replace-existing "$case_dir/other/allp"; then fail 'foreign-owned target accepted'; fi
unchanged
grep -q 'belongs to another user' "$case_dir/output"
pass 'a copy needing another owner or administrator stops all replacements'

fixture unwritable
chmod 500 "$case_dir/other"
unwritable_status=0
run_installer 9.8.7 --replace-existing "$case_dir/other/allp" || unwritable_status=$?
chmod 700 "$case_dir/other"
# uid 0 bypasses filesystem write bits; foreign-owner coverage still runs in CI.
if [ "$(/usr/bin/id -u)" != 0 ]; then
    [ "$unwritable_status" != 0 ] || fail 'unwritable directory accepted'
    unchanged
    pass 'an unwritable second directory leaves the first installation untouched'
fi

fixture directory_link
ln -s "$case_dir/other" "$case_dir/alias"
if run_installer 9.8.7 --replace-existing "$case_dir/alias/allp"; then fail 'directory symlink accepted'; fi
unchanged
pass 'linked destination directories are not traversed for installation'

fixture root_trust
export ALLP_TEST_UID=0
if run_installer 9.8.7; then fail 'root installed in a user-controlled fixture'; fi
unchanged
grep -q 'trusted root-owned directories' "$case_dir/output"
pass 'root installation refuses user-controlled directories'

fixture stage_failure
export ALLP_TEST_FAIL_STAGE="$case_dir/other/allp.backup."
if run_installer 9.8.7 --replace-existing "$case_dir/other/allp"; then fail 'staging failure ignored'; fi
unchanged
pass 'all copies stay unchanged if staging any copy fails'

fixture rename_failure
export ALLP_TEST_FAIL_RENAME="$case_dir/other/allp"
if run_installer 9.8.7 --replace-existing "$case_dir/other/allp"; then fail 'rename failure ignored'; fi
unchanged
if [ "$host_os" = Darwin ]; then original_mode="$(/usr/bin/stat -f '%Lp' "$case_dir/main/allp")";
else original_mode="$(/usr/bin/stat -c '%a' "$case_dir/main/allp")"; fi
[ "$original_mode" = 700 ] || fail 'rollback lost original mode'
pass 'failure on the second replacement rolls back the first and preserves its mode'

fixture verification_failure
export ALLP_TEST_FAIL_VERIFY="$case_dir/other/allp"
if run_installer 9.8.7 --replace-existing "$case_dir/other/allp"; then fail 'verification failure ignored'; fi
unchanged
pass 'post-install verification failure rolls back every replaced copy'

fixture bad_checksum
cp "$test_root/assets/$archive.sha256" "$case_dir/good_checksum"
printf '%064d  %s\n' 0 "$archive" > "$test_root/assets/$archive.sha256"
if run_installer 9.8.7 --replace-existing "$case_dir/other/allp"; then fail 'bad checksum accepted'; fi
unchanged
cp "$case_dir/good_checksum" "$test_root/assets/$archive.sha256"
pass 'an invalid release checksum changes no installed binary'

fixture archive_paths
cp "$test_root/assets/$archive" "$case_dir/good_archive"
printf 'unrelated\n' > "$test_root/payload/extra"
tar -czf "$test_root/assets/$archive" -C "$test_root/payload" allp extra
(cd "$test_root/assets" && write_checksum > "$archive.sha256")
if run_installer 9.8.7; then fail 'unexpected archive path accepted'; fi
unchanged
cp "$case_dir/good_archive" "$test_root/assets/$archive"
(cd "$test_root/assets" && write_checksum > "$archive.sha256")
pass 'an archive with unexpected files changes no installation'

fixture archive_symlink
mkdir "$case_dir/linked_payload"
ln -s "$case_dir/other/allp" "$case_dir/linked_payload/allp"
tar -czf "$test_root/assets/$archive" -C "$case_dir/linked_payload" allp
(cd "$test_root/assets" && write_checksum > "$archive.sha256")
if run_installer 9.8.7; then fail 'archived symlink accepted'; fi
unchanged
pass 'an archived symlink cannot become the installed binary'

fixture arguments
if run_installer --replace-existing; then fail 'missing path accepted'; fi
if run_installer --replace-existing /tmp/not-allp; then fail 'wrong filename accepted'; fi
if run_installer 9.8.7 1.2.3; then fail 'extra version accepted'; fi
unchanged
pass 'invalid arguments cause no installation'

printf '%s\n' 'Release installer regression tests passed.'
