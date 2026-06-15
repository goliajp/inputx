#!/usr/bin/env python3
"""Phase-2 bigram-LM training corpus cleaning pipeline.

Implements the 8-step recipe from pinyin-quality-gap-2026-06-14.html §06.7
(see also climb-plan CP-2.2):

    ① extract              wikiextractor (wiki), zip read (thucnews),
                            HF datasets streaming (lccc, cc100_zh)
    ② boilerplate           regex strip ([edit] / URL token / wiki templates / …)
    ③ fasttext_lid          drop lines with P(zh) < 0.7 (lid.176.bin)
    ④ opencc                t2s.json — traditional → simplified
    ⑤ sentence_split        split on [。!?…]+ into one sentence per line
    ⑥ length_filter         keep len 4..120 AND han_ratio ≥ 0.6
    ⑦ minhash_dedup         MinHash-LSH sentence-level dedup (threshold 0.7)
    ⑧ tokenize              jieba.cut(HMM=False) + user_dict from
                            library.tsv top-100k phrases

Output: tools/scoring/09_bigram_lm/data/cleaned.txt
(one tokenized sentence per line, space-separated)

USAGE
=====

  # run all steps for all sources (large; expect hours)
  python3 clean.py --all

  # run a single step (resumable — re-running a finished step is fast)
  python3 clean.py --step extract
  python3 clean.py --step boilerplate
  python3 clean.py --step fasttext_lid
  python3 clean.py --step opencc
  python3 clean.py --step sentence_split
  python3 clean.py --step length_filter
  python3 clean.py --step minhash_dedup
  python3 clean.py --step tokenize

  # only one source through one step
  python3 clean.py --step extract --source wiki

  # status (which intermediate files exist, sizes, line counts)
  python3 clean.py --status

Intermediates live under tools/scoring/09_bigram_lm/data/<step>/<src>.txt
where step ∈ {01_extracted, 02_noboiler, 03_zhonly, 04_simplified,
05_split, 06_filtered, 07_deduped, 08_tokenized} and src ∈ {wiki,
thucnews, lccc, cc100_zh}. Each step reads from the previous step's dir,
keeps per-source files separate until the final merge into cleaned.txt
(MinHash dedup runs on the merged stream from step 05_split onwards
because cross-source duplicates must also be killed).

All intermediates are gitignored. Re-running any single step on its own
is safe and idempotent for that step's output file.
"""
from __future__ import annotations

import argparse
import bz2
import gzip
import io
import re
import sys
import zipfile
from pathlib import Path
from typing import Callable, Iterable, Iterator

# ---------------------------------------------------------------------------
# Paths / constants
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parents[3]
WORK = Path(__file__).resolve().parent / "data"
WORK.mkdir(parents=True, exist_ok=True)

RAW_WIKI = REPO_ROOT / "tools/eval/corpus/raw/wiki/zhwiki-latest-pages-articles.xml.bz2"
RAW_THUCNEWS = REPO_ROOT / "tools/eval/corpus/raw/pd/THUCNews.zip"
RAW_LCCC = REPO_ROOT / "tools/eval/corpus/raw/lccc"
RAW_CC100 = REPO_ROOT / "tools/eval/corpus/raw/cc100"

LIBRARY_TSV = REPO_ROOT / "core/crates/inputx-pinyin/data/library.tsv"

STEPS = [
    "01_extracted",
    "02_noboiler",
    "03_zhonly",
    "04_simplified",
    "05_split",
    "06_filtered",
    "07_deduped",   # 07: merged + deduped (single file: 07_deduped/all.txt)
    "08_tokenized", # 08: tokenized (single file: 08_tokenized/cleaned.txt)
]

SOURCES = ["wiki", "thucnews", "lccc", "cc100_zh"]

STEP_BY_NAME = {
    "extract":         "01_extracted",
    "boilerplate":     "02_noboiler",
    "fasttext_lid":    "03_zhonly",
    "opencc":          "04_simplified",
    "sentence_split":  "05_split",
    "length_filter":   "06_filtered",
    "minhash_dedup":   "07_deduped",
    "tokenize":        "08_tokenized",
}

ORDERED_STEP_NAMES = list(STEP_BY_NAME.keys())

