#!/usr/bin/env python3
"""
build_mobileconfig.py — emit a .mobileconfig profile that installs a font
system-wide on iOS via the Settings → Profile flow.

This is the entitlement-free path (CTFontManager.persistent now needs a
managed entitlement + Font Provider extension as of iOS 13+; configuration
profiles bypass that gate).

Usage:
    python3 tools/build_mobileconfig.py \
        --font ios/App/Resources/InputxCJKExtended.ttf \
        --output ios/App/Resources/InputxCJKExtended.mobileconfig

Run once after font changes; commit the output. iOS will warn the user
the profile is unsigned — that's expected for ad-hoc distribution. Sign
with `Apple Configurator 2` if a sealed profile is desired before App
Store submission.
"""

import argparse
import base64
import sys
import uuid
from pathlib import Path
from xml.sax.saxutils import escape


def emit_mobileconfig(
    font_path: Path,
    payload_id_prefix: str,
    payload_org: str,
    family_name: str,
    description: str,
) -> str:
    font_b64 = base64.b64encode(font_path.read_bytes()).decode("ascii")
    # Wrap base64 to 64-char lines for readability.
    wrapped = "\n".join(
        font_b64[i : i + 64] for i in range(0, len(font_b64), 64)
    )

    inner_uuid = str(uuid.uuid5(uuid.NAMESPACE_DNS, f"{payload_id_prefix}.font"))
    outer_uuid = str(uuid.uuid5(uuid.NAMESPACE_DNS, payload_id_prefix))

    return f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>PayloadContent</key>
    <array>
        <dict>
            <key>Font</key>
            <data>
{wrapped}
            </data>
            <key>Name</key>
            <string>{escape(family_name)}</string>
            <key>PayloadDescription</key>
            <string>{escape(family_name)} font.</string>
            <key>PayloadDisplayName</key>
            <string>{escape(family_name)}</string>
            <key>PayloadIdentifier</key>
            <string>{payload_id_prefix}.font</string>
            <key>PayloadType</key>
            <string>com.apple.font</string>
            <key>PayloadUUID</key>
            <string>{inner_uuid}</string>
            <key>PayloadVersion</key>
            <integer>1</integer>
        </dict>
    </array>
    <key>PayloadDescription</key>
    <string>{escape(description)}</string>
    <key>PayloadDisplayName</key>
    <string>{escape(family_name)} (扩展汉字)</string>
    <key>PayloadIdentifier</key>
    <string>{payload_id_prefix}</string>
    <key>PayloadOrganization</key>
    <string>{escape(payload_org)}</string>
    <key>PayloadRemovalDisallowed</key>
    <false/>
    <key>PayloadType</key>
    <string>Configuration</string>
    <key>PayloadUUID</key>
    <string>{outer_uuid}</string>
    <key>PayloadVersion</key>
    <integer>1</integer>
</dict>
</plist>
"""


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--font", type=Path, required=True)
    ap.add_argument("--output", type=Path, required=True)
    ap.add_argument(
        "--payload-id",
        default="jp.golia.inputx.fontprofile.cjkext",
    )
    ap.add_argument("--organization", default="GOLIA K.K.")
    ap.add_argument("--family-name", default="Inputx CJK Extended")
    ap.add_argument(
        "--description",
        default=(
            "Inputx 输入法 — 安装 CJK 扩展 B+ 区汉字字体到系统级，"
            "其他 app 也能正确显示罕见汉字。基于 Plangothic Project (SIL OFL 1.1)。"
        ),
    )
    args = ap.parse_args()

    if not args.font.exists():
        print(f"error: {args.font} not found", file=sys.stderr)
        return 1

    xml = emit_mobileconfig(
        font_path=args.font,
        payload_id_prefix=args.payload_id,
        payload_org=args.organization,
        family_name=args.family_name,
        description=args.description,
    )

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(xml)
    sz = args.output.stat().st_size
    print(f"wrote {args.output} ({sz/1024/1024:.1f} MB)", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
