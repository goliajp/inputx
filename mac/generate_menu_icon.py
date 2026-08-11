#!/usr/bin/env python3
"""Generate the menu-bar IME indicator TIFF, matching Apple's
TamilIM/SCIM reference exactly: multipage TIFF, 16×16 @72dpi (@1x)
+ 32×32 @144dpi (@2x), LZW-compressed RGBA, black-on-transparent
alpha mask (template-tinted by macOS via TISIconIsTemplate=true).

Wired in mac/Info.plist as `tsInputMethodIconFileKey`,
`tsInputModeMenuIconFileKey`, `tsInputModePaletteIconFileKey`.

The Add-Input-Source picker tile is rendered by macOS from
`TISIconLabels.Primary` — it does NOT use this file. Confirmed
against Apple's SCIM where `pinyin.tiff` is also 16×16 monochrome
yet the picker shows the dark-rounded "拼音" tile.

Run:
    python3 mac/generate_menu_icon.py
    # then mac/hot-patch-assets.sh menu-icon

Maintainer-only — output is checked into mac/Resources/.
"""

import os
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    sys.exit("PIL not available. Install via: pip3 install Pillow")

RESOURCES = Path(__file__).resolve().parent / "Resources"
OUT_TIFF = RESOURCES / "inputx_menu_icon.tiff"
GLYPH = "五"

# Per-size targets (no naive 2× scale — Apple re-renders at each size
# so the 32px version isn't just blurred 16px).
#   canvas: the actual pixel grid macOS will draw into
#   inkbox: target glyph height; ~87% of canvas (matches Apple SCIM)
#   dpi:    resolution metadata that distinguishes @1x from @2x
SPECS = [
    {"canvas": 16, "inkbox": 12, "dpi": 72},
    {"canvas": 32, "inkbox": 24, "dpi": 144},
]


def find_heavy_cjk_font(target_px: int) -> ImageFont.FreeTypeFont:
    """PingFang SC Medium — system default CJK face on macOS. Its
    Medium weight is the heaviest PingFang variant macOS ships
    (there's no PingFang Bold/Heavy). Alpha hardening below
    compensates for the thinner strokes at 16×16."""
    candidates = [
        ("/System/Library/AssetsV2/com_apple_MobileAsset_Font8/86ba2c91f017a3749571a82f2c6d890ac7ffb2fb.asset/AssetData/PingFang.ttc", 7),
        ("/System/Library/Fonts/ヒラギノ角ゴシック W9.ttc", 0),
        ("/System/Library/Fonts/STHeiti Medium.ttc", 1),
    ]
    for path, idx in candidates:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, size=target_px, index=idx)
            except OSError:
                continue
    sys.exit("No usable CJK font found.")