# Boilerplate strip regexes (climb plan §06.7 step ②)
BOILERPLATE_RES = [
    (re.compile(r"\[edit\]"), ""),
    (re.compile(r"\[citation needed\]"), ""),
    (re.compile(r"\[\d+\]"), ""),                 # footnote markers [1] [12]
    (re.compile(r"https?://\S+"), ""),
    (re.compile(r"www\.\S+"), ""),
    (re.compile(r"<[^>]+>"), ""),                 # any leftover html
    (re.compile(r"\{\{[^}]+\}\}"), ""),           # wiki templates
    (re.compile(r"\[\[(?:[^|\]]+\|)?([^\]]+)\]\]"), r"\1"),  # wiki [[link|text]]
    (re.compile(r"&nbsp;"), " "),
    (re.compile(r"&amp;"), "&"),
    (re.compile(r"\s+"), " "),                    # collapse whitespace
]

SENTENCE_SPLIT_RE = re.compile(r"[。!?！？…]+")
HAN_RE = re.compile(r"[一-鿿]")


# ---------------------------------------------------------------------------
# Util helpers
# ---------------------------------------------------------------------------

def step_dir(step: str) -> Path:
    p = WORK / step
    p.mkdir(parents=True, exist_ok=True)
    return p


def src_path(step: str, src: str) -> Path:
    return step_dir(step) / f"{src}.txt"


def line_count(path: Path) -> int:
    if not path.exists():
        return 0
    n = 0
    with path.open("rb") as f:
        while True:
            chunk = f.read(1 << 20)
            if not chunk:
                break
            n += chunk.count(b"\n")
    return n


def iter_lines(path: Path) -> Iterator[str]:
    with path.open("r", encoding="utf-8", errors="replace") as f:
        for line in f:
            yield line.rstrip("\n")


def write_lines(path: Path, lines: Iterable[str]) -> int:
    path.parent.mkdir(parents=True, exist_ok=True)
    n = 0
    with path.open("w", encoding="utf-8") as f:
        for line in lines:
            f.write(line)
            f.write("\n")
            n += 1
    return n


def log(msg: str) -> None:
    print(f"[clean] {msg}", flush=True)


# ---------------------------------------------------------------------------
# Step 1 · extract
# ---------------------------------------------------------------------------

def extract_wiki(out_path: Path) -> int:
    """Extract zhwiki via HuggingFace ``wikimedia/wikipedia``.

    Originally planned to run ``wikiextractor`` over the 3.1 GB ``.xml.bz2``
    dump we fetched in Phase 0, but ``wikiextractor`` 3.0.6 ships a regex
    that ``re`` in Python 3.14+ rejects (``PatternError: global flags not
    at the start``). Rather than patch the third-party source, switch to
    ``wikimedia/wikipedia`` on HuggingFace which is the same upstream dump,
    pre-extracted, and lives on the same datasets pipeline as the lccc /
    cc100 extractors below — one less moving part. The local bz2 stays
    around as backup. Pass a ``limit`` to subsample.
    """
    if out_path.exists() and out_path.stat().st_size > 500_000_000:
        log(f"wiki already extracted ({out_path.stat().st_size:,} bytes) — skip")
        return line_count(out_path)
    try:
        from datasets import load_dataset
    except ImportError:
        log("datasets not installed — pip install datasets")
        return 0
    # ``wikimedia/wikipedia`` is the canonical pre-cleaned snapshot. Config
    # names look like ``20231101.zh``; we try a few recent ones.
    configs = ["20231101.zh", "20230901.zh", "20230701.zh"]
    ds = None
    last_err = None
    for cfg in configs:
        try:
            log(f"wiki: trying HF wikimedia/wikipedia / {cfg} (streaming)…")
            ds = load_dataset("wikimedia/wikipedia", cfg, split="train", streaming=True)
            log(f"wiki: ok, using {cfg}")
            break
        except Exception as e:
            last_err = e
            log(f"wiki:   config {cfg} failed: {type(e).__name__}: {e}")
    if ds is None:
        log(f"wiki: FAILED to load any wikimedia/wikipedia config — last error: {last_err}")
        return 0
    n = 0
    written_bytes = 0
    with out_path.open("w", encoding="utf-8") as out:
        for row in ds:
            text = (row.get("text") or "").strip()
            if not text:
                continue
            # one paragraph per line; HF wikipedia text has \n separators
            for para in text.split("\n"):
                para = para.strip()
                if para:
                    out.write(para)
                    out.write("\n")
                    n += 1
                    written_bytes += len(para) + 1
            if n % 500_000 == 0:
                log(f"wiki:   {n:,} paragraphs, {written_bytes // 1_000_000} MB")
    log(f"wiki: extracted {n:,} paragraphs, {written_bytes // 1_000_000} MB")
    return n


