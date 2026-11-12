use color_picker::{hex6, hsv_editor, linear_rgb, Hsv};
use rune::{col, el, row, text, App, AppSpec, Color, Cx, El, Titlebar};

const TITLEBAR_H: f32 = 38.0;

#[derive(Clone, Copy, Debug)]
struct State {
    hsv: Hsv,
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let rgb = s.hsv.to_rgb_u32();
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
            text("Color Picker Demo")
                .font(rune::tok::type_::TITLE)
                .color(Color::WHITE),
        );

    let content = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(12.0)
        .pt(16.0)
        .pb(20.0)
        .pl(20.0)
        .pr(20.0)
        .child(
            row()
                .gap(12.0)
                .items_center()
                .child(
                    el().w(48.0)
                        .h(48.0)
                        .radius(6.0)
                        .fill(linear_rgb(rgb))
                        .border(1.0, Color::WHITE.alpha(0.2)),
                )
                .child(text(format!("{}  RRGGBB={:06X}", hex6(rgb), rgb)).color(Color::WHITE)),
        )
        .child(hsv_editor::<State, _>(
            "demo",
            s.hsv,
            |s: &mut State, hsv| {
                s.hsv = hsv;
            },
        ));

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
        "color-picker-demo",
        (420.0, 300.0),
        || State {
            hsv: Hsv::from_rgb_u32(0x33_66_99),
        },
        view,
    ))
    .titlebar(Titlebar::new(TITLEBAR_H))
    .run();
}
