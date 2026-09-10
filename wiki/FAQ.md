# Frequently Asked Questions

[← Wiki home](Home.md) · [فارسی](FAQ.fa.md)

**Is Allp another package manager?** No. It orchestrates installed native tools
and owns neither dependency resolution nor a universal package database.

**Does it hide native commands?** No. Mutating argv, source, scope and privilege
are shown before confirmation.

**Should I run `sudo allp`?** Normally no. Run as a user and let the central
runner elevate only root-required children.

**Are `update` and `upgrade` the same?** No. Semantics are backend-defined;
typically update refreshes metadata and upgrade changes installed software.

**Does `--yes` approve everything?** No. It skips only final Allp confirmation.
Ambiguous sources and bootstrap remain explicit.

**Can profiles reproduce exact versions?** Not yet. Version 1 records observed
versions but does not pin them.

**Does Allp modify project lockfiles?** Host Cargo maintenance never does.
Python/Node project actions require explicit target/scope and remain cautious.

**Is it production hardened?** It is public alpha and not security-audited.
