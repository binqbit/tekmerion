#!/usr/bin/env python3
"""Reject changes to the upstream forwarder used by authentication tests."""

from pathlib import Path
import hashlib
import json

root = Path(__file__).resolve().parents[1] / "tests/third-party/chainlink-solana"
manifest = json.loads((root / "UPSTREAM.json").read_text())
for entry in manifest["files"]:
    path = (root / entry["path"]).resolve()
    if not path.is_relative_to(root):
        raise SystemExit("Invalid upstream path")
    raw = path.read_bytes()
    sha = hashlib.sha256(raw).hexdigest()
    blob = hashlib.sha1(f"blob {len(raw)}\0".encode() + raw).hexdigest()
    if sha != entry["sha256"] or blob != entry["git_blob"]:
        raise SystemExit(f"Upstream source drift: {entry['path']}")
print(f"Verified {len(manifest['files'])} upstream files at {manifest['revision']}")
