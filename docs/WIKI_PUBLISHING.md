# Publishing The Allp Wiki

The source-controlled wiki lives in `wiki/`. Keeping it in the main repository
makes documentation changes reviewable and versioned with the code.

GitHub hosts a repository wiki in a separate Git repository. A repository owner
must first enable **Settings → General → Features → Wikis** and create the first
wiki page if GitHub has not initialized the wiki remote.

After that one-time action, publish a reviewed snapshot:

```bash
git clone https://github.com/allp-manager/allp-manager.wiki.git /tmp/allp-manager-wiki
cp wiki/*.md /tmp/allp-manager-wiki/
cd /tmp/allp-manager-wiki
git add --all
git commit -m "docs: publish bilingual Allp wiki"
git push origin HEAD
```

Do not copy unreviewed generated files or credentials. The main repository's
`wiki/` directory remains the source of truth; edit it first, review it through
a normal pull request, and then publish the same files to the wiki remote.

GitHub recognizes `Home.md` as the entry page and `_Sidebar.md` as navigation.
The Persian sidebar remains available as `_Sidebar.fa.md`; every Persian page
also links back to the Persian home so language navigation does not depend on
GitHub's global sidebar behavior.