def extract_thucnews(out_path: Path) -> int:
    """Walk THUCNews.zip — one article body per line, headlines as separate lines."""
    if not RAW_THUCNEWS.exists():
        log(f"thucnews source missing at {RAW_THUCNEWS} — skip")
        return 0
    if out_path.exists() and out_path.stat().st_size > 100_000_000:
        log(f"thucnews already extracted ({out_path.stat().st_size:,} bytes) — skip")
        return line_count(out_path)
    log("thucnews: streaming 740k+ files from zip…")
    n = 0
    with zipfile.ZipFile(RAW_THUCNEWS, "r") as z, out_path.open("w", encoding="utf-8") as out:
        names = [name for name in z.namelist() if name.endswith(".txt")]
        log(f"thucnews: {len(names):,} txt files")
        for i, name in enumerate(names):
            if i % 50000 == 0:
                log(f"thucnews:   {i:,}/{len(names):,}")
            try:
                data = z.read(name).decode("utf-8", errors="replace")
            except Exception:
                continue
            for line in data.splitlines():
                line = line.strip()
                if line:
                    out.write(line)
                    out.write("\n")
                    n += 1
    log(f"thucnews: extracted {n:,} lines")
    return n


def extract_lccc(out_path: Path, limit: int | None = None) -> int:
    """Fetch LCCC base via HuggingFace datasets, emit one utterance per line."""
    if out_path.exists() and out_path.stat().st_size > 100_000_000:
        log(f"lccc already extracted ({out_path.stat().st_size:,} bytes) — skip")
        return line_count(out_path)
    try:
        from datasets import load_dataset
    except ImportError:
        log("datasets not installed — pip install datasets")
        return 0
    log("lccc: loading silver/lccc base via HF datasets (first fetch ≈ 1-2 GB)…")
    ds = load_dataset("silver/lccc", "base", split="train", streaming=True)
    n = 0
    with out_path.open("w", encoding="utf-8") as out:
        for row in ds:
            utterances = row.get("dialog") or row.get("conversation") or []
            for utt in utterances:
                if isinstance(utt, str):
                    text = utt.strip()
                elif isinstance(utt, dict):
                    text = (utt.get("text") or utt.get("utterance") or "").strip()
                else:
                    text = ""
                if text:
                    out.write(text)
                    out.write("\n")
                    n += 1
                    if n % 1_000_000 == 0:
                        log(f"lccc:   {n:,} utterances")
                    if limit is not None and n >= limit:
                        log(f"lccc: hit limit {limit}")
                        return n
    log(f"lccc: extracted {n:,} utterances")
    return n


def extract_cc100(out_path: Path, target_bytes: int = 10_000_000_000) -> int:
    """Stream HF cc100 zh, subsample first ~10 GB of text into one line per line."""
    if out_path.exists() and out_path.stat().st_size > target_bytes * 0.8:
        log(f"cc100 already at {out_path.stat().st_size:,} bytes (≥ 80% of {target_bytes:,}) — skip")
        return line_count(out_path)
    try:
        from datasets import load_dataset
    except ImportError:
        log("datasets not installed — pip install datasets")
        return 0
    log(f"cc100: streaming HF cc100 zh up to ~{target_bytes//1_000_000_000} GB…")
    ds = load_dataset("cc100", lang="zh", split="train", streaming=True, trust_remote_code=True)
    n = 0
    written = 0
    with out_path.open("w", encoding="utf-8") as out:
        for row in ds:
            text = (row.get("text") or "").strip()
            if not text:
                continue
            line = text.replace("\n", " ").strip()
            if not line:
                continue
            out.write(line)
            out.write("\n")
            written += len(line) + 1
            n += 1
            if n % 500_000 == 0:
                log(f"cc100:   {n:,} lines, {written//1_000_000} MB")
            if written >= target_bytes:
                log(f"cc100: target reached ({written//1_000_000} MB)")
                break
    log(f"cc100: extracted {n:,} lines, {written//1_000_000} MB")
    return n


