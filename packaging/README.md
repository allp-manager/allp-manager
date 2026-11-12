# Distributing Allp through package managers

The signed Allp APT feed is hosted at
[allp-manager.github.io/apt](https://allp-manager.github.io/apt/).
Stable GitHub releases trigger the tested AUR publication workflow automatically.
This does not imply acceptance into a distribution's official repositories.

| Ecosystem | Prepared path | Publication still needed |
|---|---|---|
| Debian/Ubuntu APT | Published signed Stable feed for amd64/arm64 | One-time source setup; official distro inclusion is separate |
| Fedora/RHEL DNF and compatible RPM tools | RPM spec and native builder | Test supported distribution versions; signed RPM repository/COPR or upstream acceptance |
| Arch/AUR | Published `allp-manager` source package and `aur.yml` workflow | Automatic update after a successful Stable GitHub release; six-hour reconciliation retries |
| Homebrew/Linuxbrew | Checksum-based source formula template/generator | Publish a tap repository; homebrew/core has its own acceptance criteria |
| Snap/Flathub | Not packaged as a sandboxed Allp app | Host orchestration needs a separate confinement/host-access design |

## DEB and RPM

For the published Stable package, follow the
[one-time APT source/key setup](https://allp-manager.github.io/apt/), then run:

```bash
sudo apt update
sudo apt install allp
```

The initial v0.6.5 package requires glibc 2.39 (Ubuntu 24.04+ / Debian 13+).
The feed contains published Stable builds, not uncommitted development changes.
The [APT maintenance guide](apt/MAINTAINING.md) describes signing, automated
metadata refresh, isolated tests and the dedicated hosting repository.

Build on the intended target distribution and architecture, using its native
packaging tools. Shared-library dependencies are derived from the actual ELF;
do not build on a newer libc and assume the package supports older systems.
The builder accepts native x86_64 and aarch64 Linux binaries and does not
install, sign, upload, enable a service, or install optional package managers.

```bash
cargo build --locked --release
# Debian/Ubuntu: dpkg-dev and binutils must be available.
bash scripts/package-native.sh deb target/release/allp dist/packages
# On an RPM build host with rpm-build and binutils:
bash scripts/package-native.sh rpm target/release/allp dist/packages
# Or: make package-deb / make package-rpm
# Inspect/extract without installing on the host:
bash scripts/test-native-packages.sh target/release/allp
```

Review the result with `dpkg-deb --info FILE.deb` or `rpm -qpi FILE.rpm`.
Install intentionally using `sudo apt install ./FILE.deb` or
`sudo dnf install ./FILE.rpm`. The binary installs to `/usr/bin/allp`; Allp's
self-update ownership checks defer upgrades of package-owned binaries to their
native manager. No native managers are mandatory runtime dependencies.

The manually dispatched **Native package artifacts** workflow builds and tests
DEB/RPM installation in disposable runners and uploads artifacts only. It does
not publish releases or repositories. Local development binaries retain their
build revision in the package version; stable distribution artifacts must be
built from a reviewed stable tag with matching build identity.

## Arch and AUR

Install the [allp-manager AUR package](https://aur.archlinux.org/packages/allp-manager)
with `yay -S allp-manager` or `paru -S allp-manager`. The installed command is
`allp`; later package upgrades go through the same AUR helper.

After a successful **Release** workflow, **AUR package** resolves the published
Stable tag and commit, verifies its source manifest/checksum, builds and tests
the package as a regular Arch user, installs it in a disposable container, then
pushes PKGBUILD and `.SRCINFO` using `AUR_SSH_PRIVATE_KEY`. The installed binary
retains the tagged source commit and is marked as a distribution build.
Prereleases, failed/mismatched runs and version downgrades cannot publish.

The workflow also reconciles the latest Stable release every six hours, so a
temporary AUR outage can recover automatically. It skips versions already in
AUR. For a manual retry, run **AUR package** from `main` with the Stable tag and
`publish: true`; `publish: false` builds/tests without pushing. Build artifacts
contain PKGBUILD and `.SRCINFO`, never the SSH key.

## Homebrew tap

Download the exact tagged GitHub source archive, verify its origin/tag, then:

```bash
python3 scripts/generate-homebrew-formula.py --version X.Y.Z \
  --source allp-manager-X.Y.Z.tar.gz --output dist/tap/Formula/allp.rb
```

The generator checks the archived Cargo version and computes SHA-256 from the
archive bytes. Test the generated formula with `brew install --build-from-source`
and `brew test` on supported Linux/macOS hosts before publishing it to a tap.
No published tap is assumed here.

## Repository publication

A direct `.deb` download is sufficient for `apt install ./FILE.deb`; unqualified
`apt install allp` requires a configured repository containing the package.
APT publication needs package indexes, signed Release/InRelease metadata, a
signing key, hosting and a supported distribution/architecture policy; the Allp
feed provides these. RPM
publication similarly needs repository metadata and signing. Keep keys outside
the repository and never disable signature checks to simplify installation.

Official documentation: [Debian packaging](https://www.debian.org/doc/manuals/debmake-doc/index.en.html),
[RPM guide](https://rpm-packaging-guide.github.io/),
[Homebrew taps](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap).
