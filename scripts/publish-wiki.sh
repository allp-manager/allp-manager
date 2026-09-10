#!/usr/bin/env sh
set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
wiki_source="$repository_root/wiki"
wiki_remote=${ALLP_WIKI_REMOTE:-https://github.com/allp-manager/allp-manager.wiki.git}
mode=${1:-publish}

case "$mode" in
  publish|--dry-run) ;;
  *)
    printf 'Usage: %s [publish|--dry-run]\n' "$0" >&2
    exit 2
    ;;
esac

test -f "$wiki_source/Home.md"
test -f "$wiki_source/Home.fa.md"
test -f "$wiki_source/_Sidebar.md"

if command -v gh >/dev/null 2>&1 && ! gh auth status >/dev/null 2>&1; then
  cat >&2 <<'EOF'
GitHub authentication is required before publishing the Wiki.
Run these once, then retry `make wiki-publish`:
  gh auth login
  gh auth setup-git
EOF
  exit 5
fi

publish_dir=$(mktemp -d "${TMPDIR:-/tmp}/allp-wiki-publish.XXXXXX")
cleanup() {
  rm -rf -- "$publish_dir"
}
trap cleanup EXIT HUP INT TERM

git clone --quiet "$wiki_remote" "$publish_dir"

# The reviewed main-repository wiki directory is the source of truth. Removing
# Markdown files only inside this temporary clone also cleans up accidentally
# created GitHub pages such as `Home.fa.md.md`.
find "$publish_dir" -maxdepth 1 -type f -name '*.md' -delete
cp "$wiki_source"/*.md "$publish_dir"/

git -C "$publish_dir" add --all
if git -C "$publish_dir" diff --cached --quiet; then
  printf '%s\n' 'Wiki is already synchronized.'
  exit 0
fi

printf 'Wiki pages ready: %s\n' "$(find "$wiki_source" -maxdepth 1 -type f -name '*.md' | wc -l | tr -d ' ')"
git -C "$publish_dir" diff --cached --stat

if [ "$mode" = "--dry-run" ]; then
  printf '%s\n' 'Dry run complete; nothing was committed or pushed.'
  exit 0
fi

git -C "$publish_dir" commit --quiet -m 'docs: publish bilingual Allp wiki'
git -C "$publish_dir" push origin HEAD
printf '%s\n' 'All Wiki pages were published successfully.'
