use font_picker::{family_list, FontCatalog};
use rune::{col, el, row, text, App, AppSpec, Color, Cx, El, Titlebar};

const TITLEBAR_H: f32 = 38.0;

#[derive(Clone, Debug)]
struct State {
    catalog: FontCatalog,
    selected: String,
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
        .child(
            text("↑/↓, PgUp/PgDn, Home/End to navigate (click list to focus)")
                .font(rune::tok::type_::LABEL)
                .color(Color {
                    r: 0.65,
                    g: 0.65,
                    b: 0.68,
                    a: 1.0,
                }),
        )
        .child(
            el().w_full()
                .grow(1.0)
                .min_h(120.0)
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

fn system_catalog() -> FontCatalog {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use fontique::{Collection, CollectionOptions};
        let mut collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: true,
        });
        let mut families: Vec<String> = collection.family_names().map(str::to_owned).collect();
        families.sort();
        families.dedup();
        if !families.is_empty() {
            let mut all = vec![
                "System Mono (platform)".to_owned(),
                "Mono (app slot)".to_owned(),
                "Sans (app slot)".to_owned(),
            ];
            all.extend(families);
            return FontCatalog::new(all);
        }
    }
    let mut catalog = FontCatalog::builtin();
    catalog
        .families
        .insert(0, "System Mono (platform)".to_owned());
    catalog
}

fn init() -> State {
    let catalog = system_catalog();
    let selected = if catalog.families.iter().any(|f| f == "Menlo") {
        "Menlo".to_owned()
    } else if catalog.families.iter().any(|f| f == "Monaco") {
        "Monaco".to_owned()
    } else {
        catalog
            .families
            .first()
            .cloned()
            .unwrap_or_else(|| "System Mono (platform)".to_owned())
    };
    State { catalog, selected }
}

fn main() {
    App::new(AppSpec::new("font-picker-demo", (520.0, 560.0), init, view))
        .titlebar(Titlebar::new(TITLEBAR_H))
        .run();
}
