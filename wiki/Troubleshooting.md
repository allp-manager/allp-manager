# Troubleshooting

[← Wiki home](Home.md) · [فارسی](Troubleshooting.fa.md)

Start with read-only evidence:

```bash
allp --version --verbose
allp doctor
allp detect --verbose
allp detect --json >allp-detect.json
```

| Symptom | Safe response |
|---|---|
| Backend missing | Confirm the native executable and inspect `detect --verbose`; bootstrap only after reviewing its separate plan. |
| Backend busy / lock | Wait for or inspect the owning process. Never delete dpkg/rpm lock files. |
| Search says incomplete | Inspect backend issues; unknown parser output is not a valid “no match”. |
| Multiple noninteractive matches | Add an exact package ID and `--from <backend>`. |
| Flatpak has no remotes | Run `doctor flatpak`; explicitly review the offered Flathub user-remote plan. |
| Snap exact resolution fails | Run `doctor snap`; authoritative REST not-found is final, while transport failures may permit CLI fallback. |
| npm permission failure | Fix prefix ownership or use a user-owned Node manager; do not sudo global npm through Allp. |
| Cargo upgrade unavailable | Install `cargo-update` intentionally or manage binary crates manually. |
| Bazzite host package change | Prefer Flatpak, Homebrew or containers; review rpm-ostree layering and reboot requirements. |
| Self-update unavailable | Run `allp self-update --check-only -v`; target or manifest failure leaves the installed binary unchanged. |
| Old binary still runs | Use `command -v allp`, `make install-check`, then refresh shell hashing with `hash -r` or `rehash`. |
| Live UI is unsuitable | Add `--no-tui`; redirected, JSON and noninteractive runs already fall back. |

For APT stale metadata after a failed refresh, fix the underlying problem and
retry. `--allow-stale-metadata` is an explicit recovery override, not a normal
operating mode.

When filing a bug, include the exact command, distro/tool versions, expected
behavior, actual native output, and sanitized `allp detect --json`. Do not post
credentials or vulnerability details publicly.
