//! Geometric glyphs rasterized into the gray atlas.

use crate::atlas::{Atlas, GlyphRect};

pub(crate) fn raster(atlas: &mut Atlas, ch: char, cells: u8) -> Option<GlyphRect> {
    let cp = u32::from(ch);
    match cp {
        0x2500..=0x257F => box_drawing(atlas, cp, cells),
        0x2580..=0x259F => block(atlas, cp, cells),
        0x2800..=0x28FF => braille(atlas, cp, cells),
        0x1FB00..=0x1FB3B
        | 0x1CD00..=0x1CDE5
        | 0x1CEA0
        | 0x1CEA3
        | 0x1CEA8
        | 0x1CEAB
        | 0x1FBE6
        | 0x1FBE7 => legacy(atlas, cp, cells),
        _ => None,
    }
}

struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![0u8; (w as usize) * (h as usize)],
        }
    }

    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, a: u8) {
        let x0 = x0.clamp(0, self.w as i32);
        let x1 = x1.clamp(0, self.w as i32);
        let y0 = y0.clamp(0, self.h as i32);
        let y1 = y1.clamp(0, self.h as i32);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        for y in y0..y1 {
            let row = (y as usize) * (self.w as usize);
            for p in &mut self.px[row + x0 as usize..row + x1 as usize] {
                if a > *p {
                    *p = a;
                }
            }
        }
    }

    /// Complementary fractional edges so adjacent blocks share a pixel boundary.
    fn fill_fraction(&mut self, x0: f64, x1: f64, y0: f64, y1: f64, a: u8) {
        self.rect(
            frac_min(x0, self.w),
            frac_min(y0, self.h),
            frac_max(x1, self.w),
            frac_max(y1, self.h),
            a,
        );
    }

    fn blend(&mut self, x: i32, y: i32, a: u8) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || a == 0 {
            return;
        }
        let i = (y as u32 * self.w + x as u32) as usize;
        if a > self.px[i] {
            self.px[i] = a;
        }
    }

    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thick: f32) {
        let half = (thick / 2.0).max(0.5);
        let min_x = (x0.min(x1) - half - 1.0).floor() as i32;
        let max_x = (x0.max(x1) + half + 1.0).ceil() as i32;
        let min_y = (y0.min(y1) - half - 1.0).floor() as i32;
        let max_y = (y0.max(y1) + half + 1.0).ceil() as i32;
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len2 = dx * dx + dy * dy;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let t = if len2 > 0.0 {
                    (((px - x0) * dx + (py - y0) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let cx = x0 + t * dx;
                let cy = y0 + t * dy;
                let dist = ((px - cx) * (px - cx) + (py - cy) * (py - cy)).sqrt();
                let cov = (half + 0.5 - dist).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, (cov * 255.0).round() as u8);
                }
            }
        }
    }
}

fn frac_min(frac: f64, size: u32) -> i32 {
    let s = size as f64;
    (s - ((1.0 - frac) * s).round()) as i32
}

fn frac_max(frac: f64, size: u32) -> i32 {
    (frac * size as f64).round() as i32
}

fn commit(atlas: &mut Atlas, cells: u8, canvas: Canvas) -> Option<GlyphRect> {
    let origin = atlas.alloc(cells)?;
    Some(atlas.insert(&canvas.px, canvas.w, canvas.h, origin))
}

fn braille(atlas: &mut Atlas, cp: u32, cells: u8) -> Option<GlyphRect> {
    let (cw, ch) = atlas.cell();
    let slot_w = cw * u32::from(cells.max(1));
    let mut c = Canvas::new(slot_w, ch);
    let (width, height) = (slot_w as i32, ch as i32);
    let mut w = (width / 4).min(height / 8);
    let mut x_spacing = width / 4;
    let mut y_spacing = height / 8;
    let mut x_margin = x_spacing / 2;
    let mut y_margin = y_spacing / 2;
    let mut x_left = width - 2 * x_margin - x_spacing - 2 * w;
    let mut y_left = height - 2 * y_margin - 3 * y_spacing - 4 * w;
    if x_left >= 2 && y_left >= 4 && w == 0 {
        w += 1;
        x_left -= 2;
        y_left -= 4;
    }
    if x_left >= 2 && x_margin == 0 {
        x_margin = 1;
        x_left -= 2;
    }
    if y_left >= 2 && y_margin == 0 {
        y_margin = 1;
        y_left -= 2;
    }
    if x_left >= 1 {
        x_spacing += 1;
        x_left -= 1;
    }
    if y_left >= 3 {
        y_spacing += 1;
        y_left -= 3;
    }
    if x_left >= 2 {
        x_margin += 1;
        x_left -= 2;
    }
    if y_left >= 2 {
        y_margin += 1;
        y_left -= 2;
    }
    if x_left >= 2 && y_left >= 4 {
        w += 1;
    }
    let xs = [x_margin, x_margin + w + x_spacing];
    let ys = [
        y_margin,
        y_margin + w + y_spacing,
        y_margin + 2 * (w + y_spacing),
        y_margin + 3 * (w + y_spacing),
    ];
    let bits = (cp & 0xFF) as u8;
    for (mask, col, row) in [
        (0x01u8, 0usize, 0usize),
        (0x02, 0, 1),
        (0x04, 0, 2),
        (0x40, 0, 3),
        (0x08, 1, 0),
        (0x10, 1, 1),
        (0x20, 1, 2),
        (0x80, 1, 3),
    ] {
        if bits & mask != 0 {
            let x = xs[col];
            let y = ys[row];
            c.rect(x, y, x + w, y + w, 0xFF);
        }
    }
    commit(atlas, cells.max(1), c)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    None,
    Light,
    Heavy,
    Double,
}