EXTRACTORS = {
    "wiki": extract_wiki,
    "thucnews": extract_thucnews,
    "lccc": extract_lccc,
    "cc100_zh": extract_cc100,
}


# ---------------------------------------------------------------------------
# Step 2 · boilerplate strip
# ---------------------------------------------------------------------------

def strip_boilerplate(line: str) -> str:
    for pat, rep in BOILERPLATE_RES:
        line = pat.sub(rep, line)
    return line.strip()


def step_per_line(in_dir: str, out_dir: str, src: str, fn: Callable[[str], str | None]) -> int:
    in_p = src_path(in_dir, src)
    out_p = src_path(out_dir, src)
    if not in_p.exists():
        log(f"{src}: no input at {in_p} — skip")
        return 0
    if out_p.exists() and out_p.stat().st_size > 0:
        log(f"{src}: {out_dir} already exists, line_count={line_count(out_p):,} — skip")
        return line_count(out_p)
    n_in = n_out = 0
    with out_p.open("w", encoding="utf-8") as out:
        for line in iter_lines(in_p):
            n_in += 1
            new = fn(line)
            if new:
                out.write(new)
                out.write("\n")
                n_out += 1
            if n_in % 1_000_000 == 0:
                log(f"{src}:   {n_in:,} in, {n_out:,} out")
    log(f"{src}: {n_in:,} → {n_out:,} (drop {n_in - n_out:,})")
    return n_out


# ---------------------------------------------------------------------------
# Step 3 · fasttext language id
# ---------------------------------------------------------------------------

FASTTEXT_MODEL: object | None = None


def fasttext_model():
    global FASTTEXT_MODEL
    if FASTTEXT_MODEL is not None:
        return FASTTEXT_MODEL
    import fasttext
    model_path = WORK / "lid.176.bin"
    if not model_path.exists():
        import urllib.request
        log("fasttext: downloading lid.176.bin (~125 MB)…")
        url = "https://dl.fbaipublicfiles.com/fasttext/supervised-models/lid.176.bin"
        urllib.request.urlretrieve(url, model_path)
    log("fasttext: loading lid.176.bin…")
    FASTTEXT_MODEL = fasttext.load_model(str(model_path))
    return FASTTEXT_MODEL


def keep_if_zh(line: str, threshold: float = 0.7) -> str | None:
    if not line or len(line) < 4:
        return None
    if not HAN_RE.search(line):
        return None
    m = fasttext_model()
    labels, probs = m.predict(line.replace("\n", " "))
    if labels and labels[0] == "__label__zh" and probs[0] >= threshold:
        return line
    return None


# ---------------------------------------------------------------------------
# Step 4 · OpenCC traditional → simplified
# ---------------------------------------------------------------------------

OPENCC: object | None = None


def opencc_t2s(line: str) -> str | None:
    global OPENCC
    if OPENCC is None:
        from opencc import OpenCC
        OPENCC = OpenCC("t2s")
    out = OPENCC.convert(line)
    return out if out else None


# ---------------------------------------------------------------------------
# Step 5 · sentence split
# ---------------------------------------------------------------------------

def sentence_split_lines(in_path: Path, out_path: Path) -> tuple[int, int]:
    n_in = n_out = 0
    with out_path.open("w", encoding="utf-8") as out:
        for line in iter_lines(in_path):
            n_in += 1
            for sent in SENTENCE_SPLIT_RE.split(line):
                sent = sent.strip()
                if sent:
                    out.write(sent)
                    out.write("\n")
                    n_out += 1
            if n_in % 1_000_000 == 0:
                log(f"  {n_in:,} in, {n_out:,} out")
    return n_in, n_out


# ---------------------------------------------------------------------------
# Step 6 · length + han_ratio filter
# ---------------------------------------------------------------------------

def length_han_filter(line: str, lo: int = 4, hi: int = 120, han_ratio: float = 0.6) -> str | None:
    if not (lo <= len(line) <= hi):
        return None
    n_han = len(HAN_RE.findall(line))
    if n_han / max(1, len(line)) < han_ratio:
        return None
    return line


# ---------------------------------------------------------------------------
# Step 7 · MinHash dedup (across the merged 06_filtered stream)
# ---------------------------------------------------------------------------

