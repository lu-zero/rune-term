//! Terminal and UI palettes. Flexoki Black (pure black bg) is the default.
//!
//! Palette credits:
//! - Flexoki: Steph Ango, <https://stephango.com/flexoki>
//! - Dracula: Dracula Theme, <https://draculatheme.com>
//! - Nord: Sven Greb / Arctic Ice Studio, <https://www.nordtheme.com>
//! - Solarized: Ethan Schoonover, <https://ethanschoonover.com/solarized/>
//! - Gruvbox: Pavel Pertsev (morhetz), <https://github.com/morhetz/gruvbox>
//! - Catppuccin Mocha: Catppuccin, <https://catppuccin.com>

use std::sync::{Arc, OnceLock};

use rune::color::srgb_decode;
use rune::Color;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub name: String,
    pub bg: u32,
    pub fg: u32,
    pub fg_bright: u32,
    pub muted: u32,
    pub tab_bg: u32,
    pub active_border: u32,
    pub accent: u32,
    pub ansi16: [[u8; 3]; 16],
    pub author: String,
    pub url: String,
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

pub fn builtins() -> &'static [Arc<Palette>] {
    static ALL: OnceLock<Vec<Arc<Palette>>> = OnceLock::new();
    ALL.get_or_init(|| {
        vec![
            flexoki_black(),
            flexoki_dark(),
            flexoki_light(),
            dracula(),
            nord(),
            solarized_dark(),
            solarized_light(),
            gruvbox_dark(),
            catppuccin_mocha(),
        ]
    })
}

pub fn default_palette() -> Arc<Palette> {
    flexoki_black()
}

pub fn resolve_builtin(name: &str) -> Option<Arc<Palette>> {
    let name = name.trim();
    builtins()
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .cloned()
}

pub fn resolve(name: &str) -> Arc<Palette> {
    resolve_builtin(name).unwrap_or_else(default_palette)
}

pub const fn hex(value: u32) -> [u8; 3] {
    [
        ((value >> 16) & 0x0000_00FF) as u8,
        ((value >> 8) & 0x0000_00FF) as u8,
        (value & 0x0000_00FF) as u8,
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
fn flexoki_black() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Flexoki Black".to_owned(),
            bg: 0x0000_0000,
            fg: 0x00CE_CDC3,
            fg_bright: 0x00E6_E4D9,
            muted: 0x0087_8580,
            tab_bg: 0x001C_1B1A,
            active_border: 0x0034_3331,
            accent: 0x0020_5EA6,
            ansi16: [
                hex(0x0010_0F0F),
                hex(0x00AF_3029),
                hex(0x0066_800B),
                hex(0x00AD_8301),
                hex(0x0020_5EA6),
                hex(0x00A0_2F6F),
                hex(0x0024_837B),
                hex(0x00CE_CDC3),
                hex(0x006F_6E69),
                hex(0x00D1_4D41),
                hex(0x0087_9A39),
                hex(0x00D0_A215),
                hex(0x0043_85BE),
                hex(0x00CE_5D97),
                hex(0x003A_A99F),
                hex(0x00E6_E4D9),
            ],
            author: "Steph Ango".to_owned(),
            url: "https://stephango.com/flexoki".to_owned(),
        })
    })
    .clone()
}

/// Flexoki dark using the palette's base background.
fn flexoki_dark() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Flexoki Dark".to_owned(),
            bg: 0x0010_0F0F,
            fg: 0x00CE_CDC3,
            fg_bright: 0x00FF_FCF0,
            muted: 0x0087_8580,
            tab_bg: 0x001C_1B1A,
            active_border: 0x0028_2726,
            accent: 0x0020_5EA6,
            ansi16: [
                hex(0x0010_0F0F),
                hex(0x00AF_3029),
                hex(0x0066_800B),
                hex(0x00AD_8301),
                hex(0x0020_5EA6),
                hex(0x00A0_2F6F),
                hex(0x0024_837B),
                hex(0x00CE_CDC3),
                hex(0x0057_5653),
                hex(0x00D1_4D41),
                hex(0x0087_9A39),
                hex(0x00D0_A215),
                hex(0x0043_85BE),
                hex(0x00CE_5D97),
                hex(0x003A_A99F),
                hex(0x00FF_FCF0),
            ],
            author: "Steph Ango".to_owned(),
            url: "https://stephango.com/flexoki".to_owned(),
        })
    })
    .clone()
}