use Style::{Double as D, Heavy as H, Light as L, None as N};

#[derive(Clone, Copy)]
struct Lines {
    up: Style,
    right: Style,
    down: Style,
    left: Style,
}

const fn ln(up: Style, right: Style, down: Style, left: Style) -> Lines {
    Lines {
        up,
        right,
        down,
        left,
    }
}

fn box_thickness(cell_h: u32) -> u32 {
    ((cell_h as f32 / 16.0).round().max(1.0)) as u32
}

fn box_drawing(atlas: &mut Atlas, cp: u32, cells: u8) -> Option<GlyphRect> {
    let (cw, ch) = atlas.cell();
    let slot_w = cw * u32::from(cells.max(1));
    let mut c = Canvas::new(slot_w, ch);
    let base = box_thickness(ch);
    let l = base.max(1);
    let h = base.saturating_mul(2).max(1);
    match cp {
        0x2500 => lc(&mut c, ln(N, L, N, L)),
        0x2501 => lc(&mut c, ln(N, H, N, H)),
        0x2502 => lc(&mut c, ln(L, N, L, N)),
        0x2503 => lc(&mut c, ln(H, N, H, N)),
        0x2504 => dash_h(&mut c, 3, l, l.max(4)),
        0x2505 => dash_h(&mut c, 3, h, l.max(4)),
        0x2506 => dash_v(&mut c, 3, l, l.max(4)),
        0x2507 => dash_v(&mut c, 3, h, l.max(4)),
        0x2508 => dash_h(&mut c, 4, l, l.max(4)),
        0x2509 => dash_h(&mut c, 4, h, l.max(4)),
        0x250A => dash_v(&mut c, 4, l, l.max(4)),
        0x250B => dash_v(&mut c, 4, h, l.max(4)),
        0x250C => lc(&mut c, ln(N, L, L, N)),
        0x250D => lc(&mut c, ln(N, H, L, N)),
        0x250E => lc(&mut c, ln(N, L, H, N)),
        0x250F => lc(&mut c, ln(N, H, H, N)),
        0x2510 => lc(&mut c, ln(N, N, L, L)),
        0x2511 => lc(&mut c, ln(N, N, L, H)),
        0x2512 => lc(&mut c, ln(N, N, H, L)),
        0x2513 => lc(&mut c, ln(N, N, H, H)),
        0x2514 => lc(&mut c, ln(L, L, N, N)),
        0x2515 => lc(&mut c, ln(L, H, N, N)),
        0x2516 => lc(&mut c, ln(H, L, N, N)),
        0x2517 => lc(&mut c, ln(H, H, N, N)),
        0x2518 => lc(&mut c, ln(L, N, N, L)),
        0x2519 => lc(&mut c, ln(L, N, N, H)),
        0x251A => lc(&mut c, ln(H, N, N, L)),
        0x251B => lc(&mut c, ln(H, N, N, H)),
        0x251C => lc(&mut c, ln(L, L, L, N)),
        0x251D => lc(&mut c, ln(L, H, L, N)),
        0x251E => lc(&mut c, ln(H, L, L, N)),
        0x251F => lc(&mut c, ln(L, L, H, N)),
        0x2520 => lc(&mut c, ln(H, L, H, N)),
        0x2521 => lc(&mut c, ln(H, H, L, N)),
        0x2522 => lc(&mut c, ln(L, H, H, N)),
        0x2523 => lc(&mut c, ln(H, H, H, N)),
        0x2524 => lc(&mut c, ln(L, N, L, L)),
        0x2525 => lc(&mut c, ln(L, N, L, H)),
        0x2526 => lc(&mut c, ln(H, N, L, L)),
        0x2527 => lc(&mut c, ln(L, N, H, L)),
        0x2528 => lc(&mut c, ln(H, N, H, L)),
        0x2529 => lc(&mut c, ln(H, N, L, H)),
        0x252A => lc(&mut c, ln(L, N, H, H)),
        0x252B => lc(&mut c, ln(H, N, H, H)),
        0x252C => lc(&mut c, ln(N, L, L, L)),
        0x252D => lc(&mut c, ln(N, L, L, H)),
        0x252E => lc(&mut c, ln(N, H, L, L)),
        0x252F => lc(&mut c, ln(N, H, L, H)),
        0x2530 => lc(&mut c, ln(N, L, H, L)),
        0x2531 => lc(&mut c, ln(N, L, H, H)),
        0x2532 => lc(&mut c, ln(N, H, H, L)),
        0x2533 => lc(&mut c, ln(N, H, H, H)),
        0x2534 => lc(&mut c, ln(L, L, N, L)),
        0x2535 => lc(&mut c, ln(L, L, N, H)),
        0x2536 => lc(&mut c, ln(L, H, N, L)),
        0x2537 => lc(&mut c, ln(L, H, N, H)),
        0x2538 => lc(&mut c, ln(H, L, N, L)),
        0x2539 => lc(&mut c, ln(H, L, N, H)),
        0x253A => lc(&mut c, ln(H, H, N, L)),
        0x253B => lc(&mut c, ln(H, H, N, H)),
        0x253C => lc(&mut c, ln(L, L, L, L)),
        0x253D => lc(&mut c, ln(L, L, L, H)),
        0x253E => lc(&mut c, ln(L, H, L, L)),
        0x253F => lc(&mut c, ln(L, H, L, H)),
        0x2540 => lc(&mut c, ln(H, L, L, L)),
        0x2541 => lc(&mut c, ln(L, L, H, L)),
        0x2542 => lc(&mut c, ln(H, L, H, L)),
        0x2543 => lc(&mut c, ln(H, L, L, H)),
        0x2544 => lc(&mut c, ln(H, H, L, L)),
        0x2545 => lc(&mut c, ln(L, L, H, H)),
        0x2546 => lc(&mut c, ln(L, H, H, L)),
        0x2547 => lc(&mut c, ln(H, H, L, H)),
        0x2548 => lc(&mut c, ln(L, H, H, H)),
        0x2549 => lc(&mut c, ln(H, L, H, H)),
        0x254A => lc(&mut c, ln(H, H, H, L)),
        0x254B => lc(&mut c, ln(H, H, H, H)),
        0x254C => dash_h(&mut c, 2, l, l),
        0x254D => dash_h(&mut c, 2, h, h),
        0x254E => dash_v(&mut c, 2, l, h),
        0x254F => dash_v(&mut c, 2, h, h),
        0x2550 => lc(&mut c, ln(N, D, N, D)),
        0x2551 => lc(&mut c, ln(D, N, D, N)),
        0x2552 => lc(&mut c, ln(N, D, L, N)),
        0x2553 => lc(&mut c, ln(N, L, D, N)),
        0x2554 => lc(&mut c, ln(N, D, D, N)),
        0x2555 => lc(&mut c, ln(N, N, L, D)),
        0x2556 => lc(&mut c, ln(N, N, D, L)),
        0x2557 => lc(&mut c, ln(N, N, D, D)),
        0x2558 => lc(&mut c, ln(L, D, N, N)),
        0x2559 => lc(&mut c, ln(D, L, N, N)),
        0x255A => lc(&mut c, ln(D, D, N, N)),
        0x255B => lc(&mut c, ln(L, N, N, D)),
        0x255C => lc(&mut c, ln(D, N, N, L)),
        0x255D => lc(&mut c, ln(D, N, N, D)),
        0x255E => lc(&mut c, ln(L, D, L, N)),
        0x255F => lc(&mut c, ln(D, L, D, N)),
        0x2560 => lc(&mut c, ln(D, D, D, N)),
        0x2561 => lc(&mut c, ln(L, N, L, D)),
        0x2562 => lc(&mut c, ln(D, N, D, L)),
        0x2563 => lc(&mut c, ln(D, N, D, D)),
        0x2564 => lc(&mut c, ln(N, D, L, D)),
        0x2565 => lc(&mut c, ln(N, L, D, L)),
        0x2566 => lc(&mut c, ln(N, D, D, D)),
        0x2567 => lc(&mut c, ln(L, D, N, D)),
        0x2568 => lc(&mut c, ln(D, L, N, L)),
        0x2569 => lc(&mut c, ln(D, D, N, D)),
        0x256A => lc(&mut c, ln(L, D, L, D)),
        0x256B => lc(&mut c, ln(D, L, D, L)),
        0x256C => lc(&mut c, ln(D, D, D, D)),
        0x256D..=0x2570 => arc(&mut c, cp),
        0x2571 => diagonal_ur_ll(&mut c, l),
        0x2572 => diagonal_ul_lr(&mut c, l),
        0x2573 => {
            diagonal_ur_ll(&mut c, l);
            diagonal_ul_lr(&mut c, l);
        }
        0x2574 => lc(&mut c, ln(N, N, N, L)),
        0x2575 => lc(&mut c, ln(L, N, N, N)),
        0x2576 => lc(&mut c, ln(N, L, N, N)),
        0x2577 => lc(&mut c, ln(N, N, L, N)),
        0x2578 => lc(&mut c, ln(N, N, N, H)),
        0x2579 => lc(&mut c, ln(H, N, N, N)),
        0x257A => lc(&mut c, ln(N, H, N, N)),
        0x257B => lc(&mut c, ln(N, N, H, N)),
        0x257C => lc(&mut c, ln(N, H, N, L)),
        0x257D => lc(&mut c, ln(L, N, H, N)),
        0x257E => lc(&mut c, ln(N, L, N, H)),
        0x257F => lc(&mut c, ln(H, N, L, N)),
        _ => return None,
    }
    commit(atlas, cells.max(1), c)
}