def minhash_dedup_merged(merged_path: Path, out_path: Path, num_perm: int = 64, threshold: float = 0.8) -> tuple[int, int]:
    """One pass over the merged 06_filtered stream, drop near-duplicates.

    Threshold 0.8 (≈ 80% shingle-Jaccard) is conservative — kills paste-style
    duplicates without nuking similar-topic legitimate variation. Each
    sentence's signature is materialised; LSH index lives in RAM. For ~30M
    sentences with 64-perm MinHash this stays under a few GB."""
    from datasketch import MinHash, MinHashLSH

    lsh = MinHashLSH(threshold=threshold, num_perm=num_perm)
    n_in = n_out = 0
    log(f"minhash: indexing+writing as we stream (threshold {threshold}, num_perm {num_perm})")
    with out_path.open("w", encoding="utf-8") as out:
        for line in iter_lines(merged_path):
            n_in += 1
            # 3-char shingles (jieba-free, fast, works for CJK)
            shingles = {line[i:i + 3] for i in range(max(1, len(line) - 2))}
            if not shingles:
                continue
            m = MinHash(num_perm=num_perm)
            for sh in shingles:
                m.update(sh.encode("utf-8"))
            if lsh.query(m):
                continue  # near-duplicate
            lsh.insert(str(n_in), m)
            out.write(line)
            out.write("\n")
            n_out += 1
            if n_in % 200_000 == 0:
                log(f"minhash:   {n_in:,} in, {n_out:,} out  (drop {n_in - n_out:,})")
    return n_in, n_out


# ---------------------------------------------------------------------------
# Step 8 · jieba tokenize
# ---------------------------------------------------------------------------

JIEBA_INIT = False


def jieba_init():
    global JIEBA_INIT
    if JIEBA_INIT:
        return
    import jieba
    jieba.initialize()
    # User dict from library.tsv top-100k phrases
    if LIBRARY_TSV.exists():
        log("jieba: loading top-100k phrases from library.tsv as user_dict…")
        with LIBRARY_TSV.open("r", encoding="utf-8") as f:
            rows = []
            for i, line in enumerate(f):
                if i >= 100_000:
                    break
                parts = line.rstrip().split("\t")
                if not parts:
                    continue
                word = parts[0].strip()
                if len(word) >= 2:
                    rows.append(word)
        for w in rows:
            jieba.add_word(w)
        log(f"jieba: user_dict loaded {len(rows):,} phrases")
    JIEBA_INIT = True


def jieba_tokenize_line(line: str) -> str | None:
    import jieba
    toks = list(jieba.cut(line, HMM=False))
    toks = [t for t in toks if t.strip()]
    if len(toks) < 2:
        return None
    return " ".join(toks)


# ---------------------------------------------------------------------------
# Step orchestration
# ---------------------------------------------------------------------------

def run_extract(source: str | None = None):
    targets = [source] if source else SOURCES
    for src in targets:
        out = src_path("01_extracted", src)
        fn = EXTRACTORS[src]
        try:
            fn(out)
        except Exception as e:  # extract failures are logged but don't stop other sources
            log(f"{src}: extract FAILED: {e}")


def run_boilerplate(source: str | None = None):
    targets = [source] if source else SOURCES
    for src in targets:
        step_per_line("01_extracted", "02_noboiler", src, strip_boilerplate)


def run_fasttext_lid(source: str | None = None):
    targets = [source] if source else SOURCES
    for src in targets:
        step_per_line("02_noboiler", "03_zhonly", src, keep_if_zh)


def run_opencc(source: str | None = None):
    targets = [source] if source else SOURCES
    for src in targets:
        step_per_line("03_zhonly", "04_simplified", src, opencc_t2s)


def run_sentence_split(source: str | None = None):
    targets = [source] if source else SOURCES
    for src in targets:
        in_p = src_path("04_simplified", src)
        out_p = src_path("05_split", src)
        if not in_p.exists():
            log(f"{src}: no input at {in_p} — skip")
            continue
        if out_p.exists() and out_p.stat().st_size > 0:
            log(f"{src}: 05_split exists, line_count={line_count(out_p):,} — skip")
            continue
        n_in, n_out = sentence_split_lines(in_p, out_p)
        log(f"{src}: split {n_in:,} → {n_out:,}")


