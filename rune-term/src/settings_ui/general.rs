//! The general tab: what is the app's, not a profile's or a theme's. A change
//! is saved as it is made.

use rune::{col, el, row, text, El, ScrollSpec};
use rune_kit::{Kit, SegmentedControl, Switch};

use super::SettingsModel;
use crate::settings::{OnClose, Settings};

/// Build the General tab.
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
        .width(Some(204.0))
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

    let hint = Switch::new(settings.hint_fallback)
        .label("Hint fallback glyphs")
        .hide_label()
        .on_toggle(|m: &mut SettingsModel| {
            change(m, |settings| {
                settings.hint_fallback = !settings.hint_fallback
            });
        })
        .build(kit)
        .key("hint-fallback");

    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(24.0)
        .pt(16.0)
        .pb(24.0)
        .gap(24.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            col()
                .key("general-header")
                .gap(2.0)
                .child(text("General").font(kit.ty.ui_medium).color(kit.c.text_base))
                .child(
                    text("How the terminal closes tabs and handles input.")
                        .font(kit.ty.caption())
                        .color(kit.c.text_muted),
                ),
        )
        .child(section(
            kit,
            "general-closing",
            "Closing",
            vec![
                setting(
                    kit,
                    "Ask before closing a running program",
                    "Confirm when a close would end a program that is still running.",
                    confirm,
                ),
                setting(
                    kit,
                    "Closing a session tab",
                    "Kill the window, or hide it and leave the session running.",
                    on_close,
                ),
            ],
        ))
        .child(section(
            kit,
            "general-mouse",
            "Mouse",
            vec![setting(
                kit,
                "Send right-clicks to a program",
                "When the program reports the mouse, pass the right-click through instead of opening the menu.",
                report,
            )],
        ))
        .child(section(
            kit,
            "general-rendering",
            "Rendering",
            vec![setting(
                kit,
                "Hint fallback glyphs",
                "Turn off for a faster first render when your installed faces already look right.",
                hint,
            )],
        ));
    super::with_actions(None, Vec::new(), body)
}

fn section(kit: &Kit, key: &str, title: &str, rows: Vec<El>) -> El {
    let fill = if kit.dark {
        kit.light(0.035)
    } else {
        kit.dark_a(0.025)
    };
    let mut card = col()
        .key("card")
        .px(12.0)
        .py(6.0)
        .radius(kit.radius.md)
        .fill(fill)
        .border(1.0, kit.c.border_muted);
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            card = card.child(
                el().key(format!("separator-{index}"))
                    .h(1.0)
                    .ml(4.0)
                    .mr(4.0)
                    .fill(kit.c.border_muted)
                    .a11y_hidden(),
            );
        }
        card = card.child(row);
    }
    col()
        .key(key)
        .gap(8.0)
        .child(
            text(title.to_owned())
                .key("header")
                .font(kit.ty.ui_medium)
                .color(kit.c.text_muted)
                .pl(4.0),
        )
        .child(card)
}

fn setting(kit: &Kit, title: &str, description: &str, control: El) -> El {
    row()
        .w_full()
        .min_h(52.0)
        .py(10.0)
        .gap(16.0)
        .center_y()
        .child(
            col()
                .grow(1.0)
                .shrink()
                .gap(2.0)
                .child(
                    text(title.to_owned())
                        .font(kit.ty.ui_medium)
                        .color(kit.c.text_base),
                )
                .child(
                    text(description.to_owned())
                        .font(kit.ty.caption())
                        .color(kit.c.text_muted),
                ),
        )
        .child(control)
}

fn change(model: &mut SettingsModel, edit: impl FnOnce(&mut Settings)) {
    edit(&mut model.config.settings);
    if super::write_config(model) {
        super::push(model);
    }
}