fn lc(c: &mut Canvas, lines: Lines) {
    let cw = c.w;
    let ch = c.h;
    let base = box_thickness(ch);
    let light = base.max(1);
    let heavy = base.saturating_mul(2).max(1);
    let h_light_top = ch.saturating_sub(light) / 2;
    let h_light_bottom = h_light_top + light;
    let h_heavy_top = ch.saturating_sub(heavy) / 2;
    let h_heavy_bottom = h_heavy_top + heavy;
    let h_double_top = h_light_top.saturating_sub(light);
    let h_double_bottom = h_light_bottom + light;
    let v_light_left = cw.saturating_sub(light) / 2;
    let v_light_right = v_light_left + light;
    let v_heavy_left = cw.saturating_sub(heavy) / 2;
    let v_heavy_right = v_heavy_left + heavy;
    let v_double_left = v_light_left.saturating_sub(light);
    let v_double_right = v_light_right + light;

    let up_bottom = if lines.left == H || lines.right == H {
        h_heavy_bottom
    } else if lines.left != lines.right || lines.down == lines.up {
        if lines.left == D || lines.right == D {
            h_double_bottom
        } else {
            h_light_bottom
        }
    } else if lines.left == N && lines.right == N {
        h_light_bottom
    } else {
        h_light_top
    };
    let down_top = if lines.left == H || lines.right == H {
        h_heavy_top
    } else if lines.left != lines.right || lines.up == lines.down {
        if lines.left == D || lines.right == D {
            h_double_top
        } else {
            h_light_top
        }
    } else if lines.left == N && lines.right == N {
        h_light_top
    } else {
        h_light_bottom
    };
    let left_right = if lines.up == H || lines.down == H {
        v_heavy_right
    } else if lines.up != lines.down || lines.left == lines.right {
        if lines.up == D || lines.down == D {
            v_double_right
        } else {
            v_light_right
        }
    } else if lines.up == N && lines.down == N {
        v_light_right
    } else {
        v_light_left
    };
    let right_left = if lines.up == H || lines.down == H {
        v_heavy_left
    } else if lines.up != lines.down || lines.right == lines.left {
        if lines.up == D || lines.down == D {
            v_double_left
        } else {
            v_light_left
        }
    } else if lines.up == N && lines.down == N {
        v_light_left
    } else {
        v_light_right
    };
    let i = |v: u32| v as i32;
    match lines.up {
        N => {}
        L => c.rect(i(v_light_left), 0, i(v_light_right), i(up_bottom), 0xFF),
        H => c.rect(i(v_heavy_left), 0, i(v_heavy_right), i(up_bottom), 0xFF),
        D => {
            let left_bottom = if lines.left == D {
                h_light_top
            } else {
                up_bottom
            };
            let right_bottom = if lines.right == D {
                h_light_top
            } else {
                up_bottom
            };
            c.rect(i(v_double_left), 0, i(v_light_left), i(left_bottom), 0xFF);
            c.rect(
                i(v_light_right),
                0,
                i(v_double_right),
                i(right_bottom),
                0xFF,
            );
        }
    }
    match lines.right {
        N => {}
        L => c.rect(
            i(right_left),
            i(h_light_top),
            i(cw),
            i(h_light_bottom),
            0xFF,
        ),
        H => c.rect(
            i(right_left),
            i(h_heavy_top),
            i(cw),
            i(h_heavy_bottom),
            0xFF,
        ),
        D => {
            let top_left = if lines.up == D {
                v_light_right
            } else {
                right_left
            };
            let bottom_left = if lines.down == D {
                v_light_right
            } else {
                right_left
            };
            c.rect(i(top_left), i(h_double_top), i(cw), i(h_light_top), 0xFF);
            c.rect(
                i(bottom_left),
                i(h_light_bottom),
                i(cw),
                i(h_double_bottom),
                0xFF,
            );
        }
    }
    match lines.down {
        N => {}
        L => c.rect(i(v_light_left), i(down_top), i(v_light_right), i(ch), 0xFF),
        H => c.rect(i(v_heavy_left), i(down_top), i(v_heavy_right), i(ch), 0xFF),
        D => {
            let left_top = if lines.left == D {
                h_light_bottom
            } else {
                down_top
            };
            let right_top = if lines.right == D {
                h_light_bottom
            } else {
                down_top
            };
            c.rect(i(v_double_left), i(left_top), i(v_light_left), i(ch), 0xFF);
            c.rect(
                i(v_light_right),
                i(right_top),
                i(v_double_right),
                i(ch),
                0xFF,
            );
        }
    }
    match lines.left {
        N => {}
        L => c.rect(0, i(h_light_top), i(left_right), i(h_light_bottom), 0xFF),
        H => c.rect(0, i(h_heavy_top), i(left_right), i(h_heavy_bottom), 0xFF),
        D => {
            let top_right = if lines.up == D {
                v_light_left
            } else {
                left_right
            };
            let bottom_right = if lines.down == D {
                v_light_left
            } else {
                left_right
            };
            c.rect(0, i(h_double_top), i(top_right), i(h_light_top), 0xFF);
            c.rect(
                0,
                i(h_light_bottom),
                i(bottom_right),
                i(h_double_bottom),
                0xFF,
            );
        }
    }
}

