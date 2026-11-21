#!/usr/bin/env python3
"""Regenerate CjkFallback.ttf from a Maple Mono NF CN face (needs fonttools).

Usage: python fallback-subset.py [SOURCE.ttf]
Default source is the installed V8 regular, or the release font if vendored.
The subset is the common Han set (GB2312 level 1 + Big5 common), kana and CJK
punctuation, plus the small scripts a Latin-only face is likely to miss.
"""

import pathlib
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent
OUTPUT = ROOT / "CjkFallback.ttf"
SOURCE = (
    pathlib.Path(sys.argv[1])
    if len(sys.argv) > 1
    else ROOT / "MapleMonoNF-CN.ttf"
)
if not SOURCE.exists():
    SOURCE = (
        pathlib.Path.home()
        / "Library/Fonts/MapleMono-NF-CN-Regular.ttf"
    )


def decoded(lead_range, trail_range, codec):
    result = set()
    for lead in lead_range:
        for trail in trail_range:
            try:
                result.add(ord(bytes([lead, trail]).decode(codec)))
            except UnicodeDecodeError:
                pass
    return result


def codepoints():
    points = decoded(range(0xB0, 0xD8), range(0xA1, 0xFF), "gb2312")
    points |= decoded(range(0xA4, 0xC7), range(0x40, 0x7F), "big5")
    for start, end in [
        (0x3000, 0x30FF),  # CJK punctuation and kana
        (0x0370, 0x04FF),  # Greek and Cyrillic
        (0x0100, 0x024F),  # Latin extended
    ]:
        points.update(range(start, end + 1))
    return sorted(points)


def main() -> None:
    font = TTFont(SOURCE)
    options = subset.Options()
    options.layout_features = []
    options.hinting = False
    options.notdef_glyph = False
    options.notdef_outline = False
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=codepoints())
    subsetter.subset(font)

    names = font["name"]
    family = "rune-term CJK Fallback"
    for name_id in (1, 3, 4, 6, 16):
        names.removeNames(nameID=name_id)
    for platform, encoding, language in ((3, 10, 0x409), (1, 0, 0)):
        for name_id in (1, 3, 4, 6, 16):
            names.setName(
                family if name_id != 6 else "rune-term-CJK-Fallback",
                name_id, platform, encoding, language,
            )

    font.save(OUTPUT)
    print(f"{OUTPUT}: {OUTPUT.stat().st_size} bytes from {SOURCE}")


if __name__ == "__main__":
    main()
