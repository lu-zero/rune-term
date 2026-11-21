#!/usr/bin/env python3
"""Regenerate the compressed fallback fonts from installed Maple Mono V8 faces.

Usage: python fallback-subset.py [CN.ttf] [JP.ttf] [KR.ttf]
Defaults are the installed NF-CN, NF-JP and NF-KR regulars.

Needs fonttools and the zstd CLI. Outputs go to the term-font-data-* crates:

term-font-data-common    — common Han, kana, full Hangul, Greek/Cyrillic.
term-font-data-ja        — JIS X 0208 kanji.
term-font-data-zh-full   — GBK Hanzi.
"""

import pathlib
import subprocess
import sys
import tempfile

from fontTools import subset
from fontTools.merge import Merger
from fontTools.ttLib import TTFont

ROOT = pathlib.Path(__file__).resolve().parent
DATA = ROOT.parent
FONTS_DIR = pathlib.Path.home() / "Library/Fonts"
CN = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else FONTS_DIR / "MapleMono-NF-CN-Regular.ttf"
JP = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else FONTS_DIR / "MapleMono-NF-JP-Regular.ttf"
KR = pathlib.Path(sys.argv[3]) if len(sys.argv) > 3 else FONTS_DIR / "MapleMono-NF-KR-Regular.ttf"
COMMON = DATA / "term-font-data-common/data/CjkFallback.ttf.zst"
JAPANESE = DATA / "term-font-data-ja/data/JaFallback.ttf.zst"
CHINESE_FULL = DATA / "term-font-data-zh-full/data/ZhFullFallback.ttf.zst"
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


def japanese_points():
    return sorted(
        cp
        for cp in decoded(
            [*range(0x81, 0xA0), *range(0xE0, 0xFC)], range(0x40, 0xFE), "shift_jis"
        )
        if 0x4E00 <= cp <= 0x9FFF
    )


def chinese_full_points():
    return sorted(
        cp
        for cp in decoded(range(0x81, 0xFE), range(0x40, 0xFE), "gbk")
        if 0x4E00 <= cp <= 0x9FFF
    )


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
        merged.save(pathlib.Path(tmp) / "common.ttf")

        reduced(JP, japanese_points(), pathlib.Path(tmp) / "ja.ttf")
        japanese = TTFont(pathlib.Path(tmp) / "ja.ttf")
        rename(japanese)
        japanese.save(pathlib.Path(tmp) / "ja-final.ttf")

        reduced(CN, chinese_full_points(), pathlib.Path(tmp) / "zh.ttf")
        chinese = TTFont(pathlib.Path(tmp) / "zh.ttf")
        rename(chinese)
        chinese.save(pathlib.Path(tmp) / "zh-final.ttf")

        for source, dest in [
            ("common.ttf", COMMON),
            ("ja-final.ttf", JAPANESE),
            ("zh-final.ttf", CHINESE_FULL),
        ]:
            subprocess.run(
                ["zstd", "-q", "-19", "-f", str(pathlib.Path(tmp) / source), "-o", str(dest)],
                check=True,
            )

    for path in (COMMON, JAPANESE, CHINESE_FULL):
        print(f"{path.name}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main()
