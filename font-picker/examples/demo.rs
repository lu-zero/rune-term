use font_picker::{family_list, preview, FontCatalog};
use rune::{col, el, text, App, AppSpec, Color, Cx, El};

#[derive(Clone, Debug)]
struct State {
    catalog: FontCatalog,
    selected: String,
}

fn view(s: &State, _cx: &mut Cx<State>) -> El {
    col()
        .w_full()
        .h_full()
        .gap(10.0)
        .padding(20.0)
        .fill(Color {
            r: 0.08,
            g: 0.08,
            b: 0.08,
            a: 1.0,
        })
        .child(text("font-picker demo").color(Color::WHITE))
        .child(preview(&s.selected))
        .child(
            el().w_full()
                .h(220.0)
                .radius(6.0)
                .border(1.0, Color::WHITE.alpha(0.2))
                .child(family_list::<State, _>(
                    &s.catalog.families,
                    &s.selected,
                    |s: &mut State, family: &str| s.selected = family.to_owned(),
                )),
        )
        .child(text(format!("Selected: {}", s.selected)).color(Color::WHITE))
}

fn main() {
    App::new(AppSpec::new(
        "font-picker-demo",
        (420.0, 360.0),
        || State {
            catalog: FontCatalog::builtin(),
            selected: "Cascadia Code NF".to_owned(),
        },
        view,
    ))
    .run();
}
