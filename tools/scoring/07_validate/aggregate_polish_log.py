#!/usr/bin/env python3
"""Aggregate polish-log.jsonl into actionable scoring patches.

Reads `~/Library/Containers/jp.golia.inputmethod.wubi/Data/Library/Application
Support/Inputx/polish-log.jsonl` (the mac IME telemetry — every time the
user picked a candidate that wasn't #0, one line is appended) and surfaces
the repeat patterns that should drive the next data drop.

Output: one TSV report per category, plus a quickfix `.tsv` ready for
piping into the supplemental-phrases pipeline.

Usage:
    python3 aggregate_polish_log.py                    # default mac path
    python3 aggregate_polish_log.py --log path/to.jsonl
    python3 aggregate_polish_log.py --since 2026-05-20 # filter by date

Categories surfaced:
    1. **repeat-miss**: same (input, picked-word) ≥ 3 times → strong
       signal that picked-word should be #0 for that input. Outputs a
       freq-boost suggestion.
    2. **near-miss-bigger-rank**: picked-word was at rank ≥ 4 in the
       candidate list → ranking is far off, not just a 1↔2 flip.
    3. **buffer-no-match**: picked-word didn't appear in the candidates
       at all (user typed something, panel showed nothing useful,
       picked from somewhere else?) — currently can't happen unless
       PolishLog spec changes; reserved for future.

The pipeline's `06_llm_annotate` step optionally consumes the
`repeat-miss` output as a soft prior; the human reviewer pass folds
the suggestions into `data/supplemental/`.
"""

from __future__ import annotations

import argparse
import collections
import datetime as dt
import json
import sys
from pathlib import Path

DEFAULT_LOG = Path.home() / (
    "Library/Containers/jp.golia.inputmethod.wubi/Data/Library/"
    "Application Support/Inputx/polish-log.jsonl"
)

REPEAT_THRESHOLD = 3  # ≥ 3 picks of the same (input, word) = strong signal
NEAR_MISS_RANK = 4    # picked rank ≥ this = far-off ranking


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--log", type=Path, default=DEFAULT_LOG,
                   help=f"path to polish-log.jsonl (default: {DEFAULT_LOG})")
    p.add_argument("--since", type=str, default=None,
                   help="ISO date (YYYY-MM-DD); only consider entries on/after")
    p.add_argument("--out-dir", type=Path,
                   default=Path(__file__).resolve().parent.parent / "data" / "polish_reports",
                   help="where to write aggregate reports")
    return p.parse_args()


def load(log_path: Path, since: dt.datetime | None):
    if not log_path.exists():
        print(f"  no polish-log at {log_path} — nothing to aggregate")
        return []
    entries = []
    with log_path.open() as f:
        for ln, raw in enumerate(f, 1):
            raw = raw.strip()
            if not raw: continue
            try:
                e = json.loads(raw)
            except json.JSONDecodeError as err:
                print(f"  line {ln}: bad json, skipping ({err})")
                continue
            if since:
                try:
                    ts = dt.datetime.fromisoformat(e["ts"].rstrip("Z"))
                    if ts < since: continue
                except (KeyError, ValueError):
                    pass
            entries.append(e)
    return entries


def repeat_misses(entries):
    """Same (buffer, pickedWord) picked ≥ THRESHOLD times."""
    by_pair = collections.Counter()
    by_pair_ctx = collections.defaultdict(list)
    for e in entries:
        key = (e.get("buffer", ""), e.get("pickedWord", ""))
        by_pair[key] += 1
        by_pair_ctx[key].append(e)
    rows = []
    for (buf, word), n in by_pair.most_common():
        if n < REPEAT_THRESHOLD: continue
        # Compute average pickedIdx across hits — useful signal too.
        ctx = by_pair_ctx[(buf, word)]
        avg_idx = sum(c.get("pickedIdx", 0) for c in ctx) / max(1, len(ctx))
        rows.append({
            "buffer": buf,
            "preferred": word,
            "count": n,
            "avg_picked_idx": round(avg_idx, 1),
        })
    return rows


def near_miss_bigger_rank(entries):
    """Picks where pickedIdx ≥ NEAR_MISS_RANK — ranking far off."""
    rows = []
    for e in entries:
        idx = e.get("pickedIdx", 0)
        if idx >= NEAR_MISS_RANK:
            rows.append({
                "buffer": e.get("buffer", ""),
                "picked": e.get("pickedWord", ""),
                "picked_idx": idx,
                "top3": e.get("candidates", [])[:3],
            })
    return rows


def write_report(rows, out_path: Path, header_cols: list[str]):
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w") as f:
        f.write("\t".join(header_cols) + "\n")
        for r in rows:
            f.write("\t".join(str(r[k]) for k in header_cols) + "\n")


