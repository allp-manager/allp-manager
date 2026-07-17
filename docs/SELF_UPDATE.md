# Self-Update Architecture

`allp self-update` and phase 1 of `allp update` use the trusted repository constant `Aliazadi-1776/allp`. User-controlled repository URLs are not accepted.

`GitHubReleaseSource` retrieves release metadata over HTTPS with bounded redirects, time, and response size. Stable mode ignores drafts and prereleases; prerelease mode is explicit. Strict three-part semantic versions are compared numerically. Conditional ETags, selected channel, timestamps, and seen/attempted/successful versions are stored in the platform state directory without credentials.

Every usable release must include `allp-release-manifest.json`. Asset selection matches OS, architecture, libc where applicable, executable name, and target triple. A missing target is structured `UnsupportedTarget` and leaves the current installation untouched.

## Verification And Replacement

The updater validates the exact official repository/tag/asset URL, maximum size, SHA-256, archive paths, expected binary, and staged `--version`. Symlinks, hard links, traversal, foreign assets, and mismatched versions fail before replacement.

Linux and macOS copy the verified binary to the installed binary's directory, preserve mode and ownership, create a rollback backup, rename atomically, verify again, and restore the backup on failure. A non-writable installation displays and elevates only the internal replacement helper.

Windows copies a verified helper into staging and defers replacement until the current process exits. The helper keeps rollback semantics, launches the new binary with a completion/version marker, and lets the new process clean staging.

After a successful `allp update` self-replacement, the new binary receives `ALLP_SELF_UPDATE_COMPLETED=1`, preserves the original CLI arguments and a small allowlist of environment variables, skips another self-check, and continues backend updates once.

`--skip-self-update`, `--self-only`, `--check-only`, and `--offline` have independent meanings. A normal update may continue after a reported self-update/network failure; `--self-only` returns the failure without backend mutation.
