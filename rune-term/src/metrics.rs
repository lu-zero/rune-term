//! Terminal cell metrics.

pub use rune_term_core::font::{cell_points, cell_points_for, clamp_font_size};

use crate::model::Model;
use crate::settings::Resolved;

/// Takes `resolved`'s face and size into the model. True when the cell changed.
pub fn apply_font(model: &mut Model, resolved: &Resolved) -> bool {
    let face = term_grid::Face::load(&resolved.font_family);
    if face == model.face && (resolved.font_size - model.font_size).abs() <= f32::EPSILON {
        return false;
    }
    model.face = face;
    model.font_size = resolved.font_size;
    model.cell_points = cell_points_for(&model.face, model.font_size);
    true
}
