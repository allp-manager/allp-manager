#!/usr/bin/env python3
"""Exercise the signed archive with isolated APT state; never install on the host."""
import importlib.util
import json
import os
from pathlib import Path
import pwd
import subprocess
import sys
import tempfile
from types import SimpleNamespace


def run(args, **kwargs):
    return subprocess.run(args, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                          check=True, **kwargs).stdout


def package(root, architecture, version="0.0.1~archive-test", content="test fixture\n"):
    directory = root / f"package-{architecture}-{version}"
    (directory / "DEBIAN").mkdir(parents=True)
    (directory / "usr/share/doc/allp").mkdir(parents=True)
    (directory / "usr/share/doc/allp/test").write_text(content)
    (directory / "DEBIAN/control").write_text(
        f"Package: allp\nVersion: {version}\nArchitecture: {architecture}\n"
        "Maintainer: Allp archive tests <test@example.invalid>\nDescription: Archive test fixture\n")
    destination = root / f"allp_{version}_{architecture}.deb"
    run(["dpkg-deb", "--build", "--root-owner-group", str(directory), str(destination)])
    return destination


def apt_environment(root, archive):
    for name in ("empty", "state/lists/partial", "cache/archives/partial", "log"):
        (root / name).mkdir(parents=True, exist_ok=True)
    source = root / "archive.sources"
    source.write_text(f"Types: deb\nURIs: file:{archive}\nSuites: stable\nComponents: main\n"
                      f"Architectures: amd64\nSigned-By: {archive}/allp-archive-keyring.gpg\n")
    config = root / "apt.conf"
    config.write_text(
        f'Dir::Etc::main "/dev/null";\nDir::Etc::parts "{root}/empty";\n'
        f'Dir::Etc::sourcelist "{source}";\nDir::Etc::sourceparts "{root}/empty";\n'
        f'Dir::Etc::trusted "{root}/empty.gpg";\nDir::Etc::trustedparts "{root}/empty";\n'
        f'Dir::State "{root}/state";\nDir::State::status "/var/lib/dpkg/status";\n'
        f'Dir::Cache "{root}/cache";\nDir::Log "{root}/log";\n'
        f'APT::Sandbox::User "{pwd.getpwuid(os.getuid()).pw_name}";\n'
        'APT::Architecture "amd64";\nAPT::Architectures { "amd64"; };\n'
        'Acquire::Languages "none";\nAPT::Get::List-Cleanup "false";\n')
    return {**os.environ, "APT_CONFIG": str(config), "LC_ALL": "C"}


def main():
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("archive_builder", Path(__file__).with_name("build-apt-repository.py"))
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    with tempfile.TemporaryDirectory(prefix="allp-apt-test-") as temp:
        root = Path(temp)
        key_home = root / "keys"
        key_home.mkdir(mode=0o700)
        try:
            run(["gpg", "--homedir", str(key_home), "--batch", "--pinentry-mode", "loopback",
                 "--passphrase", "", "--quick-gen-key", "Allp archive test <test@example.invalid>",
                 "ed25519", "sign", "1d"])
            listing = run(["gpg", "--homedir", str(key_home), "--with-colons", "--list-keys"])
            fingerprint = next(line.split(":")[9] for line in listing.splitlines() if line.startswith("fpr:"))
            debs = [package(root, arch) for arch in ("amd64", "arm64")]
            archive = root / "archive"
            args = SimpleNamespace(output=archive, base_url="https://example.invalid/allp", suite="stable",
                                   key=fingerprint, valid_days=90, packages=debs, previous=None, gnupghome=key_home)
            builder.build(args)
            run(["gpgv", "--keyring", str(archive / "allp-archive-keyring.gpg"),
                 str(archive / "dists/stable/Release.gpg"), str(archive / "dists/stable/Release")])
            assert set(record["architecture"] for record in json.loads((archive / "archive.json").read_text())["packages"].values()) == {"amd64", "arm64"}
            assert not list(archive.rglob("*private*"))
            env = apt_environment(root / "apt", archive)
            config = run(["apt-config", "dump"], env=env)
            assert "Post-Invoke" not in config, "host APT hooks must never be loaded"
            run(["apt-get", "update", "--error-on=any"], env=env)
            policy = run(["apt-cache", "policy", "allp"], env=env)
            assert "Candidate: 0.0.1~archive-test" in policy, policy
            simulation = run(["apt-get", "--simulate", "install", "allp"], env=env)
            assert "Inst allp " in simulation, simulation
            print("PASS: isolated APT verifies the signature and resolves install allp")

            # Retained by-hash indexes let clients finish a previous metadata read.
            next_archive = root / "next"
            args.output, args.previous, args.packages = next_archive, archive, []
            builder.build(args)
            for old in archive.glob("dists/**/by-hash/SHA256/*"):
                assert (next_archive / old.relative_to(archive)).read_bytes() == old.read_bytes()
            print("PASS: archive refresh preserves immutable packages and by-hash indexes")

            # Same package version with different bytes must never replace published bytes.
            replacement = root / "replacement"
            replacement.mkdir()
            args.output = root / "bad-replacement"
            args.packages = [package(replacement, "amd64", content="different bytes\n")]
            try:
                builder.build(args)
            except ValueError as error:
                assert "immutable" in str(error), error
            else:
                raise AssertionError("changed published package accepted")
            assert not args.output.exists()
            print("PASS: changed bytes under an existing version are rejected")

            # A fresh APT client must reject metadata changed after signing.
            in_release = archive / "dists/stable/InRelease"
            in_release.write_bytes(in_release.read_bytes().replace(b"Label: Allp", b"Label: Fake", 1))
            tampered_env = apt_environment(root / "tampered-apt", archive)
            attempt = subprocess.run(["apt-get", "update", "--error-on=any"], env=tampered_env,
                                     text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            assert attempt.returncode != 0 and any(message in attempt.stdout for message in (
                "BADSIG", "Bad signature", "Message has been manipulated")), attempt.stdout
            print("PASS: APT rejects tampered metadata")
        finally:
            subprocess.run(["gpgconf", "--homedir", str(key_home), "--kill", "gpg-agent"], check=False)


if __name__ == "__main__":
    main()
