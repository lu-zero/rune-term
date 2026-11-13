//! Reusable terminal runtime state.

use rune::Mods;

use crate::ime::Preedit;
use crate::render::GridDrawStamp;
use crate::surfaces::Surfaces;

#[derive(Default)]
pub struct State {
    pub surfaces: Surfaces,
    pub preedit: Option<Preedit>,
    pub cursor_cell: (u16, u16),
    pub blink_phase_on: bool,
    pub held_mods: Mods,
    pub grid_draw: GridDrawStamp,
}

impl State {
    pub fn new(theme_name: String) -> Self {
        Self {
            surfaces: Surfaces::new(),
            preedit: None,
            cursor_cell: (0, 0),
            blink_phase_on: true,
            held_mods: Mods::default(),
            grid_draw: GridDrawStamp {
                theme: theme_name,
                ..GridDrawStamp::default()
            },
        }
    }
}