def load_base_freqs(weights_path: Path):
    """Build a (pinyin, word) → base_freq dict so the quickfix boost
    can target a freq that actually wins the candidate's pinyin slot.

    Returns ({} when weights.tsv missing, callers should still proceed
    — boost falls back to count-based heuristic).
    """
    base: dict[tuple[str, str], int] = {}
    if not weights_path.exists():
        return base
    with weights_path.open() as f:
        for raw in f:
            line = raw.rstrip("\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < 3:
                continue
            try:
                freq = int(parts[2])
            except ValueError:
                continue
            base[(parts[0], parts[1])] = freq
    return base


def top_peer_freq_at(base: dict, pinyin: str, exclude_word: str) -> int:
    """Highest base freq at `pinyin` excluding `exclude_word`. Returns
    0 if no peers (the picked word would lead by default at this
    pinyin and no boost is needed — but we still emit a small floor)."""
    top = 0
    for (p, w), f in base.items():
        if p == pinyin and w != exclude_word and f > top:
            top = f
    return top


def write_quickfix_tsv(repeat_rows, out_path: Path, weights_path: Path):
    """Emit a TSV that builds into pinyin.fst via build_fst.rs overlay
    pipeline (with MAX semantics).

    Boost auto-tunes against actual base freq at the same pinyin:
    `freq = max(top_peer_freq + MARGIN, base + repeat_bonus)`. This
    way the picked word ACTUALLY wins the slot — under MAX overlay
    semantics, just setting freq=40000 isn't enough when 确实 (base
    42104) sits next to 缺失 (base 25090) at `queshi`. We need to
    publish a freq that beats 确实, otherwise the user-corrected
    `queshi → 缺失` keeps losing.

    MARGIN = 5000 gives the picked word a clear lead without
    discarding the peers entirely (peers remain visible at rank 2+).
    """
    MARGIN = 5000
    REPEAT_BONUS_PER_PICK = 5000  # historical compat

    out_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        from pypinyin import pinyin, Style
        compute_pinyin = lambda w: "".join(
            p[0] for p in pinyin(w, style=Style.NORMAL, heteronym=False)
        )
    except ImportError:
        compute_pinyin = lambda w: "???"

    base = load_base_freqs(weights_path)
    if base:
        print(f"[aggregate] loaded {len(base)} (pinyin,word) base freqs for auto-tune",
              file=sys.stderr)
    else:
        print(f"[aggregate] WARNING: no weights.tsv — boost uses count-only heuristic",
              file=sys.stderr)

    def is_pinyin_shaped(buf: str) -> bool:
        # Filter out wubi-only buffers (no a/e/i/o/u → can't be pinyin).
        # Catches `cfyt`, `tjvs`, `ggtt`, etc. Wubi picks don't belong
        # in the pinyin overlay since they came from a different engine
        # and wouldn't even be queried by pinyin lookup.
        return any(c in "aeiouv" for c in buf)

    def is_simplified_cjk_only(word: str) -> bool:
        # Pure CJK + no punctuation. Catches Japanese-tinged commits
        # like `え？` (mixed kana + fullwidth punct) that leaked into
        # pinyin polish-log via the unified commit path.
        if not word:
            return False
        for c in word:
            cp = ord(c)
            # CJK Unified Ideographs basic block.
            if 0x4E00 <= cp <= 0x9FFF:
                continue
            return False
        return True

    skipped = 0
    with out_path.open("w") as f:
        f.write("# repeat-miss-driven quickfix (review before applying)\n")
        f.write("# format: pinyin\\tword\\tfreq\\t(original_count)\n")
        f.write("# boost auto-tuned to beat top peer at same pinyin under MAX overlay semantics\n")
        for r in repeat_rows:
            buf = r["buffer"]
            word = r["preferred"]
            n = r["count"]
            if not is_pinyin_shaped(buf):
                skipped += 1
                continue
            if not is_simplified_cjk_only(word):
                skipped += 1
                continue
            py = buf if buf.isascii() and buf.islower() else compute_pinyin(word)
            # Compute target freq to actually win the slot.
            this_base = base.get((py, word), 0)
            peer_top = top_peer_freq_at(base, py, word) if base else 0
            target_to_win = peer_top + MARGIN if peer_top > 0 else 0
            count_based = 25000 + REPEAT_BONUS_PER_PICK * n
            freq = max(target_to_win, count_based, this_base + REPEAT_BONUS_PER_PICK * n)
            f.write(f"{py}\t{word}\t{freq}\t# n={n} base={this_base} peer={peer_top}\n")
    if skipped:
        print(f"[aggregate] skipped {skipped} non-pinyin / non-CJK rows from quickfix",
              file=sys.stderr)


def main() -> int:
    a = parse_args()
    since = None
    if a.since:
        since = dt.datetime.fromisoformat(a.since)
    entries = load(a.log, since)
    print(f"[aggregate] loaded {len(entries)} entries from {a.log}")
    if not entries:
        return 0

    r_repeat = repeat_misses(entries)
    r_near = near_miss_bigger_rank(entries)

    a.out_dir.mkdir(parents=True, exist_ok=True)
    write_report(r_repeat, a.out_dir / "repeat_misses.tsv",
                 ["buffer", "preferred", "count", "avg_picked_idx"])
    write_report(r_near, a.out_dir / "near_miss_bigger_rank.tsv",
                 ["buffer", "picked", "picked_idx", "top3"])
    weights_path = (Path(__file__).resolve().parent.parent.parent.parent
                    / "core/crates/inputx-pinyin/data/weights/weights.tsv")
    write_quickfix_tsv(r_repeat, a.out_dir / "quickfix_boost.tsv", weights_path)

    print(f"[aggregate] wrote {len(r_repeat)} repeat-miss rows")
    print(f"[aggregate] wrote {len(r_near)} near-miss-bigger-rank rows")
    print(f"[aggregate] quickfix TSV at {a.out_dir / 'quickfix_boost.tsv'}")
    if r_repeat:
        print("\nTop repeat-miss patterns (review-then-merge):")
        for r in r_repeat[:10]:
            print(f"  {r['buffer']:>12} → {r['preferred']:<8} × {r['count']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
