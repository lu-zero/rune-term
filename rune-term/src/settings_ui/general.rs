//! The general tab: what is the app's, not a profile's or a theme's. A change
//! is saved as it is made.

use rune::{col, text, El, ScrollSpec};
use rune_kit::{Field, Kit, SegmentedControl, Switch};

use super::SettingsModel;
use crate::settings::{OnClose, Settings};

pub fn editor(kit: &Kit, model: &SettingsModel) -> El {
    let settings = &model.config.settings;
    let confirm = Switch::new(settings.confirm_close)
        .label("Ask before closing a running program")
        .description("A shell at its prompt closes without asking.")
        .on_toggle(|m: &mut SettingsModel| {
            change(m, |settings| {
                settings.confirm_close = !settings.confirm_close
            });
        })
        .build(kit)
        .key("confirm-close");
    let at = match settings.on_close {
        OnClose::Kill => 0,
        OnClose::Detach => 1,
    };
    let on_close = Field::new()
        .label("Closing a session tab")
        .control(
            SegmentedControl::new(&["Kill", "Detach"], Some(at))
                .label("Closing a session tab")
                .width(None)
                .on_pick(|m: &mut SettingsModel, at, _tx| {
                    change(m, |settings| {
                        settings.on_close = [OnClose::Kill, OnClose::Detach][at]
                    });
                })
                .build(kit),
        )
        .build(kit);
    let on_close = col().w_full().gap(6.0).child(on_close).child(
        text("Kill ends the tmux or rmux window; Detach hides it, still running. A session profile can choose otherwise.")
            .font(kit.ty.small)
            .color(kit.c.text_muted)
            .wrap()
            .w_full(),
    );
    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(20.0)
        .pt(8.0)
        .pb(16.0)
        .gap(18.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(confirm)
        .child(on_close);
    super::with_actions(Vec::new(), body)
}

fn change(model: &mut SettingsModel, edit: impl FnOnce(&mut Settings)) {
    edit(&mut model.config.settings);
    if super::write_config(model) {
        super::push(model);
    }
}
