use font_picker::{family_list, FontCatalog};
use rune::{col, el, row, text, App, AppSpec, Color, Cx, El, TextStyle, Titlebar};

const TITLEBAR_H: f32 = 38.0;

#[derive(Clone, Debug)]
struct State {
    catalog: FontCatalog,
    selected: String,
}

fn font_preview(family: &str) -> El {
    let family_static: &'static str = Box::leak(family.to_owned().into_boxed_str());
    let style = TextStyle::new(16.0, 400.0).family(rune::tok::Family::Named(family_static));
    col()
        .w_full()
        .gap(4.0)
        .child(text(family).font(rune::tok::type_::LABEL).color(Color {
            r: 0.75,
            g: 0.75,
            b: 0.78,
            a: 1.0,
        }))
        .child(
            text("The quick brown fox jumps over 0123456789 — λ")
                .font(style)
                .color(Color::WHITE),
        )
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let insets = cx.chrome_insets();
    let h = if insets.height > 0.0 {
        insets.height
    } else {
        TITLEBAR_H
    };
    let leading = insets.leading.max(cx.titlebar_inset());
    let trailing = insets.trailing;

    let header = row()
        .key("titlebar")
        .w_full()
        .h(h)
        .items_center()
        .pl(leading + 12.0)
        .pr(trailing + 12.0)
        .window_drag()
        .fill(Color {
            r: 0.10,
            g: 0.10,
            b: 0.11,
            a: 1.0,
        })
        .child(
            text("Font Picker Demo")
                .font(rune::tok::type_::TITLE)
                .color(Color::WHITE),
        );

    let content = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(10.0)
        .pt(16.0)
        .pb(20.0)
        .pl(20.0)
        .pr(20.0)
        .child(font_preview(&s.selected))
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
        .child(text(format!("Selected: {}", s.selected)).color(Color::WHITE));

    col()
        .w_full()
        .h_full()
        .fill(Color {
            r: 0.08,
            g: 0.08,
            b: 0.08,
            a: 1.0,
        })
        .child(header)
        .child(content)
}

fn main() {
    App::new(AppSpec::new(
        "font-picker-demo",
        (420.0, 380.0),
        || State {
            catalog: FontCatalog::builtin(),
            selected: "Menlo".to_owned(),
        },
        view,
    ))
    .titlebar(Titlebar::new(TITLEBAR_H))
    .run();
}
