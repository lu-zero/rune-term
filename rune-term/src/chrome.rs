//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

use std::hash::{DefaultHasher, Hash, Hasher};

use rune::{
    col, el, row, text, text_input, Color, Cx, El, PopupItem, PopupMenu, TextEdit, TextStyle, Tx,
};

use crate::chrome_view;
use crate::model::Model;
use crate::theme::{self, Palette};

pub const PANE_MARK: u32 = 1;
pub const LEAF: LeafId = LeafId(1);

pub use crate::split::{Leaf, LeafId, Node};

/// The window titlebar height, and the strip that lives in it.
pub const TITLEBAR_H: f32 = 38.0;
/// One hit target. A short title and a long one use this same box.
const TAB_W: f32 = 132.0;
pub(crate) const TAB_PAD_X: f32 = 14.0;
pub(crate) const TAB_RADIUS: f32 = 7.0;

/// The title band's height: the window's own when it has one, else [`TITLEBAR_H`].
pub(crate) fn band_height(insets_height: f32) -> f32 {
    if insets_height > 0.0 {
        insets_height
    } else {
        TITLEBAR_H
    }
}

/// A tab's height inside a band of `band` points.
pub(crate) fn tab_box_height(band: f32) -> f32 {
    (band - 10.0).max(22.0)
}

/// The band across the top of a window. Native controls sit over `leading` and
/// `trailing`, and the empty part of the band drags the window.
pub(crate) fn title_strip(band: f32, leading: f32, trailing: f32, fill: Color) -> El {
    row()
        .w_full()
        .h(band)
        .fill(fill)
        .center_y()
        .pl(leading + 10.0)
        .pr(trailing + 10.0)
        .gap(6.0)
        .window_drag()
}

/// Tab strip. Dark chrome above the black pane.
pub fn tab(p: &Palette) -> rune::Color {
    theme::linear(p.tab_bg)
}

/// Cell background. Transparent cells show this.
pub fn slot(p: &Palette) -> rune::Color {
    theme::linear(p.bg)
}

pub fn text_color(p: &Palette) -> rune::Color {
    theme::linear(p.fg)
}

pub fn slot_premul(bg: u32, srgb_blend: bool) -> [f32; 4] {
    let slot = theme::linear(bg);
    if srgb_blend {
        slot.premul_srgb(1.0)
    } else {
        slot.premul(1.0)
    }
}

/// Identifies a group of tabs (e.g. one attached tmux session).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TabGroupId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabGroup {
    pub id: TabGroupId,
    pub label: String,
    /// Only the label shows, and the group's selected tab.
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub title: String,
    pub profile: String,
    pub root: Node,
    pub focused: LeafId,
    /// Group this tab belongs to, if any.
    pub group: Option<TabGroupId>,
    /// The multiplexer window this tab shows. Its group's backend owns the layout.
    pub window: Option<u32>,
}

pub fn view(model: &Model, cx: &mut Cx<Model>) -> El {
    let p = model.palette.as_ref();
    let tab = tab_strip(model, cx);
    let pane = chrome_view::area(model, cx);
    // Rune delivers Cmd+V to the root handler, not to the focused pane.
    let mut root = col()
        .w_full()
        .h_full()
        .fill(theme::linear(p.tab_bg))
        .on_paste(crate::selection::on_paste)
        .child(tab)
        .child(pane);
    if let Some(uri) = &model.pending_link_open {
        root = root.child(confirm_open_link(uri, p));
    }
    if let Some(name) = &model.session_name {
        root = root.child(name_session(name, p));
    }
    root
}

fn name_session(name: &TextEdit, p: &Palette) -> El {
    let field = text_input(name, TextStyle::new(13.0, 400.0))
        .key("session-name")
        .single_line()
        .h(28.0)
        .w_full()
        .fill(theme::linear(p.bg))
        .color(theme::linear(p.fg))
        .on_edit(|model: &mut Model| model.session_name.as_mut().expect("the prompt is open"))
        .on_submit(|model: &mut Model, _tx| crate::mux::submit_name(model));
    let card = col()
        .key("name-session")
        .w(320.0)
        .fill(theme::linear(p.tab_bg))
        .border(1.0, theme::linear(p.active_border))
        .radius(8.0)
        .gap(12.0)
        .pl(16.0)
        .pr(16.0)
        .pt(14.0)
        .pb(14.0)
        .modal(true, |model: &mut Model| {
            model.session_name = None;
        })
        .child(text("New tmux session").color(text_color(p)))
        .child(field)
        .child(
            row()
                .w_full()
                .gap(8.0)
                .justify_center()
                .child(
                    el().key("cancel-name-session")
                        .pl(12.0)
                        .pr(12.0)
                        .pt(6.0)
                        .pb(6.0)
                        .radius(6.0)
                        .border(1.0, theme::linear(p.active_border))
                        .focusable(true)
                        .on_press_tx(|model: &mut Model, _tx| {
                            model.session_name = None;
                        })
                        .child(text("Cancel").color(theme::linear(p.muted))),
                )
                .child(
                    el().key("create-session")
                        .pl(12.0)
                        .pr(12.0)
                        .pt(6.0)
                        .pb(6.0)
                        .radius(6.0)
                        .fill(theme::linear(p.accent))
                        .focusable(true)
                        .on_press_tx(|model: &mut Model, _tx| crate::mux::submit_name(model))
                        .child(text("Create").color(text_color(p))),
                ),
        );
    rune::el()
        .key("name-session-overlay")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .child(card)
}

