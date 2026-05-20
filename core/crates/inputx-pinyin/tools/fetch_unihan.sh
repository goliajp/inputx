#!/usr/bin/env bash
# Fetch the Unicode Unihan data bundle and extract the Readings file.
# Idempotent: skips download if Unihan_Readings.txt already exists.
#
# Source: Unicode Character Database, latest UCD release.
# License: Unicode License v3 (PD-equivalent terms; attribution preserved
#          in derived data/readings_unihan.tsv header).

set -euo pipefail

URL="https://www.unicode.org/Public/UCD/latest/ucd/Unihan.zip"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DATA_DIR="$SCRIPT_DIR/../data/unihan"
TARGET="$DATA_DIR/Unihan_Readings.txt"

mkdir -p "$DATA_DIR"

if [[ -f "$TARGET" ]]; then
    echo "Already present: $TARGET ($(du -h "$TARGET" | cut -f1))"
    echo "Delete the file to re-fetch."
    exit 0
fi

TMP="$(mktemp -d)"
trap "rm -rf '$TMP'" EXIT

echo "Downloading $URL …"
curl -sSL --max-time 120 "$URL" -o "$TMP/Unihan.zip"

SIZE=$(du -h "$TMP/Unihan.zip" | cut -f1)
SHA=$(shasum -a 256 "$TMP/Unihan.zip" | cut -d' ' -f1)
echo "Downloaded: $SIZE   sha256: $SHA"

echo "Extracting Unihan_Readings.txt …"
unzip -q -o "$TMP/Unihan.zip" Unihan_Readings.txt -d "$DATA_DIR"

if [[ ! -f "$TARGET" ]]; then
    echo "ERROR: extraction did not produce $TARGET"
    exit 1
fi

echo "OK: $TARGET ($(du -h "$TARGET" | cut -f1))"
