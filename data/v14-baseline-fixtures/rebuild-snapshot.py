#!/usr/bin/env python3
"""Regenerate the baseline-fixture snapshot against today's engine.

Replaces the v1.3-era build-v1.3-snapshot.sh (which hard-coded a stale
probe binary path /Volumes/INTEL2T/...). Usage:

    python3 data/v14-baseline-fixtures/rebuild-snapshot.py <output.json>

Same 33-buffer × 2-jp × 3-edge corpus as the original v1.3 snapshot
builder. Run at v-cycle boundaries to refresh the comparison baseline.
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PROBE_CRATE = ROOT / "core"

POLISH = ["jixu", "sheji", "lixiang", "jiazai", "juti", "tongyi", "aiyi",
          "shinjuku", "shinjuk", "pianni", "zho", "lianxiang", "mo",
          "nihaomawojiao", "kaopu", "yongbuliao", "taikexi", "houxuanqu",
          "famiriaare", "vaiorin", "fami", "famin", "fa-----", "ko-hi-",
          "jieni"]
WUBI = ["ggg", "j", "jjjj", "wwww", "q", "ahxg"]
EDGE = ["qwxzy", "hellox"]


def probe(buf, mode="", jp=False):
    args = ["cargo", "run", "--quiet", "--release", "--bin",
            "inputx-probe", "--", buf]
    if mode:
        args += ["--mode", mode]
    if jp:
        args += ["--jp"]
    r = subprocess.run(args, capture_output=True, text=True, cwd=PROBE_CRATE)
    if r.returncode != 0:
        raise RuntimeError(f"probe {buf!r} failed: {r.stderr}")
    return json.loads(r.stdout)


def entry(buf, mode, jp, category):
    mode_label = {"": "Mixed", "wubi": "Wubi", "pinyin": "Pinyin",
                  "japanese": "Japanese"}[mode]
    p = probe(buf, mode, jp)
    return {
        "buffer": buf,
        "mode": mode_label,
        "jp": jp,
        "category": category,
        "preedit": p["preedit"],
        "top10": [{"word": c["word"], "source": c["source"],
                    "score": c.get("score", 0)} for c in p["candidates"]],
    }


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    if out is None:
        print("usage: rebuild-snapshot.py <output.json>", file=sys.stderr)
        sys.exit(2)

    entries = []
    for buf in POLISH:
        for jp in [False, True]:
            entries.append(entry(buf, "", jp, "polish"))
    for buf in WUBI:
        for jp in [False, True]:
            entries.append(entry(buf, "", jp, "wubi"))
    for buf in EDGE:
        for jp in [False, True]:
            entries.append(entry(buf, "", jp, "edge"))
    # Empty-buffer edge
    entries.append(entry("", "", False, "edge-empty"))
    # Mode-switch cases
    entries.append(entry("nihao", "wubi", False, "mode-switch-wubi-overflow"))
    entries.append(entry("nihao", "", False, "mode-switch-pinyin-default"))

    out.write_text(json.dumps(entries, ensure_ascii=False, indent=2))
    print(f"wrote {len(entries)} entries → {out}")


if __name__ == "__main__":
    main()
