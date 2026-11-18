//! The menu bar: Settings in the app menu, and a Session menu that acts on the selected tab.
//! A menu bar item has no submenu and no id of its own, so the sessions to attach stay in
//! the pane's menu.

use rune::{MenuItem, Tx};

use crate::model::Model;

/// What the Session menu greys out, from the selected tab.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shape {
    grouped: bool,
    window: bool,
    lost: bool,
}

pub(crate) fn shape(model: &Model) -> Shape {
    let index = model.active;
    let group = model.tabs.get(index).and_then(|tab| tab.group);
    let lost = group.is_some_and(|id| {
        model
            .tab_groups
            .iter()
            .any(|group| group.id == id && group.lost)
    });
    Shape {
        grouped: group.is_some(),
        window: !lost && crate::mux::window_of(model, index).is_some(),
        lost,
    }
}

/// Puts the menu that fits the selected tab in the menu bar, when it changed.
pub(crate) fn sync(model: &mut Model) {
    let shape = shape(model);
    if model.menu_shape == Some(shape) {
        return;
    }
    let Some(handle) = &model.terminal_handle else {
        return;
    };
    if handle.set_menu(items(shape)).is_ok() {
        model.menu_shape = Some(shape);
    }
}

fn items(shape: Shape) -> Vec<MenuItem<Model>> {
    let session =
        |title: &str, action: fn(&mut Model, &mut Tx)| MenuItem::new(title, action).menu("Session");
    vec![
        MenuItem::new("Settings…", |model, _| crate::settings_ui::open(model))
            .key_equivalent("cmd+,".parse().expect("a shortcut")),
        session("New Session", |model, _| new(model, "tmux-new")),
        session("New Named Session…", |model, _| new(model, "tmux-named")),
        session("Connect to Host…", |model, _| new(model, "tmux-host")),
        MenuItem::separator().menu("Session"),
        session("Hide Window", |model, tx| {
            crate::tabs::close(model, model.active, tx)
        })
        .enabled(shape.window),
        session("Close Window", |model, tx| {
            crate::tabs::kill_window(model, model.active, tx)
        })
        .enabled(shape.window),
        session("Reconnect", |model, _| {
            crate::mux::reconnect(model, model.active)
        })
        .enabled(shape.lost),
        MenuItem::separator().menu("Session"),
        session("Detach Session", |model, tx| {
            crate::mux::detach(model, model.active, tx)
        })
        .enabled(shape.grouped),
        session("Save as Profile…", |model, _| {
            crate::mux::ask_profile_name(model, model.active)
        })
        .enabled(shape.grouped),
    ]
}

fn new(model: &mut Model, id: &str) {
    crate::mux::menu_action(model, id);
}
