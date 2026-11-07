//! macOS cascade for codepoints Cascadia Code NF does not cover.
//! CoreText picks the system font, including color emoji, and draws it.
//! The emoji file stays on disk.

use std::sync::OnceLock;

use core_foundation::base::{CFRange, TCFType};
use core_foundation::string::CFString;
use core_graphics::base::{kCGBitmapByteOrder32Little, kCGImageAlphaPremultipliedFirst, CGFloat};
use core_graphics::color_space::CGColorSpace;
use core_graphics::context::{CGContext, CGTextDrawingMode};
use core_graphics::font::CGGlyph;
use core_graphics::geometry::{CGPoint, CGRect, CGSize};
use core_text::font::{self as ct_font, CTFont, CTFontRef};
use core_text::font_descriptor::{kCTFontColorGlyphsTrait, kCTFontOrientationDefault};

const ALPHA_ONLY: u32 = 7;
const MAX_BITMAP: usize = 512;

pub struct Drawn {
    pub color: bool,
    pub width: u32,
    pub height: u32,
    /// Left bearing, pixels. Positive is to the right of the pen.
    pub left: i32,
    /// Baseline-to-top, pixels. Positive is up.
    pub top: i32,
    pub bytes: Vec<u8>,
}

pub fn draw(font_bytes: &'static [u8], ch: char, size_px: f32) -> Option<Drawn> {
    if !size_px.is_finite() || size_px < 1.0 {
        return None;
    }
    let base = base_font(font_bytes)?;
    let sized = base.clone_with_font_size(f64::from(size_px));
    let text = ch.to_string();
    let string = CFString::new(&text);
    let mut utf16 = [0u16; 2];
    let encoded = ch.encode_utf16(&mut utf16);
    let count = encoded.len() as isize;
    let font_ref = unsafe {
        CTFontCreateForString(
            sized.as_concrete_TypeRef(),
            string.as_concrete_TypeRef(),
            CFRange {
                location: 0,
                length: count,
            },
        )
    };
    if font_ref.is_null() {
        return None;
    }
    let font = unsafe { CTFont::wrap_under_create_rule(font_ref) };
    if font.family_name().contains("LastResort") {
        return None;
    }
    let mut glyphs = [0u16; 2];
    let ok = unsafe { font.get_glyphs_for_characters(utf16.as_ptr(), glyphs.as_mut_ptr(), count) };
    if !ok || glyphs[0] == 0 {
        return None;
    }
    let color = font.symbolic_traits() & kCTFontColorGlyphsTrait != 0;
    rasterize(&font, glyphs[0], color)
}

fn base_font(font_bytes: &'static [u8]) -> Option<CTFont> {
    static FONT: OnceLock<Option<CTFont>> = OnceLock::new();
    FONT.get_or_init(|| {
        let desc = core_text::font_manager::create_font_descriptor(font_bytes).ok()?;
        Some(ct_font::new_from_descriptor(&desc, 1.0))
    })
    .clone()
}

fn rasterize(font: &CTFont, glyph: u16, color: bool) -> Option<Drawn> {
    let glyphs = [glyph as CGGlyph];
    let raw = font.get_bounding_rects_for_glyphs(kCTFontOrientationDefault, &glyphs);
    let bounds = if color && (raw.size.width <= 0.0 || raw.size.height <= 0.0) {
        let ascent = font.ascent();
        let descent = font.descent();
        let mut advance = CGSize::new(0.0, 0.0);
        unsafe {
            font.get_advances_for_glyphs(
                kCTFontOrientationDefault,
                glyphs.as_ptr(),
                &mut advance,
                1,
            );
        }
        if advance.width <= 0.0 || ascent + descent <= 0.0 {
            return None;
        }
        CGRect::new(
            &CGPoint::new(0.0, -descent),
            &CGSize::new(advance.width, ascent + descent),
        )
    } else if raw.size.width <= 0.0 || raw.size.height <= 0.0 {
        return None;
    } else {
        raw
    };

    const PAD: i32 = 1;
    let left = (bounds.origin.x.floor() as i32) - PAD;
    let bottom = (bounds.origin.y.floor() as i32) - PAD;
    let width = ((bounds.size.width.ceil() as i32) + 2 * PAD).max(1) as usize;
    let height = ((bounds.size.height.ceil() as i32) + 2 * PAD).max(1) as usize;
    if width > MAX_BITMAP || height > MAX_BITMAP {
        return None;
    }
    let top = bottom + height as i32;

    let (mut bytes, context) = if color {
        let mut bytes = vec![0u8; width * height * 4];
        let context = CGContext::create_bitmap_context(
            Some(bytes.as_mut_ptr().cast()),
            width,
            height,
            8,
            width * 4,
            &CGColorSpace::create_device_rgb(),
            kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little,
        );
        (bytes, context)
    } else {
        let mut bytes = vec![0u8; width * height];
        let context = CGContext::create_bitmap_context(
            Some(bytes.as_mut_ptr().cast()),
            width,
            height,
            8,
            width,
            &CGColorSpace::create_device_gray(),
            ALPHA_ONLY,
        );
        (bytes, context)
    };
    context.set_should_antialias(true);
    context.set_allows_antialiasing(true);
    context.set_should_smooth_fonts(true);
    context.set_gray_fill_color(0.0, 1.0);
    context.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
    context.set_allows_font_subpixel_positioning(false);
    context.set_should_subpixel_position_fonts(false);
    context.set_allows_font_subpixel_quantization(false);
    context.set_should_subpixel_quantize_fonts(false);
    let origin = CGPoint::new(-left as CGFloat, -bottom as CGFloat);
    font.draw_glyphs(&glyphs, &[origin], context);

    if color {
        bgra_to_rgba(&mut bytes);
        if bytes.chunks_exact(4).all(|px| px[3] == 0) {
            return None;
        }
    } else if bytes.iter().all(|px| *px == 0) {
        return None;
    }

    Some(Drawn {
        color,
        width: width as u32,
        height: height as u32,
        left,
        top,
        bytes,
    })
}

fn bgra_to_rgba(bytes: &mut [u8]) {
    for px in bytes.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    fn CTFontCreateForString(
        current_font: CTFontRef,
        string: core_foundation::string::CFStringRef,
        range: CFRange,
    ) -> CTFontRef;
}
