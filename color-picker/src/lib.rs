//! A small RGB color picker for Rune UIs.
//!
//! Colors are represented as 0xRRGGBB `u32` values in sRGB, matching
//! `rune-term` theme storage.

use rune::color::srgb_decode;
use rune::{col, el, row, text, Color, Drag, El, Phase};

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

/// Which RGB channel a slider controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Red,
    Green,
    Blue,
}

/// The look of the sliders and the swatch, so the editor can follow the
/// window it draws in.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub track: Color,
    pub track_border: Color,
    pub thumb: Color,
    pub thumb_border: Color,
    pub label: Color,
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// A draggable 1D slider for one channel of `rgb`. `set` receives the new 0xRRGGBB.
pub fn channel_slider<S, Set>(key: &str, channel: Channel, rgb: u32, set: Set, style: Style) -> El
where
    S: 'static,
    Set: Fn(&mut S, u32) + 'static,
{
    let byte = channel_byte(rgb, channel);
    let (label, fill) = match channel {
        Channel::Red => ("R", linear_rgb(0xFF_0000)),
        Channel::Green => ("G", linear_rgb(0x00_FF00)),
        Channel::Blue => ("B", linear_rgb(0x00_00FF)),
    };
    let t = clamp01(byte as f32 / 255.0);
    let track_bg = style.track;
    let track_border = style.track_border;
    let thumb = el()
        .w(14.0)
        .h(14.0)
        .radius(7.0)
        .fill(style.thumb)
        .border(1.0, style.thumb_border);

    let fill_bar = row()
        .w_full()
        .h_full()
        .items_center()
        .pl(4.0)
        .pr(4.0)
        .child(el().h(8.0).radius(4.0).fill(fill).grow(t.max(0.0001)))
        .child(el().grow((1.0 - t).max(0.0001)));

    let thumb_row = el()
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .items_center()
        .child(
            row()
                .w_full()
                .h_full()
                .items_center()
                .pl(2.0)
                .pr(2.0)
                .child(el().grow(t.max(0.0001)))
                .child(thumb)
                .child(el().grow((1.0 - t).max(0.0001))),
        );

    let track = el()
        .key(key)
        .grow(1.0)
        .h(18.0)
        .radius(9.0)
        .fill(track_bg)
        .border(1.0, track_border)
        .center_y()
        .on_drag(move |s: &mut S, d: &mut Drag, _tx| {
            if d.cancelled() {
                return;
            }
            if !matches!(d.phase, Phase::Down | Phase::Move | Phase::Up) {
                return;
            }
            let w = d.w.max(1.0);
            let inset = 8.0_f32.min(w * 0.5);
            let span = (w - inset * 2.0).max(1.0);
            let x = (d.x - inset).clamp(0.0, span);
            let frac = (x / span).clamp(0.0, 1.0);
            let byte = (frac * 255.0).round() as u32;
            set(s, with_channel(rgb, channel, byte));
        })
        .child(fill_bar)
        .child(thumb_row);

    row()
        .w_full()
        .h(22.0)
        .gap(8.0)
        .items_center()
        .child(el().w(14.0).child(text(label).color(style.label)))
        .child(track)
}

/// Red, green and blue sliders for `rgb`. `set` receives the new 0xRRGGBB.
pub fn rgb_sliders<S, Set>(prefix: &str, rgb: u32, set: Set, style: Style) -> El
where
    S: 'static,
    Set: Fn(&mut S, u32) + 'static + Clone,
{
    col()
        .w_full()
        .gap(6.0)
        .child(channel_slider::<S, _>(
            &format!("{prefix}-r"),
            Channel::Red,
            rgb,
            set.clone(),
            style,
        ))
        .child(channel_slider::<S, _>(
            &format!("{prefix}-g"),
            Channel::Green,
            rgb,
            set.clone(),
            style,
        ))
        .child(channel_slider::<S, _>(
            &format!("{prefix}-b"),
            Channel::Blue,
            rgb,
            set,
            style,
        ))
}

fn channel_byte(rgb: u32, channel: Channel) -> u32 {
    match channel {
        Channel::Red => (rgb >> 16) & 0xFF,
        Channel::Green => (rgb >> 8) & 0xFF,
        Channel::Blue => rgb & 0xFF,
    }
}

fn with_channel(rgb: u32, channel: Channel, byte: u32) -> u32 {
    let byte = byte.min(255);
    match channel {
        Channel::Red => (rgb & 0x00_FFFF) | (byte << 16),
        Channel::Green => (rgb & 0xFF_00FF) | (byte << 8),
        Channel::Blue => (rgb & 0xFF_FF00) | byte,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_write_keeps_the_other_bytes() {
        assert_eq!(with_channel(0x11_22_33, Channel::Red, 0xAB), 0xAB_22_33);
        assert_eq!(with_channel(0x11_22_33, Channel::Green, 0xCD), 0x11_CD_33);
        assert_eq!(with_channel(0x11_22_33, Channel::Blue, 0xEF), 0x11_22_EF);
        assert_eq!(channel_byte(0xAB_CD_EF, Channel::Green), 0xCD);
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
