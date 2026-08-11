"""
Systemic sweep of wubi 日本新字体 (Japanese Shinjitai) chars from the
encoder data source — the follow-up to the 2026-06-06 繁体 sweep.

Background
----------
The 2026-06-06 sweep (sweep_wubi_trad_v2.py) deleted TRAD chars whose
simplified peer is reachable in any wubi source, using opencc `t2s`
(Traditional → Simplified) as the canonical mapping. That left a gap:
**Japanese Shinjitai** forms (巌 亀 両 伝 児 図 団 …) are NOT classical
Traditional Chinese, so `t2s(c) == c` and they survived. They still
get a wubi code via the build.rs encoder running over auto_decomp.txt,
so e.g. `mid` prefix-completes to 巌 and outranks pinyin.

Fix at the encoder source: same rule as v2, but the normalisation
function is upgraded from `t2s` to `t2s ∘ jp2t` — first fold the
Japanese Shinjitai back to Traditional (jp2t), then Traditional to
Simplified (t2s). 巌 --jp2t--> 巖 --t2s--> 岩.

误伤 guard (the reason v2 stayed at t2s)
----------------------------------------
opencc `jp2t` over-reaches: it treats some chars that are legitimate
**Simplified Chinese** characters as Japanese abbreviations
(欠→缺, 予→豫, 芸→艺, 糸→丝, 醋→酢, 疏→疎, 沪→滤, 浜→滨, 缶→罐,
弁→辨). Deleting those would break Simplified input. Guard with a
GB2312 whitelist (codec-enumerated, zero-dependency, authoritative for
common Simplified chars): a candidate is deleted ONLY if it is NOT a
GB2312 hanzi. All 10 over-reach chars are in GB2312; all real
Shinjitai forms are not.

Sweep rule
----------
  for each char c (one row) in auto_decomp.txt:
    norm = t2s(jp2t(c))
    if norm != c                       (c is Shinjitai / variant)
       AND every char of norm ∈ {auto_decomp ∪ seed ∪ jianma_simplified}
                                       (simp peer reachable via wubi)
       AND c ∉ GB2312                  (c is not a Simplified regular char)
       AND NOT (t2s(c) != c ...)       (not already a v2-class TRAD; idempotent)
    THEN delete c's row from auto_decomp.txt
         AND drop c's overlay rows from wubi library.tsv
         AND log (code, word, date) to corpus_garbage_filter_v1.tsv

Run
---
  python3 docs/wubi-jp-shinjitai-sweep-2026-06-08/sweep_wubi_jp_shinjitai.py          # dry-run
  python3 docs/wubi-jp-shinjitai-sweep-2026-06-08/sweep_wubi_jp_shinjitai.py --apply  # write
"""
import subprocess
import sys
from pathlib import Path

APPLY = "--apply" in sys.argv
DATE = "2026-06-08"

ROOT = Path("core/crates/inputx-wubi/data")
AUTO = ROOT / "auto_decomp.txt"
SEED = ROOT / "seed.txt"
JIANMA = ROOT / "jianma_simplified.txt"
WUBI_LIB = ROOT / "library.tsv"
GARBAGE = Path("tools/scoring/data/polish/corpus_garbage_filter_v1.tsv")
AUDIT = Path("docs/wubi-jp-shinjitai-sweep-2026-06-08")

# opencc 1.3.x homebrew data dir
OCC = "/opt/homebrew/Cellar/opencc/1.3.1/share/opencc"


def occ(text, cfg):
    return subprocess.run(
        ["opencc", "-c", f"{OCC}/{cfg}"],
        input=text, capture_output=True, text=True
    ).stdout


def chars_in(path, col=0):
    s = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) <= col:
            continue
        for ch in parts[col]:
            s.add(ch)
    return s


# --- GB2312 Simplified-regular whitelist (codec-enumerated) ---
gb2312 = set()
for cp in range(0x4E00, 0xFA2A):
    try:
        chr(cp).encode("gb2312")
        gb2312.add(chr(cp))
    except UnicodeEncodeError:
        pass
print(f"GB2312 hanzi whitelist: {len(gb2312)}")

