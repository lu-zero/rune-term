//! Terminal and UI palettes. Flexoki Black (pure black bg) is the default.
//!
//! Palette credits:
//! - Flexoki: Steph Ango, <https://stephango.com/flexoki>
//! - Dracula: Dracula Theme, <https://draculatheme.com>
//! - Nord: Sven Greb / Arctic Ice Studio, <https://www.nordtheme.com>
//! - Solarized: Ethan Schoonover, <https://ethanschoonover.com/solarized/>
//! - Gruvbox: Pavel Pertsev (morhetz), <https://github.com/morhetz/gruvbox>
//! - Catppuccin Mocha: Catppuccin, <https://catppuccin.com>

use rune::color::srgb_decode;
use rune::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub name: &'static str,
    pub bg: u32,
    pub fg: u32,
    pub fg_bright: u32,
    pub muted: u32,
    pub tab_bg: u32,
    pub active_border: u32,
    pub accent: u32,
    pub ansi16: [[u8; 3]; 16],
}

impl Palette {
    pub fn linear(&self, value: u32) -> Color {
        linear(value)
    }

    pub fn channels(&self, value: u32) -> [f32; 4] {
        channels(value)
    }

    pub fn from_bytes(&self, rgb: [u8; 3]) -> [f32; 4] {
        from_bytes(rgb)
    }
}

pub const BUILTINS: &[Palette] = &[
    FLEXOKI_BLACK,
    FLEXOKI_DARK,
    FLEXOKI_LIGHT,
    DRACULA,
    NORD,
    SOLARIZED_DARK,
    SOLARIZED_LIGHT,
    GRUVBOX_DARK,
    CATPPUCCIN_MOCHA,
];

pub fn resolve(name: &str) -> &'static Palette {
    let name = name.trim();
    BUILTINS
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .unwrap_or(&FLEXOKI_BLACK)
}