fn flexoki_light() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Flexoki Light".to_owned(),
            bg: 0x00FF_FCF0,
            fg: 0x0010_0F0F,
            fg_bright: 0x0010_0F0F,
            muted: 0x006F_6E69,
            tab_bg: 0x00F2_F0E5,
            active_border: 0x00E6_E4D9,
            accent: 0x0020_5EA6,
            ansi16: [
                hex(0x0010_0F0F),
                hex(0x00AF_3029),
                hex(0x0066_800B),
                hex(0x00AD_8301),
                hex(0x0020_5EA6),
                hex(0x00A0_2F6F),
                hex(0x0024_837B),
                hex(0x006F_6E69),
                hex(0x00B7_B5AC),
                hex(0x00D1_4D41),
                hex(0x0087_9A39),
                hex(0x00D0_A215),
                hex(0x0043_85BE),
                hex(0x00CE_5D97),
                hex(0x003A_A99F),
                hex(0x00CE_CDC3),
            ],
            author: "Steph Ango".to_owned(),
            url: "https://stephango.com/flexoki".to_owned(),
        })
    })
    .clone()
}

/// <https://draculatheme.com>
fn dracula() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Dracula".to_owned(),
            bg: 0x0028_2A36,
            fg: 0x00F8_F8F2,
            fg_bright: 0x00FF_FFFF,
            muted: 0x0062_72A4,
            tab_bg: 0x0021_222C,
            active_border: 0x0044_475A,
            accent: 0x00BD_93F9,
            ansi16: [
                hex(0x0021_222C),
                hex(0x00FF_5555),
                hex(0x0050_FA7B),
                hex(0x00F1_FA8C),
                hex(0x00BD_93F9),
                hex(0x00FF_79C6),
                hex(0x008B_E9FD),
                hex(0x00F8_F8F2),
                hex(0x0062_72A4),
                hex(0x00FF_6E6E),
                hex(0x0069_FF94),
                hex(0x00FF_FFA5),
                hex(0x00D6_ACFF),
                hex(0x00FF_92DF),
                hex(0x00A4_FFFF),
                hex(0x00FF_FFFF),
            ],
            author: "Dracula Theme".to_owned(),
            url: "https://draculatheme.com".to_owned(),
        })
    })
    .clone()
}

/// <https://www.nordtheme.com>
fn nord() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Nord".to_owned(),
            bg: 0x002E_3440,
            fg: 0x00D8_DEE9,
            fg_bright: 0x00EC_EFF4,
            muted: 0x0081_A1C1,
            tab_bg: 0x003B_4252,
            active_border: 0x004C_566A,
            accent: 0x0088_C0D0,
            ansi16: [
                hex(0x003B_4252),
                hex(0x00BF_616A),
                hex(0x00A3_BE8C),
                hex(0x00EB_CB8B),
                hex(0x0081_A1C1),
                hex(0x00B4_8EAD),
                hex(0x0088_C0D0),
                hex(0x00E5_E9F0),
                hex(0x004C_566A),
                hex(0x00BF_616A),
                hex(0x00A3_BE8C),
                hex(0x00EB_CB8B),
                hex(0x0081_A1C1),
                hex(0x00B4_8EAD),
                hex(0x008F_BCBB),
                hex(0x00EC_EFF4),
            ],
            author: "Sven Greb / Arctic Ice Studio".to_owned(),
            url: "https://www.nordtheme.com".to_owned(),
        })
    })
    .clone()
}

/// <https://ethanschoonover.com/solarized/>
fn solarized_dark() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Solarized Dark".to_owned(),
            bg: 0x0000_2B36,
            fg: 0x0083_9496,
            fg_bright: 0x00FD_F6E3,
            muted: 0x0058_6E75,
            tab_bg: 0x0007_3642,
            active_border: 0x0058_6E75,
            accent: 0x0026_8BD2,
            ansi16: [
                hex(0x0007_3642),
                hex(0x00DC_322F),
                hex(0x0085_9900),
                hex(0x00B5_8900),
                hex(0x0026_8BD2),
                hex(0x00D3_3682),
                hex(0x002A_A198),
                hex(0x00EE_E8D5),
                hex(0x0058_6E75),
                hex(0x00CB_4B16),
                hex(0x0058_6E75),
                hex(0x0065_7B83),
                hex(0x0083_9496),
                hex(0x006C_71C4),
                hex(0x0093_A1A1),
                hex(0x00FD_F6E3),
            ],
            author: "Ethan Schoonover".to_owned(),
            url: "https://ethanschoonover.com/solarized/".to_owned(),
        })
    })
    .clone()
}

