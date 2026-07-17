# Release Drafts

`make release-prepare` writes tracked draft notes here as
`RELEASE_NOTES_vX.Y.Z.md`.

The post-commit hook copies the matching draft into `dist/` only after a
prepared commit whose subject begins with `release:`. The generated `dist/`
directory is intentionally ignored and local-only.