fn confirm_open_link(uri: &str, p: &Palette) -> El {
    let shown = if uri.len() > 80 {
        format!("{}…", &uri[..uri.floor_char_boundary(79)])
    } else {
        uri.to_string()
    };
    let to_open = uri.to_string();
    let card = col()
        .key("confirm-open-link")
        .w(420.0)
        .fill(theme::linear(p.tab_bg))
        .border(1.0, theme::linear(p.active_border))
        .radius(8.0)
        .gap(12.0)
        .pl(16.0)
        .pr(16.0)
        .pt(14.0)
        .pb(14.0)
        .modal(true, |model: &mut Model| {
            model.pending_link_open = None;
        })
        .child(text("Open external link?").color(text_color(p)))
        .child(text(shown).color(theme::linear(p.muted)))
        .child(
            row()
                .w_full()
                .gap(8.0)
                .justify_center()
                .child(
                    el().key("cancel-open-link")
                        .pl(12.0)
                        .pr(12.0)
                        .pt(6.0)
                        .pb(6.0)
                        .radius(6.0)
                        .border(1.0, theme::linear(p.active_border))
                        .focusable(true)
                        .on_press_tx(|model: &mut Model, _tx| {
                            model.pending_link_open = None;
                        })
                        .child(text("Cancel").color(theme::linear(p.muted))),
                )
                .child(
                    el().key("open-link")
                        .pl(12.0)
                        .pr(12.0)
                        .pt(6.0)
                        .pb(6.0)
                        .radius(6.0)
                        .fill(theme::linear(p.accent))
                        .focusable(true)
                        .on_press_tx(move |model: &mut Model, _tx| {
                            let _ = crate::hyperlink::open_uri_with(&to_open, |_| true);
                            model.pending_link_open = None;
                        })
                        .child(text("Open").color(text_color(p))),
                ),
        );
    rune::el()
        .key("confirm-open-link-overlay")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .child(card)
}

fn tab_strip(model: &Model, cx: &mut Cx<Model>) -> El {
    let p = model.palette.as_ref();
    let insets = cx.chrome_insets();
    let h = band_height(insets.height);
    let leading = insets.leading.max(cx.titlebar_inset());
    let tab_h = tab_box_height(h);
    let mut strip = title_strip(h, leading, insets.trailing, theme::linear(p.tab_bg))
        .key("tabs")
        .scroll_x();
    if model.tabs.is_empty() {
        return strip.child(new_tab_button(tab_h, p));
    }
    for index in 0..model.tabs.len() {
        let tab_group = model.tabs[index].group;
        let starts_group =
            tab_group.is_some() && (index == 0 || model.tabs[index - 1].group != tab_group);
        let group = tab_group.and_then(|id| group_of(model, id));
        if let Some(group) = group.filter(|_| starts_group) {
            let count = model
                .tabs
                .iter()
                .filter(|tab| tab.group == Some(group.id))
                .count();
            strip = strip.child(group_label(group, count, tab_h, p));
        }

        let active = index == model.active;
        if group.is_some_and(|group| group.collapsed) && !active {
            continue;
        }
        let hue = tab_group
            .and_then(|id| group_of(model, id))
            .map(|group| group_color(&group.label, p));
        let color = if active {
            text_color(p)
        } else if let Some(hue) = hue {
            hue
        } else {
            theme::linear(p.muted)
        };
        let mut tab = row()
            .key(format!("tab-{index}"))
            .h(tab_h)
            .w(TAB_W)
            .pl(TAB_PAD_X)
            .pr(4.0)
            .gap(6.0)
            .center_y()
            .radius(TAB_RADIUS)
            .preserve_focus_on_press(true)
            .on_press_tx(move |model: &mut Model, tx| {
                if crate::tabs::select(model, index) {
                    crate::tabs::focus_active(model, tx);
                } else {
                    tx.live_only();
                }
            })
            .on_secondary_press_local(move |model: &mut Model, point, tx| {
                tab_menu(model, index, point.window(), tx);
            });
        if active {
            tab = tab.fill(theme::linear(p.bg)).border(
                1.0,
                if let Some(hue) = hue {
                    hue
                } else {
                    theme::linear(p.active_border)
                },
            );
        } else if let Some(hue) = hue {
            tab = tab.fill(wash(hue)).border(1.0, hue);
        }
        let label = text(tab_caption(model, index))
            .grow(1.0)
            .min_w(0.0)
            .color(color)
            .truncate();
        let close = row()
            .key(format!("tab-close-{index}"))
            .w(tab_h - 8.0)
            .h(tab_h - 8.0)
            .center()
            .radius(TAB_RADIUS)
            .hover(hover_overlay(p))
            .preserve_focus_on_press(true)
            .on_press_tx(move |model: &mut Model, tx| {
                crate::tabs::close(model, index, tx);
                crate::tabs::focus_active(model, tx);
            })
            .child(text("×").color(theme::linear(p.muted)));
        strip = strip.child(tab.child(label).child(close));
    }
    strip.child(new_tab_button(tab_h, p))
}

