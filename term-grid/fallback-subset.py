#!/usr/bin/env python3
"""Regenerate CjkFallback.ttf from installed Maple Mono V8 faces (needs fonttools).

Usage: python fallback-subset.py [CN.ttf] [KR.ttf]
Defaults are the installed NF-CN and NF-KR regulars, or vendored fonts.
The subset is the common Han set (GB2312 level 1 + Big5 common), the full
modern Hangul syllable block, Jamo, kana and punctuation, plus the small
scripts a Latin-only face is likely to miss. Han and Hangul come from
different faces and are merged, so the fallback needs no separate Korean font.
"""

import pathlib
import sys
import tempfile

from fontTools import subset
from fontTools.merge import Merger
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent
OUTPUT = ROOT / "CjkFallback.ttf"
FONTS_DIR = pathlib.Path.home() / "Library/Fonts"
CN = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else FONTS_DIR / "MapleMono-NF-CN-Regular.ttf"
KR = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else FONTS_DIR / "MapleMono-NF-KR-Regular.ttf"
FAMILY = "rune-term CJK Fallback"


def decoded(lead_range, trail_range, codec):
    result = set()
    for lead in lead_range:
        for trail in trail_range:
            try:
                result.add(ord(bytes([lead, trail]).decode(codec)))
            except UnicodeDecodeError:
                pass
    return result


def han_points():
    points = decoded(range(0xB0, 0xD8), range(0xA1, 0xFF), "gb2312")
    points |= decoded(range(0xA4, 0xC7), range(0x40, 0x7F), "big5")
    for start, end in [
        (0x3000, 0x30FF),  # CJK punctuation and kana
        (0x0370, 0x04FF),  # Greek and Cyrillic
        (0x0100, 0x024F),  # Latin extended
    ]:
        points.update(range(start, end + 1))
    return sorted(points)


def hangul_points():
    # All modern precomposed syllables, Jamo and compatibility Jamo.
    return [*range(0xAC00, 0xD7A4), *range(0x1100, 0x1200), *range(0x3130, 0x3190)]


def reduced(source, points, dest):
    font = TTFont(source)
    options = subset.Options()
    options.layout_features = []
    options.hinting = False
    options.notdef_glyph = False
    options.notdef_outline = False
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=points)
    subsetter.subset(font)
    font.save(dest)


def rename(font):
    names = font["name"]
    for name_id in (1, 3, 4, 6, 16):
        names.removeNames(nameID=name_id)
    for platform, encoding, language in ((3, 10, 0x409), (1, 0, 0)):
        for name_id in (1, 3, 4, 6, 16):
            names.setName(
                FAMILY if name_id != 6 else "rune-term-CJK-Fallback",
                name_id, platform, encoding, language,
            )


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        han = pathlib.Path(tmp) / "han.ttf"
        hangul = pathlib.Path(tmp) / "hangul.ttf"
        reduced(CN, han_points(), han)
        reduced(KR, hangul_points(), hangul)
        merged = Merger().merge([han, hangul])
        rename(merged)
        merged.save(OUTPUT)
    print(f"{OUTPUT}: {OUTPUT.stat().st_size} bytes from {CN.name} + {KR.name}")


if __name__ == "__main__":
    main()