/// <https://ethanschoonover.com/solarized/>
fn solarized_light() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Solarized Light".to_owned(),
            bg: 0x00FD_F6E3,
            fg: 0x0065_7B83,
            fg_bright: 0x0007_3642,
            muted: 0x0093_A1A1,
            tab_bg: 0x00EE_E8D5,
            active_border: 0x0093_A1A1,
            accent: 0x0026_8BD2,
            ansi16: [
                hex(0x0007_3642),
                hex(0x00DC_322F),
                hex(0x0085_9900),
                hex(0x00B5_8900),
                hex(0x0026_8BD2),
                hex(0x00D3_3682),
                hex(0x002A_A198),
                hex(0x00EE_E8D5),
                hex(0x0000_2B36),
                hex(0x00CB_4B16),
                hex(0x0058_6E75),
                hex(0x0065_7B83),
                hex(0x0083_9496),
                hex(0x006C_71C4),
                hex(0x0093_A1A1),
                hex(0x00FD_F6E3),
            ],
            author: "Ethan Schoonover".to_owned(),
            url: "https://ethanschoonover.com/solarized/".to_owned(),
        })
    })
    .clone()
}

/// <https://github.com/morhetz/gruvbox>
fn gruvbox_dark() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Gruvbox Dark".to_owned(),
            bg: 0x0028_2828,
            fg: 0x00EB_DBB2,
            fg_bright: 0x00FB_F1C7,
            muted: 0x0092_8374,
            tab_bg: 0x003C_3836,
            active_border: 0x0050_4945,
            accent: 0x0045_8588,
            ansi16: [
                hex(0x0028_2828),
                hex(0x00CC_241D),
                hex(0x0098_971A),
                hex(0x00D7_9921),
                hex(0x0045_8588),
                hex(0x00B1_6286),
                hex(0x0068_9D6A),
                hex(0x00A8_9984),
                hex(0x0092_8374),
                hex(0x00FB_4934),
                hex(0x00B8_BB26),
                hex(0x00FA_BC2F),
                hex(0x0083_A598),
                hex(0x00D3_869B),
                hex(0x008E_C07C),
                hex(0x00EB_DBB2),
            ],
            author: "Pavel Pertsev (morhetz)".to_owned(),
            url: "https://github.com/morhetz/gruvbox".to_owned(),
        })
    })
    .clone()
}

/// <https://catppuccin.com>
fn catppuccin_mocha() -> Arc<Palette> {
    static P: OnceLock<Arc<Palette>> = OnceLock::new();
    P.get_or_init(|| {
        Arc::new(Palette {
            name: "Catppuccin Mocha".to_owned(),
            bg: 0x001E_1E2E,
            fg: 0x00CD_D6F4,
            fg_bright: 0x00CD_D6F4,
            muted: 0x0093_99B2,
            tab_bg: 0x0018_1825,
            active_border: 0x0031_3244,
            accent: 0x0089_B4FA,
            ansi16: [
                hex(0x0045_475A),
                hex(0x00F3_8BA8),
                hex(0x00A6_E3A1),
                hex(0x00F9_E2AF),
                hex(0x0089_B4FA),
                hex(0x00F5_C2E7),
                hex(0x0094_E2D5),
                hex(0x00BA_C2DE),
                hex(0x0058_5B70),
                hex(0x00F3_8BA8),
                hex(0x00A6_E3A1),
                hex(0x00F9_E2AF),
                hex(0x0089_B4FA),
                hex(0x00F5_C2E7),
                hex(0x0094_E2D5),
                hex(0x00A6_ADC8),
            ],
            author: "Catppuccin".to_owned(),
            url: "https://catppuccin.com".to_owned(),
        })
    })
    .clone()
}
