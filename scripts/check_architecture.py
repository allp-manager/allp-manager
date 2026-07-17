#!/usr/bin/env python3
"""Fail when generic operations contain registered backend identifiers."""

from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[1]
backend_root = root / "src" / "backends"
operation_root = root / "src" / "operations"

backend_ids: set[str] = set()
patterns = [
    re.compile(r"fn\s+id\s*\([^)]*\)\s*->\s*&'static\s+str\s*\{\s*\"([^\"]+)\"", re.S),
    re.compile(r"\bid\s*:\s*\"([a-z0-9][a-z0-9_-]*)\""),
]

for path in backend_root.rglob("*.rs"):
    text = path.read_text(encoding="utf-8")
    for pattern in patterns:
        backend_ids.update(pattern.findall(text))

violations: list[str] = []
for path in operation_root.rglob("*.rs"):
    text = path.read_text(encoding="utf-8")
    string_literals = set(re.findall(r'"([^"]*)"', text))
    for backend_id in sorted(backend_ids):
        if backend_id in string_literals:
            violations.append(f"{path.relative_to(root)} contains backend id '{backend_id}'")

if violations:
    print("Architecture boundary violation:", file=sys.stderr)
    for violation in violations:
        print(f"- {violation}", file=sys.stderr)
    print("Move native commands and backend-specific policy into the adapter.", file=sys.stderr)
    raise SystemExit(1)

print(f"Architecture boundary check passed for {len(backend_ids)} backend id(s).")
