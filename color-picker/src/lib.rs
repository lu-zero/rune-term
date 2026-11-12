//! A small HSV color picker for Rune UIs.
//!
//! Colors are represented as 0xRRGGBB `u32` values in sRGB, matching
//! `rune-term` theme storage.

use rune::color::srgb_decode;
use rune::{col, el, row, text, Color, Drag, El, Phase};

/// HSV color. Hue in degrees [0, 360), saturation/value in [0, 1].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    pub fn from_rgb_u32(rgb: u32) -> Self {
        let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
        let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
        let b = (rgb & 0xFF) as f32 / 255.0;
        Self::from_rgb(r, g, b)
    }

    pub fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let s = if max <= 0.0 { 0.0 } else { d / max };
        let v = max;
        let h = if d <= 0.0 {
            0.0
        } else if (max - r).abs() < f32::EPSILON {
            60.0 * (((g - b) / d) % 6.0)
        } else if (max - g).abs() < f32::EPSILON {
            60.0 * (((b - r) / d) + 2.0)
        } else {
            60.0 * (((r - g) / d) + 4.0)
        };
        let h = if h < 0.0 { h + 360.0 } else { h };
        Self { h, s, v }
    }

    pub fn to_rgb_u32(self) -> u32 {
        let [r, g, b] = self.to_rgb();
        let r = (r.clamp(0.0, 1.0) * 255.0).round() as u32;
        let g = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
        let b = (b.clamp(0.0, 1.0) * 255.0).round() as u32;
        (r << 16) | (g << 8) | b
    }

    pub fn to_rgb(self) -> [f32; 3] {
        let h = self.h.rem_euclid(360.0);
        let s = self.s.clamp(0.0, 1.0);
        let v = self.v.clamp(0.0, 1.0);
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;
        let (r1, g1, b1) = match (h / 60.0).floor() as i32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        [r1 + m, g1 + m, b1 + m]
    }
}

/// Linear Rune color for a 0xRRGGBB value.
pub fn linear_rgb(rgb: u32) -> Color {
    let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
    let b = (rgb & 0xFF) as f32 / 255.0;
    Color {
        r: srgb_decode(r),
        g: srgb_decode(g),
        b: srgb_decode(b),
        a: 1.0,
    }
}

/// Format 0xRRGGBB as `#RRGGBB`.
pub fn hex6(rgb: u32) -> String {
    format!("#{:06X}", rgb & 0x00FF_FFFF)
}

/// Parse `#RRGGBB`, `0xRRGGBB`, or `RRGGBB`.
pub fn parse_hex(s: &str) -> Option<u32> {
    let s = s.trim();
    let s = s
        .strip_prefix('#')
        .or_else(|| s.strip_prefix("0x"))
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if s.len() != 6 {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

/// Which HSV channel a slider controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Hue,
    Saturation,
    Value,
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// A draggable 1D slider for a channel. `set` receives the new HSV.
pub fn channel_slider<S, Set>(key: &str, channel: Channel, hsv: Hsv, set: Set) -> El
where
    S: 'static,
    Set: Fn(&mut S, Hsv) + 'static + Clone,
{
    let (label, t, fill) = match channel {
        Channel::Hue => (
            "H",
            (hsv.h.rem_euclid(360.0) / 360.0),
            linear_rgb(
                Hsv {
                    h: hsv.h,
                    s: 1.0,
                    v: 1.0,
                }
                .to_rgb_u32(),
            ),
        ),
        Channel::Saturation => (
            "S",
            hsv.s,
            linear_rgb(
                Hsv {
                    h: hsv.h,
                    s: hsv.s,
                    v: hsv.v.max(0.001),
                }
                .to_rgb_u32(),
            ),
        ),
        Channel::Value => (
            "V",
            hsv.v,
            linear_rgb(
                Hsv {
                    h: hsv.h,
                    s: hsv.s,
                    v: hsv.v,
                }
                .to_rgb_u32(),
            ),
        ),
    };
    let t = clamp01(t);
    let set_drag = set.clone();
    let track = el()
        .key(key)
        .grow(1.0)
        .h(16.0)
        .radius(4.0)
        .fill(Color {
            r: 0.18,
            g: 0.18,
            b: 0.18,
            a: 1.0,
        })
        .border(
            1.0,
            Color {
                r: 0.35,
                g: 0.35,
                b: 0.35,
                a: 1.0,
            },
        )
        .on_drag(move |s: &mut S, d: &mut Drag, _tx| {
            if d.cancelled() {
                return;
            }
            if !matches!(d.phase, Phase::Down | Phase::Move | Phase::Up) {
                return;
            }
            let w = d.w.max(1.0);
            let x = d.x.clamp(0.0, w);
            let frac = (x / w).clamp(0.0, 1.0);
            let mut next = hsv;
            match channel {
                Channel::Hue => next.h = frac * 359.999,
                Channel::Saturation => next.s = frac,
                Channel::Value => next.v = frac,
            }
            set_drag(s, next);
        })
        .child(
            row()
                .w_full()
                .h_full()
                .items_center()
                .child(el().w_frac(t.max(0.02)).h_full().radius(4.0).fill(fill)),
        );
    row()
        .w_full()
        .h(20.0)
        .gap(8.0)
        .items_center()
        .child(el().w(14.0).child(text(label).color(Color {
            r: 0.75,
            g: 0.75,
            b: 0.75,
            a: 1.0,
        })))
        .child(track)
}

/// HSV sliders + swatch for editing a color.
pub fn hsv_editor<S, Set>(prefix: &str, hsv: Hsv, set: Set) -> El
where
    S: 'static,
    Set: Fn(&mut S, Hsv) + 'static + Clone,
{
    let rgb = hsv.to_rgb_u32();
    col()
        .w_full()
        .gap(6.0)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .items_center()
                .child(
                    el().w(28.0)
                        .h(28.0)
                        .radius(4.0)
                        .fill(linear_rgb(rgb))
                        .border(
                            1.0,
                            Color {
                                r: 0.4,
                                g: 0.4,
                                b: 0.4,
                                a: 1.0,
                            },
                        ),
                )
                .child(text(hex6(rgb)).color(Color::WHITE)),
        )
        .child(channel_slider::<S, _>(
            &format!("{prefix}-h"),
            Channel::Hue,
            hsv,
            set.clone(),
        ))
        .child(channel_slider::<S, _>(
            &format!("{prefix}-s"),
            Channel::Saturation,
            hsv,
            set.clone(),
        ))
        .child(channel_slider::<S, _>(
            &format!("{prefix}-v"),
            Channel::Value,
            hsv,
            set,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_hsv_roundtrip_samples() {
        for rgb in [0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0x336699] {
            let hsv = Hsv::from_rgb_u32(rgb);
            assert_eq!(hsv.to_rgb_u32(), rgb);
        }
    }

    #[test]
    fn hex_parse_format_roundtrip() {
        assert_eq!(parse_hex("#336699"), Some(0x336699));
        assert_eq!(parse_hex("0x336699"), Some(0x336699));
        assert_eq!(parse_hex("336699"), Some(0x336699));
        assert_eq!(hex6(0x336699), "#336699");
        assert_eq!(parse_hex("#123"), None);
    }
}
