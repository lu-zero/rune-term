use font_picker::FontPicker;
use rune::{col, text, App, AppSpec, Cx, El, Tx};
use rune_kit::{ComboboxState, Kit};

struct State {
    picker: ComboboxState,
    family: String,
    listed: bool,
}

fn view(s: &State, _cx: &mut Cx<State>) -> El {
    let kit = Kit::neutral(true);
    let mut layer = Vec::new();
    let picker = FontPicker::new(
        "font",
        &s.picker,
        term_grid::monospace_families(),
        |s: &mut State| &mut s.picker,
    )
    .current(&s.family)
    .note(term_grid::BUNDLED_FAMILY, "Bundled")
    .note(term_grid::MAPLE_FAMILY, "Bundled")
    .on_choose(|s: &mut State, family, _tx| s.family = family.to_owned())
    .build(&kit, &mut layer);
    let body = col()
        .w_full()
        .h_full()
        .padding(20.0)
        .gap(12.0)
        .fill(kit.c.bg_base)
        .child(picker)
        .child(text(format!("Chosen: {}", s.family)).color(kit.c.text_base));
    rune_kit::root(body, layer)
}

/// Draws again once the families are found.
fn poll(s: &mut State, _t: f64, _tx: &mut Tx) -> bool {
    let listed = term_grid::monospace_families().is_some();
    std::mem::replace(&mut s.listed, listed) != listed
}

fn init() -> State {
    State {
        picker: ComboboxState::default(),
        family: term_grid::BUNDLED_FAMILY.to_owned(),
        listed: false,
    }
}

fn main() {
    App::new(AppSpec {
        poll: Some(poll),
        ..AppSpec::new("font-picker-demo", (420.0, 480.0), init, view)
    })
    .run();
}