/// A press collapses the group to this label, or opens it again.
fn group_label(group: &TabGroup, count: usize, tab_h: f32, p: &Palette) -> El {
    let id = group.id;
    let label = group.label.as_str();
    let caption = if group.collapsed {
        format!("{label} | {count}")
    } else {
        label.to_owned()
    };
    row()
        .key(format!("tab-group-{label}"))
        .preserve_focus_on_press(true)
        .on_press_tx(move |model: &mut Model, tx| {
            if let Some(group) = model.tab_groups.iter_mut().find(|group| group.id == id) {
                group.collapsed = !group.collapsed;
            }
            crate::tabs::focus_active(model, tx);
        })
        .h(tab_h)
        .pl(10.0)
        .pr(10.0)
        .center_y()
        .radius(TAB_RADIUS)
        .fill(wash(group_color(label, p)))
        .border(1.0, group_color(label, p))
        .child(text(caption).color(group_color(label, p)).truncate())
}

/// The group's color behind its label and tabs, faint enough for the text on it.
fn wash(mut hue: Color) -> Color {
    hue.a = 0.12;
    hue
}

/// A hue from the group's name, so a session keeps its color across runs. A light
/// background takes a darker value to keep the text readable.
fn group_color(label: &str, p: &Palette) -> Color {
    let mut hasher = DefaultHasher::new();
    label.hash(&mut hasher);
    let bits = hasher.finish();
    let h = f64::from((bits >> 32) as u32 ^ bits as u32) / f64::from(u32::MAX);
    let [r, g, b, _] = theme::channels(p.bg);
    let light = 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5;
    let v = if light { 0.6 } else { 0.9 };
    let rgb = okhsl::Okhsv { h, s: 0.7, v }.to_srgb();
    theme::linear(u32::from(rgb.r) << 16 | u32::from(rgb.g) << 8 | u32::from(rgb.b))
}

fn new_tab_button(tab_h: f32, p: &Palette) -> El {
    let size = tab_h - 8.0;
    row()
        .key("tab-new")
        .w(size)
        .h(size)
        .center()
        .radius(TAB_RADIUS)
        .hover(hover_overlay(p))
        .preserve_focus_on_press(true)
        .on_press_tx(move |model: &mut Model, tx| {
            crate::tabs::open(model);
            crate::tabs::reveal(model, tx);
        })
        .child(text("+").color(theme::linear(p.muted)))
}

/// A tmux tab tells hiding its window, killing it, and leaving the session apart.
fn tab_menu(model: &Model, index: usize, at: rune::PointerPress, tx: &mut Tx) {
    let menu = if crate::mux::window_of(model, index).is_some() {
        PopupMenu::new()
            .item(PopupItem::new("hide", "Hide Window"))
            .item(PopupItem::new("kill", "Close Window"))
            .separator()
            .item(PopupItem::new("detach", "Detach Session"))
    } else {
        PopupMenu::new().item(PopupItem::new("close", "Close Tab"))
    };
    tx.popup_menu(
        menu,
        at,
        move |model: &mut Model, picked, tx| match picked {
            Some("hide" | "close") => crate::tabs::close(model, index, tx),
            Some("kill") => crate::mux::kill_window(model, index),
            Some("detach") => crate::mux::detach(model, index, tx),
            _ => {}
        },
    );
}

