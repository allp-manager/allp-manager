# Release Drafts

`make release-prepare` writes tracked release metadata here:

- `RELEASE_TITLE_vX.Y.Z.txt`
- `RELEASE_NOTES_vX.Y.Z.md`

The post-commit hook copies the matching draft into `dist/` only after a
prepared commit whose subject begins with `release:`. The generated `dist/`
directory is intentionally ignored and local-only.

Publishing is explicit: `make release-push` verifies the local release commit
and annotated tag, then pushes the branch and tag. The tag-triggered GitHub
Actions workflow creates the GitHub Release from these tracked files.

The tag workflow checks out the exact tag, runs the full quality gate, builds
and tests the advertised Linux/macOS/Windows targets, creates one checksum per
binary archive, creates the source archive and checksum, and generates
`allp-release-manifest.json`. `MINIMUM_UPDATER_VERSION.txt` sets the oldest
updater accepted by that manifest. An existing GitHub Release is never silently
overwritten.