fn hline_middle_light(c: &mut Canvas) {
    let ch = c.h;
    let l = box_thickness(ch).max(1);
    let top = ch.saturating_sub(l) / 2;
    c.rect(0, top as i32, c.w as i32, (top + l) as i32, 0xFF);
}

fn vline_middle_light(c: &mut Canvas) {
    let cw = c.w;
    let l = box_thickness(c.h).max(1);
    let left = cw.saturating_sub(l) / 2;
    c.rect(left as i32, 0, (left + l) as i32, c.h as i32, 0xFF);
}

fn dash_h(c: &mut Canvas, count: i32, thick: u32, desired_gap: u32) {
    let cw = c.w as i32;
    let gap_count = count;
    if cw < count + gap_count {
        hline_middle_light(c);
        return;
    }
    let gap_width = desired_gap.min(c.w / (2 * count as u32)) as i32;
    let total_gap = gap_count * gap_width;
    let total_dash = cw - total_gap;
    let dash = total_dash / count;
    let mut rem = total_dash % count;
    let y = (c.h.saturating_sub(thick) / 2) as i32;
    let mut x = gap_width / 2;
    for _ in 0..count {
        let mut x1 = x + dash;
        if rem > 0 {
            rem -= 1;
            x1 += 1;
        }
        c.rect(x, y, x1, y + thick as i32, 0xFF);
        x = x1 + gap_width;
    }
}

