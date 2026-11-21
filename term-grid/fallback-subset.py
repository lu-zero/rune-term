#!/usr/bin/env python3
"""Regenerate CjkFallback.ttf from the vendored Maple Mono NF CN face (needs fonttools)."""

import pathlib

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent
SOURCE = ROOT / "MapleMonoNF-CN.ttf"
OUTPUT = ROOT / "CjkFallback.ttf"

# CJK punctuation and kana, common ideographs, and the small script ranges a
# Latin-only face is most likely to miss.
RANGES = [(0x3000, 0x30FF), (0x4E00, 0x57FF), (0x0370, 0x04FF), (0x0100, 0x024F)]


def main() -> None:
    font = TTFont(SOURCE)
    options = subset.Options()
    options.layout_features = []
    options.hinting = False
    options.notdef_glyph = False
    options.notdef_outline = False
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(
        unicodes=[code for start, end in RANGES for code in range(start, end + 1)]
    )
    subsetter.subset(font)

    names = font["name"]
    family = "rune-term CJK Fallback"
    for name_id in (1, 3, 4, 6, 16):
        names.removeNames(nameID=name_id)
    for platform, encoding, language in ((3, 10, 0x409), (1, 0, 0)):
        for name_id in (1, 3, 4, 6, 16):
            names.setName(family if name_id != 6 else "rune-term-CJK-Fallback", name_id, platform, encoding, language)

    font.save(OUTPUT)
    print(f"{OUTPUT}: {OUTPUT.stat().st_size} bytes")


if __name__ == "__main__":
    main()
