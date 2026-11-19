//! The general tab: what is the app's, not a profile's or a theme's. A change
//! is saved as it is made.

use rune::{col, row, El, ScrollSpec};
use rune_kit::{Field, Kit, SegmentedControl, Switch};

use super::profiles::half;
use super::SettingsModel;
use crate::settings::{OnClose, Settings};

pub fn editor(kit: &Kit, model: &SettingsModel) -> El {
    let settings = &model.config.settings;
    let confirm = Switch::new(settings.confirm_close)
        .label("Ask before closing a running program")
        .hide_label()
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
    let on_close = SegmentedControl::new(&["Kill", "Detach"], Some(at))
        .label("Closing a session tab")
        .width(None)
        .on_pick(|m: &mut SettingsModel, at, _tx| {
            change(m, |settings| {
                settings.on_close = [OnClose::Kill, OnClose::Detach][at]
            });
        })
        .build(kit);
    let report = Switch::new(settings.report_right_click)
        .label("Send right-clicks to a program using the mouse")
        .hide_label()
        .on_toggle(|m: &mut SettingsModel| {
            change(m, |settings| {
                settings.report_right_click = !settings.report_right_click
            });
        })
        .build(kit)
        .key("report-right-click");
    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(20.0)
        .pt(8.0)
        .pb(16.0)
        .gap(14.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            row()
                .w_full()
                .gap(12.0)
                .child(half(
                    Field::new()
                        .label("Ask before closing a running program")
                        .control(row().h(28.0).center_y().child(confirm))
                        .build(kit),
                ))
                .child(half(
                    Field::new()
                        .label("Closing a session tab")
                        .control(on_close)
                        .build(kit),
                )),
        )
        .child(
            row()
                .w_full()
                .gap(12.0)
                .child(half(
                    Field::new()
                        .label("Send right-clicks to a program using the mouse")
                        .control(row().h(28.0).center_y().child(report))
                        .build(kit),
                ))
                .child(half(rune::el())),
        );
    super::with_actions(Vec::new(), body)
}

fn change(model: &mut SettingsModel, edit: impl FnOnce(&mut Settings)) {
    edit(&mut model.config.settings);
    if super::write_config(model) {
        super::push(model);
    }
}