fn dash_v(c: &mut Canvas, count: i32, thick: u32, desired_gap: u32) {
    let ch = c.h as i32;
    let gap_count = count;
    if ch < count + gap_count {
        vline_middle_light(c);
        return;
    }
    let gap_height = desired_gap.min(c.h / (2 * count as u32)) as i32;
    let total_gap = gap_count * gap_height;
    let total_dash = ch - total_gap;
    let dash = total_dash / count;
    let mut rem = total_dash % count;
    let x = (c.w.saturating_sub(thick) / 2) as i32;
    let mut y = 0;
    for _ in 0..count {
        let mut y1 = y + dash;
        if rem > 0 {
            rem -= 1;
            y1 += 1;
        }
        c.rect(x, y, x + thick as i32, y1, 0xFF);
        y = y1 + gap_height;
    }
}

fn arc(c: &mut Canvas, cp: u32) {
    let cw = c.w as f32;
    let ch = c.h as f32;
    let thick = box_thickness(c.h).max(1);
    let thick_f = thick as f32;
    let half = thick_f / 2.0;
    let cx = (c.w.saturating_sub(thick) / 2) as f32 + half;
    let cy = (c.h.saturating_sub(thick) / 2) as f32 + half;
    let r = cw.min(ch) / 2.0;
    let (sx, sy, ex, ey) = match cp {
        0x256D => (cx, 0.0, 0.0, cy), // ╭ up -> left
        0x256E => (cx, 0.0, cw, cy),  // ╮ up -> right
        0x256F => (cx, ch, cw, cy),   // ╯ down -> right
        _ => (cx, ch, 0.0, cy),       // ╰ down -> left
    };
    let steps = ((r * 3.14159) / 0.75).ceil().max(8.0) as usize;
    let mut px = sx;
    let mut py = sy;
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let (nx, ny) = arc_point(cp, cx, cy, r, t, sx, sy, ex, ey);
        c.line(px, py, nx, ny, thick_f);
        px = nx;
        py = ny;
    }
}

fn arc_point(
    cp: u32,
    cx: f32,
    cy: f32,
    r: f32,
    t: f32,
    sx: f32,
    sy: f32,
    ex: f32,
    ey: f32,
) -> (f32, f32) {
    // Quarter-circle approximation between the stem direction and arm direction.
    let (a0, a1) = match cp {
        0x256D => (std::f32::consts::PI, 1.5 * std::f32::consts::PI), // leftward end to upward
        0x256E => (1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI),
        0x256F => (0.0, 0.5 * std::f32::consts::PI),
        _ => (0.5 * std::f32::consts::PI, std::f32::consts::PI),
    };
    let a = a0 + (a1 - a0) * t;
    let on_arc_x = cx + r * a.cos();
    let on_arc_y = cy + r * a.sin();
    // Blend: start from stem end, sweep through arc center, end at arm end.
    if t < 0.5 {
        let s = t * 2.0;
        (sx + (on_arc_x - sx) * s, sy + (on_arc_y - sy) * s)
    } else {
        let s = (t - 0.5) * 2.0;
        (
            on_arc_x + (ex - on_arc_x) * s,
            on_arc_y + (ey - on_arc_y) * s,
        )
    }
}

