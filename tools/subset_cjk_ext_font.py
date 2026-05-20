#!/usr/bin/env python3
"""
subset_cjk_ext_font.py — subset Plangothic Super (P1) to just the CJK
Extension B+ codepoints present in wubi's auto_decomp.txt, rename per OFL
RFN clause, and emit a small TTF for the Inputx keyboard candidate bar.

Inputs:
    --plangothic-p1  path to Plangothic-Super .../static/PlangothicP1-Regular.ttf
    --auto-decomp    path to wubi's data/auto_decomp.txt (source of codepoints)
    --output         output TTF path

Output font name: "Inputx CJK Extended" — distinct from Plangothic's reserved
name (OFL 1.1 §1) so subset distribution complies.

Run after any wubi data version bump that touches auto_decomp.txt.
"""

import argparse
import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont

NEW_FAMILY = "Inputx CJK Extended"
NEW_PSNAME = "InputxCJKExtended-Regular"


def collect_codepoints(auto_decomp: Path) -> list[int]:
    cps = set()
    with auto_decomp.open() as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t", 1)
            if not parts or not parts[0]:
                continue
            ch = parts[0][0]
            if ord(ch) >= 0x20000:
                cps.add(ord(ch))
    return sorted(cps)


def rename_font(font: TTFont) -> None:
    """Rewrite the name table per OFL 1.1 RFN clause."""
    name = font["name"]
    # Wipe everything; rebuild with consistent fields. Preserves only what
    # iOS needs: family (id 1), subfamily (id 2), full name (id 4), postscript
    # name (id 6).
    name.names = []
    for nid, value in [
        (1, NEW_FAMILY),
        (2, "Regular"),
        (3, NEW_PSNAME),     # unique font ID
        (4, NEW_FAMILY),     # full name == family for single-style
        (6, NEW_PSNAME),     # postscript name
        (16, NEW_FAMILY),    # typographic family
        (17, "Regular"),
    ]:
        # Mac (1, 0, 0)
        name.setName(value, nid, 1, 0, 0)
        # Windows (3, 1, 0x0409)
        name.setName(value, nid, 3, 1, 0x0409)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--plangothic-p1", required=True, type=Path)
    ap.add_argument("--auto-decomp", required=True, type=Path)
    ap.add_argument("--output", required=True, type=Path)
    args = ap.parse_args()

    cps = collect_codepoints(args.auto_decomp)
    print(f"[subset] {len(cps)} extension codepoints from {args.auto_decomp}", file=sys.stderr)

    options = subset.Options()
    options.layout_features = []
    options.hinting = False
    options.recommended_glyphs = False
    options.legacy_kern = False
    options.notdef_outline = True
    options.name_IDs = ["*"]
    options.name_legacy = False
    options.name_languages = [0x0409]
    options.drop_tables += ["DSIG", "LTSH", "VDMX", "hdmx", "fpgm", "prep", "cvt", "gasp", "FFTM"]
    options.ignore_missing_unicodes = True
    options.glyph_names = False  # smaller post table

    font = subset.load_font(str(args.plangothic_p1), options)
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=cps)
    subsetter.subset(font)

    rename_font(font)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    subset.save_font(font, str(args.output), options)
    sz = args.output.stat().st_size
    print(f"[subset] wrote {args.output} ({sz/1024/1024:.1f} MB)", file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
