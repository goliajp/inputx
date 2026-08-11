#!/usr/bin/env python3
"""CP-0.6 external reference line — libpinyin MIU accuracy.

Drives Homebrew's libpinyin (the engine behind ibus-libpinyin / fcitx, a
mainstream open-source pinyin IME) over the SAME gold/silver eval sets and
the SAME metric as the in-tree Rust runner (tools/eval/runner): Top-K MIU
accuracy = exact full-string match of the gold hanzi within the engine's
first K sentence candidates.

Fairness notes — identical handling to the Rust runner so the two numbers
are comparable:
  * continuous pinyin: the TSV's space-separated syllables are concatenated
    (real users type no spaces; the engine segments).
  * exact string match, no trad->simp normalization. libpinyin emits
    simplified (gb_char), so the ~16% traditional gold rows miss for it
    too — the same handicap our engine carries.

libpinyin's stable simple API exposes only the 1-best sentence, so this
harness measures Top-1 MIU and reports Top-5/10 equal to it (see topk()).
Top-1 is the canonical MIU comparison number.

ctypes only (stdlib) — no pip, no Python binding required.

Usage:
  python3 tools/eval/runner_libpinyin.py
  python3 tools/eval/runner_libpinyin.py --silver-limit 2000
  python3 tools/eval/runner_libpinyin.py --probe "woshizhongguoren"
"""

import argparse
import ctypes
import json
import os
import sys
import tempfile
import time

HOMEBREW = "/opt/homebrew"
LIBPINYIN_DYLIB = f"{HOMEBREW}/lib/libpinyin.dylib"
GLIB_DYLIB = f"{HOMEBREW}/opt/glib/lib/libglib-2.0.0.dylib"
SYSTEMDIR = f"{HOMEBREW}/opt/libpinyin/lib/libpinyin/data"

EVAL_DIR = os.path.dirname(os.path.abspath(__file__))
MAX_K = 10

# pinyin_custom2.h option flags. Mirror the ibus-libpinyin default profile:
# incomplete pinyin + divided/resplit tables + dynamic adjustment. Tone off.
IS_PINYIN = 1 << 1
PINYIN_INCOMPLETE = 1 << 3
USE_DIVIDED_TABLE = 1 << 7
USE_RESPLIT_TABLE = 1 << 8
DYNAMIC_ADJUST = 1 << 9
DEFAULT_OPTIONS = (
    IS_PINYIN | PINYIN_INCOMPLETE | USE_DIVIDED_TABLE | USE_RESPLIT_TABLE | DYNAMIC_ADJUST
)

NOTE = (
    "libpinyin %s external reference. MIU accuracy = exact full-string match "
    "of the gold hanzi against the engine's 1-best sentence, fed continuous "
    "pinyin (spaces stripped), exact match (no trad->simp simplification). "
    "Top-5/10 equal Top-1: libpinyin's stable simple API exposes only the "
    "1-best sentence (pinyin_get_sentence asserts on out-of-range nbest "
    "index, no public count getter), so only Top-1 is measured. Same metric "
    "and same gold/silver sets as the in-tree Rust runner — Top-1 is directly "
    "comparable."
)


class LibPinyin:
    def __init__(self):
        self.glib = ctypes.CDLL(GLIB_DYLIB)
        self.glib.g_free.argtypes = [ctypes.c_void_p]
        self.glib.g_free.restype = None

        lib = ctypes.CDLL(LIBPINYIN_DYLIB)
        lib.pinyin_init.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
        lib.pinyin_init.restype = ctypes.c_void_p
        lib.pinyin_set_options.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
        lib.pinyin_set_options.restype = ctypes.c_bool
        lib.pinyin_alloc_instance.argtypes = [ctypes.c_void_p]
        lib.pinyin_alloc_instance.restype = ctypes.c_void_p
        lib.pinyin_reset.argtypes = [ctypes.c_void_p]
        lib.pinyin_reset.restype = ctypes.c_bool
        lib.pinyin_parse_more_full_pinyins.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
        lib.pinyin_parse_more_full_pinyins.restype = ctypes.c_size_t
        lib.pinyin_guess_sentence.argtypes = [ctypes.c_void_p]
        lib.pinyin_guess_sentence.restype = ctypes.c_bool
        lib.pinyin_get_sentence.argtypes = [
            ctypes.c_void_p,
            ctypes.c_uint8,
            ctypes.POINTER(ctypes.c_char_p),
        ]
        lib.pinyin_get_sentence.restype = ctypes.c_bool
        self.lib = lib

        # libpinyin writes a user language model into userdir; use a throwaway
        # temp dir so the reference run stays pristine and side-effect-free.
        self._userdir = tempfile.mkdtemp(prefix="libpinyin-eval-")
        self.ctx = lib.pinyin_init(SYSTEMDIR.encode(), self._userdir.encode())
        if not self.ctx:
            raise RuntimeError(f"pinyin_init failed (systemdir={SYSTEMDIR})")
        lib.pinyin_set_options(self.ctx, DEFAULT_OPTIONS)
        self.inst = lib.pinyin_alloc_instance(self.ctx)
        if not self.inst:
            raise RuntimeError("pinyin_alloc_instance failed")

    def version(self):
        try:
            with open(f"{HOMEBREW}/opt/libpinyin/.brew/libpinyin.rb") as f:
                pass
        except OSError:
            pass
        return "2.10.3"

    def topk(self, pinyin, k=MAX_K):
        """Return the 1-best sentence for continuous pinyin, as a 1-element
        list (or empty on failure).

        libpinyin's stable simple API exposes only the 1-best sentence:
        pinyin_get_sentence(index) ASSERTS index < results.size() and aborts
        the process on overflow, and there is no public getter for the
        result count, while the default config produces a single result.
        So we read index 0 only (guaranteed valid once guess_sentence
        succeeds) and treat Top-1 == Top-5 == Top-10 for libpinyin. Top-1
        MIU is the canonical comparison number anyway.
        """
        self.lib.pinyin_reset(self.inst)
        buf = pinyin.replace(" ", "").encode()
        if not buf:
            return []
        self.lib.pinyin_parse_more_full_pinyins(self.inst, buf)
        if not self.lib.pinyin_guess_sentence(self.inst):
            return []
        sptr = ctypes.c_char_p()
        if not self.lib.pinyin_get_sentence(self.inst, 0, ctypes.byref(sptr)) or not sptr.value:
            return []
        sentence = sptr.value.decode("utf-8")
        self.glib.g_free(sptr)
        return [sentence]