fn diagonal_ur_ll(c: &mut Canvas, thick: u32) {
    let fw = c.w as f32;
    let fh = c.h as f32;
    let sx = (fw / fh).min(1.0);
    let sy = (fh / fw).min(1.0);
    c.line(
        fw + 0.5 * sx,
        -0.5 * sy,
        -0.5 * sx,
        fh + 0.5 * sy,
        thick as f32,
    );
}

fn diagonal_ul_lr(c: &mut Canvas, thick: u32) {
    let fw = c.w as f32;
    let fh = c.h as f32;
    let sx = (fw / fh).min(1.0);
    let sy = (fh / fw).min(1.0);
    c.line(
        -0.5 * sx,
        -0.5 * sy,
        fw + 0.5 * sx,
        fh + 0.5 * sy,
        thick as f32,
    );
}

fn block(atlas: &mut Atlas, cp: u32, cells: u8) -> Option<GlyphRect> {
    let (cw, ch) = atlas.cell();
    let slot_w = cw * u32::from(cells.max(1));
    let mut c = Canvas::new(slot_w, ch);
    let w = slot_w as f64;
    let h = ch as f64;
    let eighth = |n: f64| n / 8.0;
    let fill = |c: &mut Canvas, x0: f64, x1: f64, y0: f64, y1: f64, a: u8| {
        c.rect(
            frac_min(x0, c.w),
            frac_min(y0, c.h),
            frac_max(x1, c.w),
            frac_max(y1, c.h),
            a,
        );
    };
    let _ = (w, h); // fractions are in 0..1 space
    match cp {
        0x2580 => fill(&mut c, 0.0, 1.0, 0.0, 0.5, 0xFF),
        0x2581 => fill(&mut c, 0.0, 1.0, 1.0 - eighth(1.0), 1.0, 0xFF),
        0x2582 => fill(&mut c, 0.0, 1.0, 1.0 - eighth(2.0), 1.0, 0xFF),
        0x2583 => fill(&mut c, 0.0, 1.0, 1.0 - eighth(3.0), 1.0, 0xFF),
        0x2584 => fill(&mut c, 0.0, 1.0, 0.5, 1.0, 0xFF),
        0x2585 => fill(&mut c, 0.0, 1.0, 1.0 - eighth(5.0), 1.0, 0xFF),
        0x2586 => fill(&mut c, 0.0, 1.0, 1.0 - eighth(6.0), 1.0, 0xFF),
        0x2587 => fill(&mut c, 0.0, 1.0, 1.0 - eighth(7.0), 1.0, 0xFF),
        0x2588 => fill(&mut c, 0.0, 1.0, 0.0, 1.0, 0xFF),
        0x2589 => fill(&mut c, 0.0, eighth(7.0), 0.0, 1.0, 0xFF),
        0x258A => fill(&mut c, 0.0, eighth(6.0), 0.0, 1.0, 0xFF),
        0x258B => fill(&mut c, 0.0, eighth(5.0), 0.0, 1.0, 0xFF),
        0x258C => fill(&mut c, 0.0, 0.5, 0.0, 1.0, 0xFF),
        0x258D => fill(&mut c, 0.0, eighth(3.0), 0.0, 1.0, 0xFF),
        0x258E => fill(&mut c, 0.0, eighth(2.0), 0.0, 1.0, 0xFF),
        0x258F => fill(&mut c, 0.0, eighth(1.0), 0.0, 1.0, 0xFF),
        0x2590 => fill(&mut c, 0.5, 1.0, 0.0, 1.0, 0xFF),
        0x2591 => fill(&mut c, 0.0, 1.0, 0.0, 1.0, 0x40),
        0x2592 => fill(&mut c, 0.0, 1.0, 0.0, 1.0, 0x80),
        0x2593 => fill(&mut c, 0.0, 1.0, 0.0, 1.0, 0xC0),
        0x2594 => fill(&mut c, 0.0, 1.0, 0.0, eighth(1.0), 0xFF),
        0x2595 => fill(&mut c, 1.0 - eighth(1.0), 1.0, 0.0, 1.0, 0xFF),
        0x2596 => c.fill_fraction(0.0, 0.5, 0.5, 1.0, 0xFF),
        0x2597 => c.fill_fraction(0.5, 1.0, 0.5, 1.0, 0xFF),
        0x2598 => c.fill_fraction(0.0, 0.5, 0.0, 0.5, 0xFF),
        0x2599 => {
            c.fill_fraction(0.0, 0.5, 0.0, 0.5, 0xFF);
            c.fill_fraction(0.0, 0.5, 0.5, 1.0, 0xFF);
            c.fill_fraction(0.5, 1.0, 0.5, 1.0, 0xFF);
        }
        0x259A => {
            c.fill_fraction(0.0, 0.5, 0.0, 0.5, 0xFF);
            c.fill_fraction(0.5, 1.0, 0.5, 1.0, 0xFF);
        }
        0x259B => {
            c.fill_fraction(0.0, 0.5, 0.0, 0.5, 0xFF);
            c.fill_fraction(0.5, 1.0, 0.0, 0.5, 0xFF);
            c.fill_fraction(0.0, 0.5, 0.5, 1.0, 0xFF);
        }
        0x259C => {
            c.fill_fraction(0.0, 0.5, 0.0, 0.5, 0xFF);
            c.fill_fraction(0.5, 1.0, 0.0, 0.5, 0xFF);
            c.fill_fraction(0.5, 1.0, 0.5, 1.0, 0xFF);
        }
        0x259D => c.fill_fraction(0.5, 1.0, 0.0, 0.5, 0xFF),
        0x259E => {
            c.fill_fraction(0.5, 1.0, 0.0, 0.5, 0xFF);
            c.fill_fraction(0.0, 0.5, 0.5, 1.0, 0xFF);
        }
        0x259F => {
            c.fill_fraction(0.5, 1.0, 0.0, 0.5, 0xFF);
            c.fill_fraction(0.0, 0.5, 0.5, 1.0, 0xFF);
            c.fill_fraction(0.5, 1.0, 0.5, 1.0, 0xFF);
        }
        _ => return None,
    }
    commit(atlas, cells.max(1), c)
}

