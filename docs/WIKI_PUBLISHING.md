# Publishing The Allp Wiki

The source-controlled wiki lives in `wiki/`. Keeping it in the main repository
makes documentation changes reviewable and versioned with the code.

GitHub hosts a repository wiki in a separate Git repository. A repository owner
must first enable **Settings → General → Features → Wikis** and create the first
wiki page if GitHub has not initialized the wiki remote.

After that one-time action, publish every reviewed page with one command:

```bash
gh auth login        # one time, if this shell is not authenticated
gh auth setup-git    # let Git use the GitHub CLI credential
make wiki-check
make wiki-publish
```

`wiki-check` clones the separate Wiki repository into a temporary directory and
shows the pending snapshot without committing or pushing. `wiki-publish` copies
all pages, removes stale Markdown pages in that temporary clone, commits once,
and pushes once. The main checkout and its branch are not committed or pushed.

Do not copy unreviewed generated files or credentials. The main repository's
`wiki/` directory remains the source of truth; edit it first, review it through
a normal pull request, and then publish the same files to the wiki remote.

GitHub recognizes `Home.md` as the entry page and `_Sidebar.md` as navigation.
The Persian sidebar remains available as `_Sidebar.fa.md`; every Persian page
also links back to the Persian home so language navigation does not depend on
GitHub's global sidebar behavior.

Every `*.fa.md` page is wrapped in a sanitized `<div dir="rtl" align="right">`
container. GitHub therefore lays out Persian prose right-to-left while fenced
command examples retain their literal command text.