def render_glyph(canvas_px: int, inkbox_px: int) -> Image.Image:
    """Render a solid white rounded chip with a centered black "五" glyph.

    Inverted-color chip design (matches Sogou's menubar icon convention):
    white solid rounded background + thin gray border + black ink. The
    chip is NOT a macOS template image — it stays white-with-black-ink
    in both system appearances, intentionally giving high contrast on
    a dark menubar (dark mode) and a subtle bordered chip on a light
    menubar (light mode). Driven by user requirement: "darkmode 白色底,
    lightmode 黑色底" — true dual-appearance would need two assets;
    this is the dark-mode-optimized single asset.

    Strategy: supersampled glyph render → tight bbox crop → rescale to
    target inkbox → paste centered onto rounded white chip canvas.
    """
    ss = 8
    work_size = canvas_px * ss
    # Oversize the supersample buffer so even tall/wide glyphs don't clip.
    buf = Image.new("RGBA", (work_size * 2, work_size * 2), (0, 0, 0, 0))
    draw = ImageDraw.Draw(buf)
    font_px = int(inkbox_px * ss * 1.25)
    font = find_heavy_cjk_font(font_px)
    draw.text((work_size, work_size), GLYPH, fill=(0, 0, 0, 255), font=font, anchor="mm")

    # Find actual ink bbox (post-render, ignores font metrics).
    bbox = buf.getbbox()
    if bbox is None:
        sys.exit("Glyph rendered to empty image — font issue?")
    glyph_img = buf.crop(bbox)

    # Rescale glyph so its taller dimension == inkbox_px (at canvas resolution).
    gw, gh = glyph_img.size
    if gh >= gw:
        new_h = inkbox_px
        new_w = max(1, round(gw * inkbox_px / gh))
    else:
        new_w = inkbox_px
        new_h = max(1, round(gh * inkbox_px / gw))
    glyph_img = glyph_img.resize((new_w, new_h), Image.LANCZOS)

    # Build the chip canvas: white solid rounded rect, anti-aliased via
    # supersample + LANCZOS downscale. Edges naturally fall off in alpha
    # (the gradient runs (0,0,0,0) → (0,0,0,128) → (255,255,255,255))
    # which matches Sogou's pixel pattern exactly. An explicit gray
    # outline with RGB=(160,160,160) opaque breaks the picker's chip
    # classifier — picker treats the tiff as ink-with-stroke rather
    # than chip-with-fill, producing the "outline-only" rendering.
    chip_ss = 8
    big = Image.new("RGBA", (canvas_px * chip_ss, canvas_px * chip_ss), (0, 0, 0, 0))
    ImageDraw.Draw(big).rounded_rectangle(
        [0, 0, canvas_px * chip_ss - 1, canvas_px * chip_ss - 1],
        radius=round(canvas_px * 0.20) * chip_ss,
        fill=(255, 255, 255, 255),
    )
    canvas = big.resize((canvas_px, canvas_px), Image.LANCZOS)
    # Paste glyph centered onto chip.
    px = (canvas_px - new_w) // 2
    py = (canvas_px - new_h) // 2
    canvas.paste(glyph_img, (px, py), glyph_img)
    return canvas


def main():
    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        # tiffutil -cathidpicheck infers @2x vs @1x from FILENAME
        # (the larger file must be named `<base>@2x.<ext>`), NOT from
        # the input TIFFs' DPI tags. Confirmed: feeding TIFFs with
        # PIL-set dpi=144 still produces 72dpi output unless the
        # filename is @2x.
        base = "inputx_menu_icon"
        page_files = []
        for spec in SPECS:
            img = render_glyph(spec["canvas"], spec["inkbox"])
            suffix = "@2x" if spec["dpi"] == 144 else ""
            out = td_path / f"{base}{suffix}.tiff"
            img.save(out, "TIFF", compression="tiff_lzw")
            page_files.append(str(out))
            print(f"  rendered {out.name}: {spec['canvas']}×{spec['canvas']} → expect {spec['dpi']}dpi")

        # tiffutil -cathidpicheck merges into a multipage TIFF AND
        # validates that each page is a unique hi-DPI variant. LZW
        # compression matches Apple's pipeline (smaller than ZIP for
        # this kind of mostly-transparent alpha content).
        OUT_TIFF.parent.mkdir(parents=True, exist_ok=True)
        # tiffutil -cathidpicheck infers DPI ordering from input filenames;
        # we pass smaller first so the IFD order matches Apple's convention
        # (@1x first, @2x second).
        subprocess.run(
            ["tiffutil", "-cathidpicheck", *page_files, "-out", str(OUT_TIFF)],
            check=True,
        )
        kb = OUT_TIFF.stat().st_size / 1024
        print(f"\n✓ {OUT_TIFF.name}  ({kb:.1f} KB)")
        print("\nVerification:")
        subprocess.run(
            ["tiffutil", "-info", str(OUT_TIFF)],
            stdout=subprocess.PIPE,
            text=True,
            check=True,
        )
        # Echo the headline numbers
        info = subprocess.run(
            ["tiffutil", "-info", str(OUT_TIFF)],
            capture_output=True, text=True, check=True,
        ).stdout
        for line in info.splitlines():
            if "Image Width" in line or "Resolution" in line or "Directory" in line:
                print(f"  {line.strip()}")
        print("\nNext: mac/hot-patch-assets.sh menu-icon")


if __name__ == "__main__":
    main()
