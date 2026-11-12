# Maintaining the Allp APT archive

The public feed is hosted by the dedicated `allp-manager/apt` repository at
`https://allp-manager.github.io/apt/`. Its `public/` directory contains only public
archive files; `tools/` contains copies of the archive preparation, signing and
integration-test scripts from the Allp source repository.

The archive repackages checksum-verified, already published Stable `vX.Y.Z`
GNU/Linux assets. It never publishes a local development build as Stable.
Both amd64 and arm64 are indexed. The initial v0.6.5 binaries require glibc 2.39:
Ubuntu 24.04+ and Debian 13+ meet that baseline. APT checks the actual dependency.
Older Debian/Ubuntu releases require separately built binaries with an older
libc baseline; do not weaken the dependency to make installation succeed.

## Publication

`packaging/apt/publish.yml` is the workflow template for the dedicated archive
repository, not a workflow to enable in the Allp source tree. In that repository:

1. Keep `tools/` synchronized with reviewed versions of the three APT scripts.
2. Enable GitHub Pages with the Actions build source.
3. Store the dedicated ASCII-armored private signing key in the encrypted
   `APT_SIGNING_KEY` repository secret. Set the full public fingerprint in the
   `APT_SIGNING_FINGERPRINT` repository variable.
4. Dispatch **Publish signed APT archive**, optionally selecting a Stable tag.

The preparation job downloads and verifies source and binary release checksums,
checks source versions and ELF architecture/dependencies, and packages the assets.
It does not have the archive signing key. A separate job signs metadata without
executing the downloaded binaries. Package files are immutable: a changed package
requires a new Debian revision or upstream version.

The workflow refreshes metadata weekly; Release metadata expires after 90 days.
Monitor failed workflow runs. The initial dedicated key expires in November 2028;
rotate it before expiry and publish a reviewed key-transition procedure for users.
Keep the private key and GnuPG revocation certificate backed up outside Git.
GitHub Actions revisions are pinned; update those pins deliberately.

## Local verification

```bash
python3 scripts/test-apt-repository.py
python3 scripts/prepare-apt-release.py --tag v0.6.5 --output dist/apt-packages
python3 scripts/build-apt-repository.py --packages dist/apt-packages/*.deb \
  --output dist/apt-public --base-url https://allp-manager.github.io/apt \
  --key FULL_FINGERPRINT --gnupghome /PRIVATE/PATH/gnupg
```

The integration test uses a throwaway key, isolated APT sources/state and a
simulated `install allp`. It checks both signatures, metadata tampering rejection,
retained by-hash indexes and immutable package versions. It does not install on
the host or change `/etc/apt`.

Rollback means deploying an earlier reviewed public archive after freshly signing
its Release metadata with the current key. Do not replace an already published
version's bytes, disable signature verification or serve expired metadata.