pub(crate) fn hover_overlay(p: &Palette) -> Color {
    let mut c = theme::linear(p.fg);
    c.a = 0.10;
    c
}

fn group_of(model: &Model, id: TabGroupId) -> Option<&TabGroup> {
    model.tab_groups.iter().find(|g| g.id == id)
}

fn tab_caption(model: &Model, index: usize) -> String {
    let Some(tab) = model.tabs.get(index) else {
        return "rune-term".to_string();
    };
    let bell = crate::tabs::surface_of(tab)
        .and_then(|id| model.terminal.get(id))
        .is_some_and(|host| host.bell);
    if bell {
        format!("{} •", tab.title)
    } else {
        tab.title.clone()
    }
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn a_group_color_stands_out_on_every_builtin_theme() {
        let luminance = |c: Color| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
        for palette in theme::builtins() {
            let bg = luminance(theme::linear(palette.bg));
            for name in [
                "0",
                "1",
                "work",
                "tmux",
                "rune-term",
                "ssh",
                "logs",
                "build",
            ] {
                let fg = luminance(group_color(name, palette));
                let ratio = (fg.max(bg) + 0.05) / (fg.min(bg) + 0.05);
                assert!(ratio >= 3.0, "{name} on {}: {ratio:.2}", palette.name);
            }
        }
    }

    fn tab_h() -> f32 {
        tab_box_height(TITLEBAR_H)
    }

    fn host() -> Model {
        let mut model = crate::model::init();
        crate::delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    fn rect(rt: &Runtime<Model>, key: &str) -> [f32; 4] {
        rt.presented(key)
            .unwrap_or_else(|| panic!("missing {key}"))
            .0
    }

    #[test]
    fn a_tab_stays_wide_enough_to_hit() {
        let mut model = host();
        model.tabs[0].title = "x".into();
        crate::tabs::open(&mut model);
        model.tabs[1].title = "a-very-long-shell-title-that-would-otherwise-eat-the-strip".into();
        for _ in 0..8 {
            crate::tabs::open(&mut model);
        }
        let mut rt = Runtime::new(model, view, (800.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        for index in 0..10 {
            let tab = rect(&rt, &format!("tab-{index}"));
            assert!((tab[2] - TAB_W).abs() < 0.5, "tab {index} width {}", tab[2]);
            assert!(
                (tab[3] - tab_h()).abs() < 0.5,
                "tab {index} height {}",
                tab[3]
            );
        }
    }

    #[test]
    fn a_group_labels_its_tabs() {
        let mut model = host();
        let group = TabGroupId(1);
        model.tab_groups.push(TabGroup {
            id: group,
            label: "dev".to_owned(),
            collapsed: false,
        });
        crate::tabs::open(&mut model);
        model.tabs[1].group = Some(group);
        let mut rt = Runtime::new(model, view, (800.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let label = rect(&rt, "tab-group-dev");
        let tab = rect(&rt, "tab-1");
        assert!(label[0] < tab[0], "the group label comes first");
    }

    #[test]
    fn a_pressed_group_collapses_to_its_label_and_selected_tab() {
        let mut model = host();
        let group = TabGroupId(1);
        model.tab_groups.push(TabGroup {
            id: group,
            label: "dev".to_owned(),
            collapsed: false,
        });
        crate::tabs::open(&mut model);
        crate::tabs::open(&mut model);
        model.tabs[1].group = Some(group);
        model.tabs[2].group = Some(group);
        assert_eq!(model.active, 2);
        let mut rt = Runtime::new(model, view, (800.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let press = |rt: &mut Runtime<Model>| {
            let (x, y) = rt.find("tab-group-dev").expect("the label");
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(0.0);
        };
        press(&mut rt);
        assert!(rt.state.tab_groups[0].collapsed);
        assert!(rt.find("tab-0").is_some(), "a tab outside the group stays");
        assert!(
            rt.find("tab-1").is_none(),
            "the group's other tab folds away"
        );
        assert!(
            rt.find("tab-2").is_some(),
            "the selected tab stays on screen"
        );
        press(&mut rt);
        assert!(rt.find("tab-1").is_some());
    }

    #[test]
    fn the_edge_of_a_tab_selects_it() {
        let mut model = host();
        crate::tabs::open(&mut model);
        let mut rt = Runtime::new(model, view, (800.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert_eq!(rt.state.active, 1);
        let tab = rect(&rt, "tab-0");
        let x = tab[0] + 2.0;
        let y = tab[1] + tab[3] - 2.0;
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.active, 0);
    }
}
