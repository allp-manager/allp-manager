# AUR with yay and paru

Allp delegates AUR work to an installed yay or paru. It does not implement a
PKGBUILD builder, install helpers automatically, or bypass their review prompts.

```bash
allp detect --verbose
allp search example-bin --from aur
allp install example-bin --from yay --dry-run
allp install example-bin --from paru
allp info example-bin --from aur
allp list --from aur
allp remove example-bin --from aur --dry-run
allp upgrade --dry-run
allp upgrade
allp upgrade --from paru --dry-run
```

There is one AUR backend, with `aur`, `yay` and `paru` selectors. When both
helpers are installed, default maintenance uses yay once; select paru explicitly
to prefer it. Search exposes the installed helper choices. Installation keeps
the selected helper. AUR search and info use AUR-only queries. Malformed search
output remains an error/partial result, not an empty successful lookup.

Default upgrades run `pacman -Syu` for repository packages, then `yay -Sua` or
`paru -Sua` for AUR packages. This avoids running two full Pacman upgrades or
upgrading AUR twice. Failure of the Pacman operation defers the AUR operation;
`--allow-stale-metadata` cannot bypass that dependency. Selecting `--from aur`
or a helper alone does not add a Pacman operation. Keep the system upgraded
before building AUR packages. Other detected supported managers remain eligible
unless `--scope` or `--from` restricts them.

Build/install/upgrade plans run as the invoking user or the original sudo user,
never as direct root. Helpers manage their own Pacman elevation. Allp's `--yes`
does not add `--noconfirm`, skip review, or hide native prompts. The live dashboard
is disabled for maintenance runs with AUR so helper output and review stay in
the ordinary terminal.

Foreign package inventory uses `pacman -Qm`, even without a helper, and is
deduplicated against Pacman's inventory. Foreign packages are absent from the
configured sync repositories; they may be AUR packages **or local builds**.
Profiles preserve this distinction as `backend = "aur"` rather than attempting
`pacman -S` for a foreign ID. Restore must find an exact AUR package, and local
builds without an AUR recipe need manual handling. Without a helper the backend
supports inventory and removal through Pacman, and reports upgrades as not
applicable with helper-installation guidance.

The command boundary is covered by fixtures and fake-PATH tests. A real Arch
host must still validate build dependencies, PKGBUILD review/editor behavior,
sudo policy, network failures and installation before release promotion.

Sources: [yay manual](https://jguer.github.io/yay/man.html),
[paru README](https://github.com/Morganamilo/paru),
[makepkg documentation](https://wiki.archlinux.org/title/Makepkg).
