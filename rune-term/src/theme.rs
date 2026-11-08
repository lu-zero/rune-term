//! Flexoki dark on a black page. Text is base-200, ANSI normal is the
//! 600 ink, and bright is the 400 accent. The page is black.
//! <https://stephango.com/flexoki>

use rune::color::srgb_decode;
use rune::Color;

pub const BG: u32 = 0x0000_00;
pub const FG: u32 = 0xCECD_C3;
pub const FG_BRIGHT: u32 = 0xE6E4_D9;
pub const MUTED: u32 = 0x8785_80;
pub const TAB_BG: u32 = 0x1C1B_1A;

/// Black through bright white.
pub const ANSI16: [[u8; 3]; 16] = [
    hex(0x100F_0F),
    hex(0xAF30_29),
    hex(0x6680_0B),
    hex(0xAD83_01),
    hex(0x205E_A6),
    hex(0xA02F_6F),
    hex(0x2483_7B),
    hex(FG),
    hex(0x6F6E_69),
    hex(0xD14D_41),
    hex(0x879A_39),
    hex(0xD0A2_15),
    hex(0x4385_BE),
    hex(0xCE5D_97),
    hex(0x3AA9_9F),
    hex(FG_BRIGHT),
];

const fn hex(value: u32) -> [u8; 3] {
    [
        ((value >> 16) & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        (value & 0xff) as u8,
    ]
}

pub fn channels(value: u32) -> [f32; 4] {
    let [r, g, b] = hex(value);
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    ]
}

pub fn linear(value: u32) -> Color {
    let [r, g, b, _] = channels(value);
    Color {
        r: srgb_decode(r),
        g: srgb_decode(g),
        b: srgb_decode(b),
        a: 1.0,
    }
}
