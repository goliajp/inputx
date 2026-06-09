"""
Systemic sweep of TRAD / variant / Shinjitai chars from the wubi simcode
structural table jianma_simplified.txt — the table the 2026-06-06 繁体
sweep (auto_decomp.txt only) and the 2026-06-08 日本新字体 sweep
(auto_decomp.txt only) both missed. Surfaced by user report 2026-06-09
"kmu 員不对，应该是员，繁体不应该出现在五笔" (員 was a dup line here).

Normalisation (same as prior sweeps): norm(c) = t2s(c), falling back to
t2s(jp2t(c)) for Japanese Shinjitai forms t2s leaves untouched.

GB2312 whitelist guard (codec-enumerated): a row is a target ONLY if its
char is NOT a GB2312 hanzi — protects Simplified-regular chars opencc
over-maps (予→豫, 欠→缺, 沪→滤, 糸→丝, 後, 乾, 於 …).

Per target row (code, w) with simplified norm:
  - DUP type  — (code, norm) already exists as another row → delete this
    TRAD row (e.g. kmu 員 next to kmu 员).
  - UNIQUE type — (code, norm) not in table → rewrite w → norm, so the
    simcode stays usable, just in its Simplified form (買→买).

A row is a target only when norm is reachable via any wubi source
(auto_decomp ∪ seed ∪ jianma_simplified ∪ jianma1 ∪ library), so the
Simplified form is always still type-able.

Run:
  python3 docs/jianma-simplified-trad-sweep-2026-06-09/sweep_jianma_trad.py          # dry-run
  python3 docs/jianma-simplified-trad-sweep-2026-06-09/sweep_jianma_trad.py --apply
"""
import subprocess
import sys
from pathlib import Path

APPLY = "--apply" in sys.argv
ROOT = Path("core/crates/inputx-wubi/data")
JM = ROOT / "jianma_simplified.txt"
OCC = "/opt/homebrew/Cellar/opencc/1.3.1/share/opencc"
AUDIT = Path("docs/jianma-simplified-trad-sweep-2026-06-09")


def occ(text, cfg):
    return subprocess.run(["opencc", "-c", f"{OCC}/{cfg}"],
                          input=text, capture_output=True, text=True).stdout


def chars_in(path, col=0):
    s = set()
    for l in path.read_text(encoding="utf-8").splitlines():
        if l and not l.startswith("#"):
            pp = l.split("\t")
            if len(pp) > col:
                for ch in pp[col]:
                    s.add(ch)
    return s


# GB2312 Simplified-regular whitelist
gb2312 = set()
for cp in range(0x4E00, 0xFA2A):
    try:
        chr(cp).encode("gb2312")
        gb2312.add(chr(cp))
    except UnicodeEncodeError:
        pass

lines = JM.read_text(encoding="utf-8").splitlines()
# original (code, word) set for dup detection
codeset = set()
single_rows = []  # (idx, code, word)
for i, l in enumerate(lines):
    if l and not l.startswith("#"):
        p = l.split("\t")
        if len(p) >= 2 and len(p[1]) == 1:
            codeset.add((p[0], p[1]))
            single_rows.append((i, p[0], p[1]))

allw = (chars_in(ROOT / "auto_decomp.txt") | chars_in(ROOT / "seed.txt")
        | chars_in(ROOT / "jianma_simplified.txt") | chars_in(ROOT / "jianma1.txt"))
for l in (ROOT / "library.tsv").read_text(encoding="utf-8").splitlines():
    if l and not l.startswith("#"):
        pp = l.split("\t")
        if len(pp) >= 2:
            for ch in pp[1]:
                allw.add(ch)

words = [w for _, _, w in single_rows]
t2s = occ("\n".join(words), "t2s.json").rstrip("\n").split("\n")
jp = occ("\n".join(words), "jp2t.json").rstrip("\n").split("\n")
jpt2s = occ("\n".join(jp), "t2s.json").rstrip("\n").split("\n")

del_idx = set()         # line indices to drop
rewrite = {}            # idx -> (code, trad, simp)
dup, rew, protected = [], [], []
for (idx, code, w), s, js in zip(single_rows, t2s, jpt2s):
    norm = s if s != w else (js if js != w else w)
    if norm == w or norm not in allw:
        continue
    if w in gb2312:
        protected.append((code, w, norm))
        continue
    if (code, norm) in codeset:
        del_idx.add(idx)
        dup.append((code, w, norm))
    else:
        rewrite[idx] = (code, w, norm)
        rew.append((code, w, norm))

print(f"jianma_simplified single-char rows: {len(single_rows)}")
print(f"DUP delete: {len(dup)}   UNIQUE rewrite: {len(rew)}   GB2312 protected: {len(protected)}")

AUDIT.mkdir(parents=True, exist_ok=True)
(AUDIT / "deleted_dup.tsv").write_text(
    "# code\ttrad\tsimp_peer\n" + "\n".join(f"{c}\t{w}\t{n}" for c, w, n in dup) + "\n", encoding="utf-8")
(AUDIT / "rewritten.tsv").write_text(
    "# code\ttrad\t→simp\n" + "\n".join(f"{c}\t{w}\t{n}" for c, w, n in rew) + "\n", encoding="utf-8")
(AUDIT / "protected.tsv").write_text(
    "# code\tgb2312_char\topencc_norm\n" + "\n".join(f"{c}\t{w}\t{n}" for c, w, n in protected) + "\n", encoding="utf-8")
print(f"audit → {AUDIT}/deleted_dup.tsv  rewritten.tsv  protected.tsv")

if not APPLY:
    print("\n[dry-run] no files written. re-run with --apply.")
    sys.exit(0)

out = []
for i, l in enumerate(lines):
    if i in del_idx:
        continue
    if i in rewrite:
        code, w, n = rewrite[i]
        out.append(l.replace(f"{code}\t{w}", f"{code}\t{n}", 1))
    else:
        out.append(l)
JM.write_text("\n".join(out) + "\n", encoding="utf-8")
print(f"\njianma_simplified.txt: {len(lines)} → {len(out)} lines (-{len(del_idx)} dup), {len(rewrite)} rewritten")
print("[applied]")
