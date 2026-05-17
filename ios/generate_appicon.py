#!/usr/bin/env python3
"""Generate iOS AppIcon asset from a programmatic design.

Outputs the full AppIcon.appiconset/ directory at:
    ios/App/Assets.xcassets/AppIcon.appiconset/

Re-run after design changes. Maintainer-only — published binaries embed
the rendered PNGs, so consumers never run this.

Design (item 88 placeholder per ROADMAP — "in-house via Sketch / Figma";
this is a quick programmatic stand-in for v1; the user can swap in a
custom design before App Store submission):

    - Solid deep-navy background (#0F2D5C). No transparency (Apple rejects).
    - Large white "笔" centered (writing brush — direct symbolic match for
      a Chinese IME).
    - Bottom-right small dual-tone dot: half blue (W), half orange (P) —
      mirrors the candidate-bar source-indicator design from item 57.

Run:
    python3 ios/generate_appicon.py
"""

import json
import os
import sys
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    sys.exit("PIL not available. Install via: pip3 install Pillow")

THIS = Path(__file__).resolve()
APP_DIR = THIS.parent / "App"
ICONSET_DIR = APP_DIR / "Assets.xcassets" / "AppIcon.appiconset"
CATALOG_DIR = APP_DIR / "Assets.xcassets"

# All standard iOS icon sizes (idiom × scale × size in pt).
# Apple's AppIcon.appiconset format requires per-(idiom, scale, size) entry.
SIZES = [
    # (filename, pixel_size, idiom, size_pt, scale)
    ("Icon-20@2x.png",   40,  "iphone",  "20x20",   "2x"),
    ("Icon-20@3x.png",   60,  "iphone",  "20x20",   "3x"),
    ("Icon-29@2x.png",   58,  "iphone",  "29x29",   "2x"),
    ("Icon-29@3x.png",   87,  "iphone",  "29x29",   "3x"),
    ("Icon-40@2x.png",   80,  "iphone",  "40x40",   "2x"),
    ("Icon-40@3x.png",   120, "iphone",  "40x40",   "3x"),
    ("Icon-60@2x.png",   120, "iphone",  "60x60",   "2x"),
    ("Icon-60@3x.png",   180, "iphone",  "60x60",   "3x"),
    ("Icon-20@1x-ipad.png", 20,  "ipad",    "20x20", "1x"),
    ("Icon-20@2x-ipad.png", 40,  "ipad",    "20x20", "2x"),
    ("Icon-29@1x-ipad.png", 29,  "ipad",    "29x29", "1x"),
    ("Icon-29@2x-ipad.png", 58,  "ipad",    "29x29", "2x"),
    ("Icon-40@1x-ipad.png", 40,  "ipad",    "40x40", "1x"),
    ("Icon-40@2x-ipad.png", 80,  "ipad",    "40x40", "2x"),
    ("Icon-76@2x.png",      152, "ipad",    "76x76", "2x"),
    ("Icon-83.5@2x.png",    167, "ipad",    "83.5x83.5", "2x"),
    ("Icon-1024.png",       1024, "ios-marketing", "1024x1024", "1x"),
]

NAVY = (15, 45, 92)              # #0F2D5C
WHITE = (255, 255, 255)
WUBI_BLUE = (0, 122, 255)        # #007AFF (matches CandidateBar dot)
PINYIN_ORANGE = (255, 149, 0)    # #FF9500


def find_cjk_font(target_size):
    """macOS bundled fonts with CJK coverage (笔 in particular)."""
    candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/Library/Fonts/Songti.ttc",
    ]
    for path in candidates:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, size=target_size)
            except OSError:
                continue
    print("WARNING: no CJK font found, using PIL default — 笔 will render as `?`")
    return ImageFont.load_default()


def render_master(size: int) -> Image.Image:
    """Render the master AppIcon at the given pixel size. All smaller
    sizes downscale from this to keep the design crisp."""
    img = Image.new("RGB", (size, size), NAVY)
    draw = ImageDraw.Draw(img)

    # Center 笔 character. Font size ~70% of icon size.
    font = find_cjk_font(int(size * 0.72))
    text = "笔"
    bbox = draw.textbbox((0, 0), text, font=font)
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]
    # textbbox accounts for ascent/descent — center on the visual center
    # (subtract bbox top to align baseline).
    x = (size - text_w) // 2 - bbox[0]
    # Slight upward shift so the W/P dots in the bottom-right don't visually
    # overlap with the glyph's lowest stroke.
    y = (size - text_h) // 2 - bbox[1] - int(size * 0.04)
    draw.text((x, y), text, fill=WHITE, font=font)

    # Bottom-right small W/P indicator: a circle bisected horizontally.
    # Blue top half, orange bottom half. Marker for the dual-engine identity.
    dot_size = max(int(size * 0.18), 8)
    dot_margin = int(size * 0.07)
    dot_x = size - dot_margin - dot_size
    dot_y = size - dot_margin - dot_size

    # Top half: blue
    draw.pieslice(
        [dot_x, dot_y, dot_x + dot_size, dot_y + dot_size],
        start=180, end=360, fill=WUBI_BLUE,
    )
    # Bottom half: orange
    draw.pieslice(
        [dot_x, dot_y, dot_x + dot_size, dot_y + dot_size],
        start=0, end=180, fill=PINYIN_ORANGE,
    )
    return img


def write_contents_json(iconset_dir: Path):
    images = []
    for filename, _, idiom, size_pt, scale in SIZES:
        images.append({
            "filename": filename,
            "idiom": idiom,
            "scale": scale,
            "size": size_pt,
        })
    contents = {
        "images": images,
        "info": {"author": "lab8 generate_appicon.py", "version": 1},
    }
    (iconset_dir / "Contents.json").write_text(
        json.dumps(contents, indent=2) + "\n"
    )


def write_catalog_contents(catalog_dir: Path):
    contents = {
        "info": {"author": "lab8 generate_appicon.py", "version": 1},
    }
    (catalog_dir / "Contents.json").write_text(
        json.dumps(contents, indent=2) + "\n"
    )


def main():
    ICONSET_DIR.mkdir(parents=True, exist_ok=True)

    # Render the master once at 1024×1024, then downscale (sharper than
    # rendering at each size individually).
    master = render_master(1024)

    for filename, px, _idiom, _size_pt, _scale in SIZES:
        if px == 1024:
            img = master
        else:
            img = master.resize((px, px), Image.LANCZOS)
        img.save(ICONSET_DIR / filename, "PNG", optimize=True)
        print(f"  wrote {filename}  ({px}×{px})")

    write_contents_json(ICONSET_DIR)
    write_catalog_contents(CATALOG_DIR)
    print(f"\n✓ AppIcon assets at {ICONSET_DIR}")
    print("Next: run ios/build_sim.sh — xcodegen will pick up Assets.xcassets automatically.")


if __name__ == "__main__":
    main()