pub const fn hex(value: u32) -> [u8; 3] {
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

pub fn from_bytes(rgb: [u8; 3]) -> [f32; 4] {
    [
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
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

/// Flexoki dark with a pure black background.
pub const FLEXOKI_BLACK: Palette = Palette {
    name: "Flexoki Black",
    bg: 0x0000_00,
    fg: 0xCECD_C3,
    fg_bright: 0xE6E4_D9,
    muted: 0x8785_80,
    tab_bg: 0x1C1B_1A,
    active_border: 0x3433_31,
    accent: 0x205E_A6,
    ansi16: [
        hex(0x100F_0F),
        hex(0xAF30_29),
        hex(0x6680_0B),
        hex(0xAD83_01),
        hex(0x205E_A6),
        hex(0xA02F_6F),
        hex(0x2483_7B),
        hex(0xCECD_C3),
        hex(0x6F6E_69),
        hex(0xD14D_41),
        hex(0x879A_39),
        hex(0xD0A2_15),
        hex(0x4385_BE),
        hex(0xCE5D_97),
        hex(0x3AA9_9F),
        hex(0xE6E4_D9),
    ],
};

/// Flexoki dark using the palette's base background.
pub const FLEXOKI_DARK: Palette = Palette {
    name: "Flexoki Dark",
    bg: 0x100F_0F,
    fg: 0xCECD_C3,
    fg_bright: 0xFFFC_F0,
    muted: 0x8785_80,
    tab_bg: 0x1C1B_1A,
    active_border: 0x2827_26,
    accent: 0x205E_A6,
    ansi16: [
        hex(0x100F_0F),
        hex(0xAF30_29),
        hex(0x6680_0B),
        hex(0xAD83_01),
        hex(0x205E_A6),
        hex(0xA02F_6F),
        hex(0x2483_7B),
        hex(0xCECD_C3),
        hex(0x5756_53),
        hex(0xD14D_41),
        hex(0x879A_39),
        hex(0xD0A2_15),
        hex(0x4385_BE),
        hex(0xCE5D_97),
        hex(0x3AA9_9F),
        hex(0xFFFC_F0),
    ],
};

pub const FLEXOKI_LIGHT: Palette = Palette {
    name: "Flexoki Light",
    bg: 0xFFFC_F0,
    fg: 0x100F_0F,
    fg_bright: 0x100F_0F,
    muted: 0x6F6E_69,
    tab_bg: 0xF2F0_E5,
    active_border: 0xE6E4_D9,
    accent: 0x205E_A6,
    ansi16: [
        hex(0x100F_0F),
        hex(0xAF30_29),
        hex(0x6680_0B),
        hex(0xAD83_01),
        hex(0x205E_A6),
        hex(0xA02F_6F),
        hex(0x2483_7B),
        hex(0x6F6E_69),
        hex(0xB7B5_AC),
        hex(0xD14D_41),
        hex(0x879A_39),
        hex(0xD0A2_15),
        hex(0x4385_BE),
        hex(0xCE5D_97),
        hex(0x3AA9_9F),
        hex(0xCECD_C3),
    ],
};

/// <https://draculatheme.com>
pub const DRACULA: Palette = Palette {
    name: "Dracula",
    bg: 0x282A_36,
    fg: 0xF8F8_F2,
    fg_bright: 0xFFFF_FF,
    muted: 0x6272_A4,
    tab_bg: 0x2122_2C,
    active_border: 0x4447_5A,
    accent: 0xBD93_F9,
    ansi16: [
        hex(0x2122_2C),
        hex(0xFF55_55),
        hex(0x50FA_7B),
        hex(0xF1FA_8C),
        hex(0xBD93_F9),
        hex(0xFF79_C6),
        hex(0x8BE9_FD),
        hex(0xF8F8_F2),
        hex(0x6272_A4),
        hex(0xFF6E_6E),
        hex(0x69FF_94),
        hex(0xFFFF_A5),
        hex(0xD6AC_FF),
        hex(0xFF92_DF),
        hex(0xA4FF_FF),
        hex(0xFFFF_FF),
    ],
};

/// <https://www.nordtheme.com>
pub const NORD: Palette = Palette {
    name: "Nord",
    bg: 0x2E34_40,
    fg: 0xD8DE_E9,
    fg_bright: 0xECEF_F4,
    muted: 0x81A1_C1,
    tab_bg: 0x3B42_52,
    active_border: 0x4C56_6A,
    accent: 0x88C0_D0,
    ansi16: [
        hex(0x3B42_52),
        hex(0xBF61_6A),
        hex(0xA3BE_8C),
        hex(0xEBCB_8B),
        hex(0x81A1_C1),
        hex(0xB48E_AD),
        hex(0x88C0_D0),
        hex(0xE5E9_F0),
        hex(0x4C56_6A),
        hex(0xBF61_6A),
        hex(0xA3BE_8C),
        hex(0xEBCB_8B),
        hex(0x81A1_C1),
        hex(0xB48E_AD),
        hex(0x8FBC_BB),
        hex(0xECEF_F4),
    ],
};

/// <https://ethanschoonover.com/solarized/>
pub const SOLARIZED_DARK: Palette = Palette {
    name: "Solarized Dark",
    bg: 0x002B_36,
    fg: 0x8394_96,
    fg_bright: 0xFDF6_E3,
    muted: 0x586E_75,
    tab_bg: 0x0736_42,
    active_border: 0x586E_75,
    accent: 0x268B_D2,
    ansi16: [
        hex(0x0736_42),
        hex(0xDC32_2F),
        hex(0x8599_00),
        hex(0xB589_00),
        hex(0x268B_D2),
        hex(0xD336_82),
        hex(0x2AA1_98),
        hex(0xEEE8_D5),
        hex(0x586E_75),
        hex(0xCB4B_16),
        hex(0x586E_75),
        hex(0x657B_83),
        hex(0x8394_96),
        hex(0x6C71_C4),
        hex(0x93A1_A1),
        hex(0xFDF6_E3),
    ],
};

/// <https://ethanschoonover.com/solarized/>
pub const SOLARIZED_LIGHT: Palette = Palette {
    name: "Solarized Light",
    bg: 0xFDF6_E3,
    fg: 0x657B_83,
    fg_bright: 0x0736_42,
    muted: 0x93A1_A1,
    tab_bg: 0xEEE8_D5,
    active_border: 0x93A1_A1,
    accent: 0x268B_D2,
    ansi16: [
        hex(0x0736_42),
        hex(0xDC32_2F),
        hex(0x8599_00),
        hex(0xB589_00),
        hex(0x268B_D2),
        hex(0xD336_82),
        hex(0x2AA1_98),
        hex(0xEEE8_D5),
        hex(0x002B_36),
        hex(0xCB4B_16),
        hex(0x586E_75),
        hex(0x657B_83),
        hex(0x8394_96),
        hex(0x6C71_C4),
        hex(0x93A1_A1),
        hex(0xFDF6_E3),
    ],
};

/// <https://github.com/morhetz/gruvbox>
pub const GRUVBOX_DARK: Palette = Palette {
    name: "Gruvbox Dark",
    bg: 0x2828_28,
    fg: 0xEBDB_B2,
    fg_bright: 0xFBF1_C7,
    muted: 0x9283_74,
    tab_bg: 0x3C38_36,
    active_border: 0x5049_45,
    accent: 0x4585_88,
    ansi16: [
        hex(0x2828_28),
        hex(0xCC24_1D),
        hex(0x9897_1A),
        hex(0xD799_21),
        hex(0x4585_88),
        hex(0xB162_86),
        hex(0x689D_6A),
        hex(0xA899_84),
        hex(0x9283_74),
        hex(0xFB49_34),
        hex(0xB8BB_26),
        hex(0xFABC_2F),
        hex(0x83A5_98),
        hex(0xD386_9B),
        hex(0x8EC0_7C),
        hex(0xEBDB_B2),
    ],
};

/// <https://catppuccin.com>
pub const CATPPUCCIN_MOCHA: Palette = Palette {
    name: "Catppuccin Mocha",
    bg: 0x1E1E_2E,
    fg: 0xCDD6_F4,
    fg_bright: 0xCDD6_F4,
    muted: 0x9399_B2,
    tab_bg: 0x1818_25,
    active_border: 0x3132_44,
    accent: 0x89B4_FA,
    ansi16: [
        hex(0x4547_5A),
        hex(0xF38B_A8),
        hex(0xA6E3_A1),
        hex(0xF9E2_AF),
        hex(0x89B4_FA),
        hex(0xF5C2_E7),
        hex(0x94E2_D5),
        hex(0xBAC2_DE),
        hex(0x585B_70),
        hex(0xF38B_A8),
        hex(0xA6E3_A1),
        hex(0xF9E2_AF),
        hex(0x89B4_FA),
        hex(0xF5C2_E7),
        hex(0x94E2_D5),
        hex(0xA6AD_C8),
    ],
};