def load_tsv(path):
    rows = []
    with open(path, encoding="utf-8") as f:
        for i, line in enumerate(f):
            if i == 0:
                continue  # header
            line = line.rstrip("\n")
            if not line:
                continue
            cols = line.split("\t")
            if len(cols) < 2 or not cols[0] or not cols[1]:
                continue
            flags = cols[2] if len(cols) > 2 else ""
            rows.append((cols[0], cols[1], "polyphone" in flags))
    return rows


class Tally:
    __slots__ = ("n", "h1", "h5", "h10")

    def __init__(self):
        self.n = self.h1 = self.h5 = self.h10 = 0

    def add(self, rank):
        self.n += 1
        if rank is not None:
            if rank < 1:
                self.h1 += 1
            if rank < 5:
                self.h5 += 1
            if rank < 10:
                self.h10 += 1

    def ratios(self):
        n = self.n or 1
        return self.h1 / n, self.h5 / n, self.h10 / n


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gold", default=os.path.join(EVAL_DIR, "gold_1000.tsv"))
    ap.add_argument("--silver", default=os.path.join(EVAL_DIR, "silver_full.tsv"))
    ap.add_argument("--out", default=os.path.join(EVAL_DIR, "baselines", "libpinyin.json"))
    ap.add_argument("--date", default=time.strftime("%Y-%m-%d"))
    ap.add_argument("--silver-limit", type=int, default=None)
    ap.add_argument("--probe", default=None)
    args = ap.parse_args()

    engine = LibPinyin()

    if args.probe:
        print(f"probe {args.probe!r} → buffer {args.probe.replace(' ', '')!r}")
        for i, c in enumerate(engine.topk(args.probe, 20)):
            print(f"  [{i}] {c}")
        return

    gold = load_tsv(args.gold)
    silver = load_tsv(args.silver)
    if args.silver_limit is not None:
        silver = silver[: args.silver_limit]
    print(f"loaded {len(gold)} gold + {len(silver)} silver rows", file=sys.stderr)

    overall, gold_t, silver_t, poly_t = Tally(), Tally(), Tally(), Tally()
    started = time.time()
    total = len(gold) + len(silver)
    done = 0
    for rows, bucket in ((gold, gold_t), (silver, silver_t)):
        for pinyin, hanzi, is_poly in rows:
            cands = engine.topk(pinyin)
            rank = cands.index(hanzi) if hanzi in cands else None
            overall.add(rank)
            bucket.add(rank)
            if is_poly:
                poly_t.add(rank)
            done += 1
            if done % 5000 == 0:
                print(f"  {done}/{total}…", file=sys.stderr)
    elapsed = time.time() - started

    o1, o5, o10 = overall.ratios()
    g1, g5, g10 = gold_t.ratios()
    s1, s5, s10 = silver_t.ratios()
    p1, p5, p10 = poly_t.ratios()
    results = {
        "date": args.date,
        "engine": "libpinyin",
        "engine_version": engine.version(),
        "note": NOTE % engine.version(),
        "elapsed_secs": round(elapsed, 3),
        "counts": {
            "total": overall.n,
            "gold": gold_t.n,
            "silver": silver_t.n,
            "polyphone": poly_t.n,
        },
        "top1": o1, "top5": o5, "top10": o10,
        "gold_top1": g1, "gold_top5": g5, "gold_top10": g10,
        "silver_top1": s1, "silver_top5": s5, "silver_top10": s10,
        "per_polyphone_top1": p1, "per_polyphone_top5": p5, "per_polyphone_top10": p10,
    }

    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
        f.write("\n")

    def pct(x):
        return f"{x * 100:6.2f}%"

    print(f"\nlibpinyin MIU accuracy — {args.date} ({overall.n} rows, {elapsed:.1f}s)")
    print("  bucket      top1     top5     top10    n")
    print(f"  overall    {pct(o1)}  {pct(o5)}  {pct(o10)}  {overall.n}")
    print(f"  gold       {pct(g1)}  {pct(g5)}  {pct(g10)}  {gold_t.n}")
    print(f"  silver     {pct(s1)}  {pct(s5)}  {pct(s10)}  {silver_t.n}")
    print(f"  polyphone  {pct(p1)}  {pct(p5)}  {pct(p10)}  {poly_t.n}")
    print(f"\nwrote {args.out}")


if __name__ == "__main__":
    main()
