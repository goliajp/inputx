#!/usr/bin/env python3
"""Generate the macOS app icon (.icns) from the same programmatic
design as the iOS version (`ios/generate_appicon.py`).

Design (shared with iOS for consistent branding):
    - Solid deep-navy background (#0F2D5C). No transparency.
    - Large white "笔" centered (writing brush — symbolic match for a
      Chinese IME).
    - Bottom-right small dual-tone dot: half blue (W) / half orange (P)
      — mirrors the candidate-bar source-indicator design.

Outputs:
    mac/Resources/inputx_app_icon.icns
    mac/Resources/inputx_app_icon.iconset/   (build-time artifact, gitignored)

Run:
    python3 mac/generate_icon.py

Re-run whenever the iOS design changes — the render logic is duplicated
from ios/generate_appicon.py rather than imported, so the two files
need to stay in sync by hand. Keeping them separate avoids
PYTHONPATH/setup.py overhead for what's still a placeholder design.

Maintainer-only — the shipped .icns is checked into mac/Resources/.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    sys.exit("PIL not available. Install via: pip3 install Pillow")

THIS = Path(__file__).resolve()
RESOURCES_DIR = THIS.parent / "Resources"
ICONSET_DIR = RESOURCES_DIR / "inputx_app_icon.iconset"
ICNS_PATH = RESOURCES_DIR / "inputx_app_icon.icns"

# macOS .icns required sizes (per Apple HIG + iconutil contract).
# Each entry: (filename, pixel size). All 10 must be present;
# iconutil packs them into a single multi-resolution .icns.
SIZES = [
    ("icon_16x16.png",       16),
    ("icon_16x16@2x.png",    32),
    ("icon_32x32.png",       32),
    ("icon_32x32@2x.png",    64),
    ("icon_128x128.png",     128),
    ("icon_128x128@2x.png",  256),
    ("icon_256x256.png",     256),
    ("icon_256x256@2x.png",  512),
    ("icon_512x512.png",     512),
    ("icon_512x512@2x.png",  1024),
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
    sizes downscale from this to keep the design crisp.

    Identical to ios/generate_appicon.py:render_master — keep in sync.
    """
    img = Image.new("RGB", (size, size), NAVY)
    draw = ImageDraw.Draw(img)

    # Center 笔 character. Font size ~72% of icon size.
    font = find_cjk_font(int(size * 0.72))
    text = "笔"
    bbox = draw.textbbox((0, 0), text, font=font)
    text_w = bbox[2] - bbox[0]
    text_h = bbox[3] - bbox[1]
    x = (size - text_w) // 2 - bbox[0]
    # Slight upward shift so the W/P dot in the bottom-right doesn't visually
    # overlap with the glyph's lowest stroke.
    y = (size - text_h) // 2 - bbox[1] - int(size * 0.04)
    draw.text((x, y), text, fill=WHITE, font=font)

    # Bottom-right small W/P indicator: a circle bisected horizontally.
    # Blue top half, orange bottom half. Marker for the dual-engine identity.
    dot_size = max(int(size * 0.18), 8)
    dot_margin = int(size * 0.07)
    dot_x = size - dot_margin - dot_size
    dot_y = size - dot_margin - dot_size

    draw.pieslice(
        [dot_x, dot_y, dot_x + dot_size, dot_y + dot_size],
        start=180, end=360, fill=WUBI_BLUE,
    )
    draw.pieslice(
        [dot_x, dot_y, dot_x + dot_size, dot_y + dot_size],
        start=0, end=180, fill=PINYIN_ORANGE,
    )
    return img


def main():
    if ICONSET_DIR.exists():
        # Clean prior iconset so stale files don't slip into the .icns.
        for f in ICONSET_DIR.iterdir():
            f.unlink()
    else:
        ICONSET_DIR.mkdir(parents=True, exist_ok=True)

    # Render the master once at 1024×1024, then downscale (sharper than
    # rendering at each size individually).
    print("Rendering master at 1024×1024...")
    master = render_master(1024)

    for filename, px in SIZES:
        if px == 1024:
            img = master
        else:
            img = master.resize((px, px), Image.LANCZOS)
        img.save(ICONSET_DIR / filename, "PNG", optimize=True)
        print(f"  wrote {filename}  ({px}×{px})")

    # Pack via iconutil → multi-resolution .icns. mac/build.sh's comment
    # specifically warns AGAINST `sips -s format icns` which produces a
    # 1-resolution legacy `il32` blob that CFReleases NULL on IconRef
    # resolution. iconutil is the only safe path.
    print(f"\nPacking with iconutil → {ICNS_PATH}")
    subprocess.run(
        ["iconutil", "-c", "icns", str(ICONSET_DIR), "-o", str(ICNS_PATH)],
        check=True,
    )
    size_kb = ICNS_PATH.stat().st_size / 1024
    print(f"\n✓ {ICNS_PATH.name}  ({size_kb:.1f} KB)")
    print("Next: ./mac/build.sh && ./mac/reinstall.sh")


if __name__ == "__main__":
    main()