fn legacy(atlas: &mut Atlas, cp: u32, cells: u8) -> Option<GlyphRect> {
    let (cw, ch) = atlas.cell();
    let slot_w = cw * u32::from(cells.max(1));
    let mut c = Canvas::new(slot_w, ch);
    match cp {
        0x1FB00..=0x1FB3B => sextant(cp, &mut c),
        0x1CD00..=0x1CDE5 => octant(OCTANT_PATTERNS[(cp - 0x1CD00) as usize], &mut c),
        0x1CEA0 => octant(0b1000_0000, &mut c),
        0x1CEA3 => octant(0b0100_0000, &mut c),
        0x1CEA8 => octant(0b0000_0001, &mut c),
        0x1CEAB => octant(0b0000_0010, &mut c),
        0x1FBE6 => octant(0b0001_0100, &mut c),
        0x1FBE7 => octant(0b0010_1000, &mut c),
        _ => return None,
    }
    commit(atlas, cells.max(1), c)
}

fn sextant(cp: u32, c: &mut Canvas) {
    let idx = cp - 0x1FB00;
    let pattern = (idx + idx / 20 + 1) as u8;
    let third = 1.0 / 3.0;
    let two_thirds = 2.0 / 3.0;
    let cells = [
        (0x01u8, 0.0, 0.5, 0.0, third),
        (0x02, 0.5, 1.0, 0.0, third),
        (0x04, 0.0, 0.5, third, two_thirds),
        (0x08, 0.5, 1.0, third, two_thirds),
        (0x10, 0.0, 0.5, two_thirds, 1.0),
        (0x20, 0.5, 1.0, two_thirds, 1.0),
    ];
    for (mask, x0, x1, y0, y1) in cells {
        if pattern & mask != 0 {
            c.fill_fraction(x0, x1, y0, y1, 0xFF);
        }
    }
}

fn octant(pattern: u8, c: &mut Canvas) {
    let q = [0.0, 0.25, 0.5, 0.75, 1.0];
    for bit in 0..8u8 {
        if pattern & (1u8 << bit) != 0 {
            let row = (bit / 2) as usize;
            let (x0, x1) = if bit % 2 == 0 { (0.0, 0.5) } else { (0.5, 1.0) };
            c.fill_fraction(x0, x1, q[row], q[row + 1], 0xFF);
        }
    }
}

