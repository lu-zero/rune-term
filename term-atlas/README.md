# term-atlas

A CPU-side glyph atlas for terminal grids. It owns font faces, cell sprites and
fallback, but has no windowing or rendering backend: `rune-term` uploads the
atlas to wgpu, while a CPU frontend can blit it straight into a frame buffer.

## Use

```rust
use term_atlas::{Atlas, Face, PixelFormat};

let face = Face::load("Maple Mono NF");
let mut atlas = Atlas::with_face_format(face, 9, 17, PixelFormat::Argb);
let rect = atlas.ensure('中', 2).expect("a glyph");
// Blit from `atlas.pixels()` (coverage) or `atlas.color_pixels()` (color)
// at `rect.x`, `rect.y`, `rect.w`, `rect.h`. Check `take_dirty()` and
// `take_color_dirty()` before uploading or drawing again.
```

`PixelFormat::Rgba` is the default for wgpu textures; `PixelFormat::Argb`
stores premultiplied color as little-endian ARGB8888 bytes, as softbuffer's
frame buffer does.

## Custom font files

`Face::from_bytes(bytes, index)` takes raw font bytes and a TTC index; the
family name comes from the font's own name table. `Face::from_file(path)` reads
the first face of a file:

```rust
let face = Face::from_file("/path/to/face.ttf")?;
assert!(face.valid());
```

## Fallback

A codepoint missing from the profile's face is looked up in:

1. the compact embedded CJK subset,
2. an installed font found by [`term-font`](https://crates.io/crates/term-font),
3. CoreText's cascade on macOS.

Color glyphs come from COLR/sbix strikes through `term-font`; no color emoji
font is embedded.

## Features

| Feature | Default | Meaning |
| --- | --- | --- |
| `bundled-font` | yes | Embed Maple Mono NF as the always-usable face |
| `fallback-common` | yes | Common Han, kana and all modern Hangul |
| `fallback-ja` | no | JIS X 0208 kanji |
| `fallback-zh-full` | no | Full GBK Hanzi |
| `cascadia` | no | A second selectable face |

With `--no-default-features` no fonts are embedded; `Face::load` then uses any
installed family and `Face::valid()` reports whether one was found.

The font bytes live, zstd-compressed, in the `term-font-data-*` crates and are
decoded once at runtime by `ruzstd`. `fallback-subset.py` regenerates them from
the installed V8 Maple Mono CN/JP/KR faces (needs `fonttools` and `zstd`).

MIT licensed. The bundled fonts are SIL OFL 1.1; see the license files in
`assets/`.
