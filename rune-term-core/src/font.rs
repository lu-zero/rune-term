/// Default terminal font size in points.
pub const DEFAULT_FONT_SIZE: f32 = 14.0;
pub const MIN_FONT_SIZE: f32 = 6.0;
pub const MAX_FONT_SIZE: f32 = 96.0;

pub fn clamp_font_size(size: f32) -> f32 {
    if !size.is_finite() {
        return DEFAULT_FONT_SIZE;
    }
    size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

/// Cell of Rio's Cascadia Code regular face at 14 points.
pub fn cell_points() -> (f32, f32) {
    term_grid::face_cell()
}

pub fn cell_points_for(font_size: f32) -> (f32, f32) {
    term_grid::face_cell_for(clamp_font_size(font_size))
}
