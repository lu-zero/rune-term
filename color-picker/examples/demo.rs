use color_picker::{hex6, linear_rgb, rgb_sliders, Style};
use rune::{col, el, row, text, App, AppSpec, Color, Cx, El, Titlebar};

const TITLEBAR_H: f32 = 38.0;

#[derive(Clone, Copy, Debug)]
struct State {
    rgb: u32,
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
    let ink = Color::WHITE.alpha(0.2);
    let style = Style {
        track: Color {
            r: 0.16,
            g: 0.16,
            b: 0.17,
            a: 1.0,
        },
        track_border: ink,
        thumb: Color::WHITE,
        thumb_border: ink,
        label: Color::WHITE.alpha(0.7),
    };

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
                        .fill(linear_rgb(s.rgb))
                        .border(1.0, ink),
                )
                .child(text(hex6(s.rgb)).color(Color::WHITE)),
        )
        .child(rgb_sliders::<State, _>(
            "demo",
            s.rgb,
            |s: &mut State, rgb| {
                s.rgb = rgb;
            },
            style,
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
        (420.0, 280.0),
        || State { rgb: 0x33_66_99 },
        view,
    ))
    .titlebar(Titlebar::new(TITLEBAR_H))
    .run();
}
