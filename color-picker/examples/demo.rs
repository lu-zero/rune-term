use color_picker::{hex6, hsv_editor, linear_rgb, Hsv};
use rune::{col, el, row, text, App, AppSpec, Color, Cx, El};

#[derive(Clone, Copy, Debug)]
struct State {
    hsv: Hsv,
}

fn view(s: &State, cx: &mut Cx<State>) -> El {
    let rgb = s.hsv.to_rgb_u32();
    let insets = cx.chrome_insets();
    let leading = insets.leading.max(cx.titlebar_inset());
    col()
        .w_full()
        .h_full()
        .gap(12.0)
        .pt(20.0)
        .pb(20.0)
        .pl(20.0 + leading)
        .pr(20.0 + insets.trailing)
        .fill(Color {
            r: 0.08,
            g: 0.08,
            b: 0.08,
            a: 1.0,
        })
        .child(text("color-picker demo").color(Color::WHITE))
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
        ))
}

fn main() {
    App::new(AppSpec::new(
        "color-picker-demo",
        (420.0, 260.0),
        || State {
            hsv: Hsv::from_rgb_u32(0x33_66_99),
        },
        view,
    ))
    .run();
}
