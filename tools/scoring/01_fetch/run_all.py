#!/usr/bin/env python3
"""Fetch every corpus listed in manifest.toml. Verifies sha256 on
download. Skips already-downloaded artifacts.

Run via `make 01-fetch` or `python 01_fetch/run_all.py`.
"""

from __future__ import annotations

import hashlib
import sys
import tomllib
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent  # tools/scoring/
DATA_RAW = ROOT / "data" / "raw"
MANIFEST = Path(__file__).parent / "manifest.toml"


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def fetch_one(name: str, entry: dict) -> None:
    url = entry["url"]
    version = entry["version"]
    expected_sha = entry.get("sha256", "TODO-SET-WHEN-FETCHED")
    suffix = Path(url).suffix
    if url.endswith(".tar.gz"):
        suffix = ".tar.gz"
    elif url.endswith(".xml.bz2"):
        suffix = ".xml.bz2"
    elif url.endswith(".xml.gz"):
        suffix = ".xml.gz"
    out = DATA_RAW / f"{name}-{version}{suffix}"
    out.parent.mkdir(parents=True, exist_ok=True)

    if out.exists() and expected_sha != "TODO-SET-WHEN-FETCHED":
        got = sha256_file(out)
        if got == expected_sha:
            print(f"  [{name}] ✓ cached {out.name}")
            return
        print(f"  [{name}] sha256 mismatch — re-fetching {out.name}")
        out.unlink()

    print(f"  [{name}] downloading {url}")
    urllib.request.urlretrieve(url, out)
    got = sha256_file(out)
    if expected_sha == "TODO-SET-WHEN-FETCHED":
        print(f"  [{name}] sha256={got} — write this into manifest.toml")
    elif got != expected_sha:
        print(f"  [{name}] FATAL sha256 mismatch — expected {expected_sha}, got {got}")
        sys.exit(1)
    print(f"  [{name}] ✓ {out.name}")


def main() -> int:
    with MANIFEST.open("rb") as f:
        manifest = tomllib.load(f)
    print(f"[01_fetch] using manifest at {MANIFEST}")
    for name, entry in manifest.items():
        if not isinstance(entry, dict) or "url" not in entry:
            continue
        try:
            fetch_one(name, entry)
        except Exception as e:
            print(f"  [{name}] FAILED: {e}")
            return 1
    print("[01_fetch] done")
    return 0


if __name__ == "__main__":
    sys.exit(main())
