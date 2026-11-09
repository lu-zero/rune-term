//! `cargo test -p rune-term --no-default-features`
//! Injects display bytes, raises the damage flag, and expects one
//! `update_rearm` plus a PNG of the tab strip and the solid slot.
//! Does not inject DEC 2026, and does not spawn a shell.

#![cfg(not(feature = "pty"))]

use std::path::PathBuf;
use std::sync::Mutex;

use rune::color::srgb_encode;
use rune::Color;

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn chrome_and_a_solid_slot() {
    let _guard = LOCK.lock().expect("headless lock");
    let path =
        std::env::temp_dir().join(format!("rune-term-foundation-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    rune_term::queue_inject(b"hi\r\n");
    rune_term::app()
        .args([
            "--snap".to_string(),
            path.display().to_string(),
            "--scale".to_string(),
            "1".to_string(),
            "--frames".to_string(),
            "1".to_string(),
            "--size".to_string(),
            "800x500".to_string(),
        ])
        .run();

    let report = rune_term::harness().expect("harness published by init");
    assert_eq!(report.rearms, 1, "one update_rearm for the injected frame");
    assert!(
        !report.frame_flag,
        "update_rearm clears the flag inside the terminal lock"
    );
    assert_eq!(
        report.output_len, 0,
        "inject_output is display bytes, not child output"
    );

    let (width, height, pixels) = read_png(&path);
    assert_eq!((width, height), (800, 500));
    let mut tab = 0;
    let mut slot_bytes = [0u8; 3];
    let mut slot = 0;
    for palette in [
        &rune_term::theme::resolve("Flexoki Black"),
        &rune_term::theme::resolve("Flexoki Dark"),
    ] {
        let t = count_near(&pixels, bytes(rune_term::tab(palette)), 2);
        let s_bytes = bytes(rune_term::slot(palette));
        let s = count_near(&pixels, s_bytes, 2);
        if s > slot {
            tab = t;
            slot_bytes = s_bytes;
            slot = s;
        }
    }
    let center = at(&pixels, width, width / 2, height / 2);
    let top = at(&pixels, width, width - 8, 8);
    // First row only, under the 28px tab. "hi" is the first two cells.
    // The cursor sits on the next row.
    let (pitch_x, pitch_y) = rune_term::cell_points();
    let cell_w = pitch_x.round() as u32;
    let cell_h = pitch_y.round() as u32;
    let ink = count_lighter(&pixels, width, slot_bytes, 0, cell_w * 2, 28, 28 + cell_h);
    assert!(
        tab > 1_000 && slot > 10_000,
        "tab {tab} slot {slot} top {top:?} center {center:?} ({})",
        path.display()
    );
    assert!(
        ink > 4,
        "glyph ink {ink} in the first cells, top {top:?} center {center:?} ({})",
        path.display()
    );
    let _ = std::fs::remove_file(&path);
}

fn bytes(color: Color) -> [u8; 3] {
    [
        (srgb_encode(color.r) * 255.0).round() as u8,
        (srgb_encode(color.g) * 255.0).round() as u8,
        (srgb_encode(color.b) * 255.0).round() as u8,
    ]
}

fn count_near(pixels: &[u8], expected: [u8; 3], tol: u8) -> usize {
    pixels
        .chunks_exact(4)
        .filter(|px| {
            px[0].abs_diff(expected[0]) <= tol
                && px[1].abs_diff(expected[1]) <= tol
                && px[2].abs_diff(expected[2]) <= tol
                && px[3] >= 250
        })
        .count()
}

fn count_lighter(
    pixels: &[u8],
    width: u32,
    floor: [u8; 3],
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
) -> usize {
    let base = [floor[0].min(8), floor[1].min(8), floor[2].min(8)];
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let px = at(pixels, width, x, y);
            if px[3] >= 250
                && px[0] > base[0].saturating_add(24)
                && px[1] > base[1].saturating_add(24)
                && px[2] > base[2].saturating_add(24)
            {
                n += 1;
            }
        }
    }
    n
}

fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn read_png(path: &PathBuf) -> (u32, u32, Vec<u8>) {
    let file = std::fs::File::open(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .expect("png frame fits in memory")
    ];
    let info = reader
        .next_frame(&mut buf)
        .unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

#[cfg(feature = "graphics")]
#[test]
fn kitty_image_pixels_appear() {
    let _guard = LOCK.lock().expect("headless lock");
    let path = std::env::temp_dir().join(format!("rune-term-kitty-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    // 2x2 solid red RGBA image, displayed over 4 columns and 2 rows.
    let seq = b"\x1b_Ga=T,f=32,s=2,v=2,c=4,r=2,i=1,q=2;/wAA//8AAP//AAD//wAA/w==\x1b\\";
    rune_term::queue_inject(seq);
    rune_term::app()
        .args([
            "--snap".to_string(),
            path.display().to_string(),
            "--scale".to_string(),
            "1".to_string(),
            "--frames".to_string(),
            "1".to_string(),
            "--size".to_string(),
            "800x500".to_string(),
        ])
        .run();

    let (width, height, pixels) = read_png(&path);
    assert_eq!((width, height), (800, 500));
    let red = pixels
        .chunks_exact(4)
        .filter(|px| px[0] >= 220 && px[1] <= 40 && px[2] <= 40 && px[3] >= 250)
        .count();
    assert!(
        red > 20,
        "expected kitty image pixels, got {red} red pixels ({})",
        path.display()
    );
    let _ = std::fs::remove_file(&path);
}