/// Unicode block octant patterns for U+1CD00..=U+1CDE5 (2 columns × 4 rows).
const OCTANT_PATTERNS: [u8; 230] = [
    0b00000100, 0b00000110, 0b00000111, 0b00001000, 0b00001001, 0b00001011, 0b00001100, 0b00001101,
    0b00001110, 0b00010000, 0b00010001, 0b00010010, 0b00010011, 0b00010101, 0b00010110, 0b00010111,
    0b00011000, 0b00011001, 0b00011010, 0b00011011, 0b00011100, 0b00011101, 0b00011110, 0b00011111,
    0b00100000, 0b00100001, 0b00100010, 0b00100011, 0b00100100, 0b00100101, 0b00100110, 0b00100111,
    0b00101001, 0b00101010, 0b00101011, 0b00101100, 0b00101101, 0b00101110, 0b00101111, 0b00110000,
    0b00110001, 0b00110010, 0b00110011, 0b00110100, 0b00110101, 0b00110110, 0b00110111, 0b00111000,
    0b00111001, 0b00111010, 0b00111011, 0b00111100, 0b00111101, 0b00111110, 0b01000001, 0b01000010,
    0b01000011, 0b01000100, 0b01000101, 0b01000110, 0b01000111, 0b01001000, 0b01001001, 0b01001010,
    0b01001011, 0b01001100, 0b01001101, 0b01001110, 0b01001111, 0b01010001, 0b01010010, 0b01010011,
    0b01010100, 0b01010110, 0b01010111, 0b01011000, 0b01011001, 0b01011011, 0b01011100, 0b01011101,
    0b01011110, 0b01100000, 0b01100001, 0b01100010, 0b01100011, 0b01100100, 0b01100101, 0b01100110,
    0b01100111, 0b01101000, 0b01101001, 0b01101010, 0b01101011, 0b01101100, 0b01101101, 0b01101110,
    0b01101111, 0b01110000, 0b01110001, 0b01110010, 0b01110011, 0b01110100, 0b01110101, 0b01110110,
    0b01110111, 0b01111000, 0b01111001, 0b01111010, 0b01111011, 0b01111100, 0b01111101, 0b01111110,
    0b01111111, 0b10000001, 0b10000010, 0b10000011, 0b10000100, 0b10000101, 0b10000110, 0b10000111,
    0b10001000, 0b10001001, 0b10001010, 0b10001011, 0b10001100, 0b10001101, 0b10001110, 0b10001111,
    0b10010000, 0b10010001, 0b10010010, 0b10010011, 0b10010100, 0b10010101, 0b10010110, 0b10010111,
    0b10011000, 0b10011001, 0b10011010, 0b10011011, 0b10011100, 0b10011101, 0b10011110, 0b10011111,
    0b10100001, 0b10100010, 0b10100011, 0b10100100, 0b10100110, 0b10100111, 0b10101000, 0b10101001,
    0b10101011, 0b10101100, 0b10101101, 0b10101110, 0b10110000, 0b10110001, 0b10110010, 0b10110011,
    0b10110100, 0b10110101, 0b10110110, 0b10110111, 0b10111000, 0b10111001, 0b10111010, 0b10111011,
    0b10111100, 0b10111101, 0b10111110, 0b10111111, 0b11000001, 0b11000010, 0b11000011, 0b11000100,
    0b11000101, 0b11000110, 0b11000111, 0b11001000, 0b11001001, 0b11001010, 0b11001011, 0b11001100,
    0b11001101, 0b11001110, 0b11001111, 0b11010000, 0b11010001, 0b11010010, 0b11010011, 0b11010100,
    0b11010101, 0b11010110, 0b11010111, 0b11011000, 0b11011001, 0b11011010, 0b11011011, 0b11011100,
    0b11011101, 0b11011110, 0b11011111, 0b11100000, 0b11100001, 0b11100010, 0b11100011, 0b11100100,
    0b11100101, 0b11100110, 0b11100111, 0b11101000, 0b11101001, 0b11101010, 0b11101011, 0b11101100,
    0b11101101, 0b11101110, 0b11101111, 0b11110001, 0b11110010, 0b11110011, 0b11110100, 0b11110110,
    0b11110111, 0b11111000, 0b11111001, 0b11111011, 0b11111101, 0b11111110,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn has_ink(atlas: &Atlas, rect: GlyphRect) -> bool {
        for y in 0..rect.h {
            for x in 0..rect.w {
                let i = ((rect.y + y) * crate::atlas::ATLAS_SIZE + rect.x + x) as usize;
                if atlas.pixels()[i] > 0 {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn box_drawing_rasterizes() {
        let mut atlas = Atlas::with_cell(10, 20);
        for cp in 0x2500u32..=0x257F {
            let ch = char::from_u32(cp).unwrap();
            let rect = raster(&mut atlas, ch, 1).unwrap_or_else(|| panic!("U+{cp:04X}"));
            assert!(has_ink(&atlas, rect), "U+{cp:04X} empty");
        }
    }

    #[test]
    fn braille_rasterizes() {
        let mut atlas = Atlas::with_cell(10, 20);
        assert!(raster(&mut atlas, '\u{2800}', 1).is_some());
        let full = raster(&mut atlas, '\u{28FF}', 1).expect("full braille");
        assert!(has_ink(&atlas, full));
    }

    #[test]
    fn legacy_computing_rasterizes() {
        let mut atlas = Atlas::with_cell(10, 20);
        for cp in (0x1FB00u32..=0x1FB3B)
            .chain(0x1CD00u32..=0x1CDE5)
            .chain([0x1CEA0, 0x1CEA3, 0x1CEA8, 0x1CEAB, 0x1FBE6, 0x1FBE7])
        {
            let ch = char::from_u32(cp).unwrap();
            let rect = raster(&mut atlas, ch, 1).unwrap_or_else(|| panic!("U+{cp:05X}"));
            assert!(has_ink(&atlas, rect), "U+{cp:05X} empty");
        }
    }

    #[test]
    fn block_elements_rasterize() {
        let mut atlas = Atlas::with_cell(10, 20);
        for cp in 0x2580u32..=0x259F {
            let ch = char::from_u32(cp).unwrap();
            let rect = raster(&mut atlas, ch, 1).unwrap_or_else(|| panic!("U+{cp:04X}"));
            assert!(has_ink(&atlas, rect), "U+{cp:04X} empty");
        }
    }
}