auto_lines = AUTO.read_text(encoding="utf-8").splitlines()
auto_chars = chars_in(AUTO)
all_wubi_chars = auto_chars | chars_in(SEED) | chars_in(JIANMA)
print(f"chars: auto={len(auto_chars)} union={len(all_wubi_chars)}")

# Per-char opencc mappings
chars = sorted(auto_chars)
t2s = dict(zip(chars, occ("\n".join(chars), "t2s.json").rstrip("\n").split("\n")))
jp2t_out = occ("\n".join(chars), "jp2t.json").rstrip("\n").split("\n")
norm_out = occ("\n".join(jp2t_out), "t2s.json").rstrip("\n").split("\n")
norm = dict(zip(chars, norm_out))


def reachable(simp):
    return all(ch in all_wubi_chars for ch in simp)


# --- classify each auto_decomp row ---
deletes = []          # (char, simp_peer)
whitelisted = []      # (char, simp_peer) — GB2312-protected, kept
kept_rows = []
for line in auto_lines:
    if not line or line.startswith("#"):
        kept_rows.append(line)
        continue
    parts = line.split("\t")
    ch = parts[0]
    if len(ch) != 1:
        kept_rows.append(line)
        continue
    n = norm.get(ch, ch)
    s_old = t2s.get(ch, ch)
    v2_class = (s_old != ch and reachable(s_old))   # already-handled TRAD class
    is_target = (n != ch and reachable(n) and not v2_class)
    if not is_target:
        kept_rows.append(line)
        continue
    if ch in gb2312:
        whitelisted.append((ch, n))
        kept_rows.append(line)            # PROTECT: keep the row
    else:
        deletes.append((ch, n))           # delete: drop the row

del_set = {c for c, _ in deletes}
print(f"\ncandidates: delete={len(deletes)}  whitelist-protected={len(whitelisted)}")
print("巌 in delete set:", "巌" in del_set)

# --- wubi library.tsv overlay cleanup ---
lib_lines = WUBI_LIB.read_text(encoding="utf-8").splitlines()
lib_kept = []
overlay_dropped = []   # (code, word)
for line in lib_lines:
    if not line or line.startswith("#"):
        lib_kept.append(line)
        continue
    parts = line.split("\t")
    if len(parts) < 2:
        lib_kept.append(line)
        continue
    code, word = parts[0], parts[1]
    if word in del_set and len(word) == 1:
        overlay_dropped.append((code, word))
    else:
        lib_kept.append(line)
print(f"wubi library.tsv overlay rows dropped: {len(overlay_dropped)}")

# --- audit dumps ---
AUDIT.mkdir(parents=True, exist_ok=True)
(AUDIT / "deleted.tsv").write_text(
    "# shinjitai\tsimp_peer\n" + "\n".join(f"{c}\t{s}" for c, s in deletes) + "\n",
    encoding="utf-8")
(AUDIT / "whitelisted.tsv").write_text(
    "# gb2312_protected\topencc_mismap\n" + "\n".join(f"{c}\t{s}" for c, s in whitelisted) + "\n",
    encoding="utf-8")
(AUDIT / "library_overlay_dropped.tsv").write_text(
    "# code\tword\n" + "\n".join(f"{c}\t{w}" for c, w in overlay_dropped) + "\n",
    encoding="utf-8")
print(f"audit → {AUDIT}/deleted.tsv  whitelisted.tsv  library_overlay_dropped.tsv")

if not APPLY:
    print("\n[dry-run] no files written. re-run with --apply to commit changes.")
    sys.exit(0)

# --- APPLY ---
AUTO.write_text("\n".join(kept_rows) + "\n", encoding="utf-8")
print(f"\nauto_decomp.txt: {len(auto_lines)} → {len(kept_rows)} rows (-{len(auto_lines)-len(kept_rows)})")

WUBI_LIB.write_text("\n".join(lib_kept) + "\n", encoding="utf-8")
print(f"library.tsv: {len(lib_lines)} → {len(lib_kept)} rows (-{len(lib_lines)-len(lib_kept)})")

with GARBAGE.open("a", encoding="utf-8") as f:
    for code, word in overlay_dropped:
        f.write(f"{code}\t{word}\t{DATE}\n")
print(f"corpus_garbage_filter_v1.tsv: +{len(overlay_dropped)} rows")
print("\n[applied]")