def run_length_filter(source: str | None = None):
    targets = [source] if source else SOURCES
    for src in targets:
        step_per_line("05_split", "06_filtered", src, length_han_filter)


def run_minhash_dedup():
    """Merges 06_filtered/{src}.txt → 07_deduped/all.txt with MinHash LSH."""
    merged = step_dir("06_filtered") / "_merged.txt"
    out_p = step_dir("07_deduped") / "all.txt"
    if out_p.exists() and out_p.stat().st_size > 0:
        log(f"07_deduped/all.txt exists, line_count={line_count(out_p):,} — skip")
        return
    if not merged.exists():
        log("06_filtered/_merged.txt: building merged stream…")
        with merged.open("w", encoding="utf-8") as out:
            for src in SOURCES:
                p = src_path("06_filtered", src)
                if not p.exists():
                    log(f"  {src}: missing, skip from merge")
                    continue
                n = 0
                for line in iter_lines(p):
                    out.write(line)
                    out.write("\n")
                    n += 1
                log(f"  {src}: {n:,} lines into merged")
    log("minhash dedup over merged stream…")
    n_in, n_out = minhash_dedup_merged(merged, out_p)
    log(f"minhash: {n_in:,} → {n_out:,}")


def run_tokenize():
    in_p = step_dir("07_deduped") / "all.txt"
    out_p = step_dir("08_tokenized") / "cleaned.txt"
    if out_p.exists() and out_p.stat().st_size > 0:
        log(f"08_tokenized/cleaned.txt exists, line_count={line_count(out_p):,} — skip")
        return
    if not in_p.exists():
        log(f"07_deduped/all.txt missing — run minhash_dedup first")
        return
    jieba_init()
    n_in = n_out = 0
    with out_p.open("w", encoding="utf-8") as out:
        for line in iter_lines(in_p):
            n_in += 1
            toks = jieba_tokenize_line(line)
            if toks:
                out.write(toks)
                out.write("\n")
                n_out += 1
            if n_in % 200_000 == 0:
                log(f"tokenize:   {n_in:,} in, {n_out:,} out")
    log(f"tokenize: {n_in:,} → {n_out:,}")
    # Mirror to cleaned.txt at lm root
    final = WORK / "cleaned.txt"
    if final.exists():
        final.unlink()
    final.symlink_to(out_p)
    log(f"final symlink: {final} → {out_p}")


STEP_RUNNERS = {
    "extract":         run_extract,
    "boilerplate":     run_boilerplate,
    "fasttext_lid":    run_fasttext_lid,
    "opencc":          run_opencc,
    "sentence_split":  run_sentence_split,
    "length_filter":   run_length_filter,
    "minhash_dedup":   run_minhash_dedup,
    "tokenize":        run_tokenize,
}


def status():
    print("Pipeline status:")
    print(f"  raw:")
    print(f"    wiki     {RAW_WIKI.stat().st_size if RAW_WIKI.exists() else 'MISSING':>15}  {RAW_WIKI}")
    print(f"    thucnews {RAW_THUCNEWS.stat().st_size if RAW_THUCNEWS.exists() else 'MISSING':>15}  {RAW_THUCNEWS}")
    print(f"  intermediates (lines / bytes per source):")
    for step in STEPS:
        d = step_dir(step)
        files = sorted(d.glob("*.txt"))
        if not files:
            print(f"  {step:18s}  (empty)")
            continue
        print(f"  {step:18s}")
        for p in files:
            lc = line_count(p)
            print(f"    {p.name:30s}  {lc:>14,d} lines  {p.stat().st_size:>15,d} bytes")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--step", choices=ORDERED_STEP_NAMES)
    ap.add_argument("--source", choices=SOURCES)
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--status", action="store_true")
    args = ap.parse_args()
    if args.status:
        status()
        return
    if args.all:
        for s in ORDERED_STEP_NAMES:
            log(f"=== step: {s} ===")
            STEP_RUNNERS[s]()
        return
    if args.step:
        log(f"=== step: {args.step} ===")
        runner = STEP_RUNNERS[args.step]
        if args.source and runner in (run_extract, run_boilerplate, run_fasttext_lid,
                                       run_opencc, run_sentence_split, run_length_filter):
            runner(args.source)
        else:
            runner()
        return
    ap.print_help()


if __name__ == "__main__":
    main()
