use crate::settings;

/// Cell of Rio's Cascadia Code regular face at 14 points.
pub fn cell_points() -> (f32, f32) {
    term_grid::face_cell()
}

pub fn cell_points_for(font_size: f32) -> (f32, f32) {
    term_grid::face_cell_for(settings::clamp_font_size(font_size))
}
