//! One tab strip. The panes are [`crate::chrome_view`]. The cell tree is [`crate::split`].

use std::hash::{DefaultHasher, Hash, Hasher};

use std::ops::Range;
use std::rc::Rc;

use rune::{col, el, row, text, Color, Cx, Drag, El, Phase, PopupItem, PopupMenu, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, TextInput};

use crate::chrome_view;
use crate::model::Model;
use crate::tabs::StripDrag;
use crate::theme::{self, Palette};

pub const PANE_MARK: u32 = 1;
pub const LEAF: LeafId = LeafId(1);

pub use crate::split::{Leaf, LeafId, Node};

/// The window titlebar height, and the strip that lives in it.
pub const TITLEBAR_H: f32 = 38.0;
/// One hit target. A short title and a long one use this same box.
const TAB_W: f32 = 132.0;
/// Between the strip's items.
const STRIP_GAP: f32 = 6.0;
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
        .gap(STRIP_GAP)
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
        let mut layer = Vec::new();
        name_session(name, &crate::settings_ui::kit(p), &mut layer);
        root = root.children(layer);
    }
    root
}

/// Return or Create starts the session; Escape, Cancel or a click outside drops the name.
fn name_session(name: &TextEdit, k: &Kit, layer: &mut Vec<El>) {
    let field = TextInput::new(name)
        .placeholder("Session name")
        .label("Session name")
        .width(None)
        .on_edit(|model: &mut Model| model.session_name.as_mut().expect("the prompt is open"))
        .on_submit(|model: &mut Model, _tx| crate::mux::submit_name(model))
        .build(k)
        .key("session-name");
    let buttons = row()
        .w_full()
        .gap(8.0)
        .justify_end()
        .child(
            Button::new("Cancel")
                .on_press(|model: &mut Model| model.session_name = None)
                .build(k)
                .key("cancel-name-session"),
        )
        .child(
            Button::new("Create")
                .variant(ButtonVariant::Submit)
                .on_press(crate::mux::submit_name)
                .build(k)
                .key("create-session"),
        );
    Dialog::new("name-session", "New tmux session")
        .description("If a session has this name already, it is attached.")
        .width(360.0)
        .fit()
        .open(true)
        .on_close(|model: &mut Model| model.session_name = None)
        .child(
            col()
                .w_full()
                .gap(16.0)
                .pl(16.0)
                .pr(16.0)
                .pb(16.0)
                .child(field)
                .child(buttons),
        )
        .build(k, layer);
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
    let pitch = TAB_W + STRIP_GAP;
    let blocks = crate::tabs::blocks(&model.tabs);
    let widths: Rc<[f32]> = blocks
        .iter()
        .map(|block| block_width(model, block, pitch, cx))
        .collect();
    let drag = model.strip_drag.as_ref();
    let moved = drag
        .filter(|drag| drag.group.is_none())
        .and_then(|drag| widths.get(drag.from))
        .copied()
        .unwrap_or(0.0);
    for (at, block) in blocks.iter().enumerate() {
        let index = block.start;
        let tab_group = model.tabs[index].group;
        let group = tab_group.and_then(|id| group_of(model, id));
        let block_dx = slide(drag, None, at, moved);
        let block_dragged = dragged(drag, None, at);
        let lane = Rc::clone(&widths);
        let on_block = move |model: &mut Model, drag: &mut Drag, tx: &mut Tx| {
            drag_strip(model, None, at, &lane, drag, tx);
        };
        if let Some(group) = group {
            let label = group_label(group, block.len(), tab_h, p).on_drag(on_block.clone());
            strip = strip.child(follow(label, block_dx, block_dragged));
        }
        let inside: Rc<[f32]> = vec![pitch; block.len()].into();
        for index in block.clone() {
            let active = index == model.active;
            if group.is_some_and(|group| group.collapsed) && !active {
                continue;
            }
            let at_in = index - block.start;
            let tab = tab_el(model, index, tab_h, p);
            let tab = match tab_group {
                // A collapsed group shows one tab, which has nowhere to go.
                Some(id) if group.is_some_and(|group| group.collapsed) => {
                    let tab =
                        tab.on_drag(move |model: &mut Model, drag: &mut Drag, tx: &mut Tx| {
                            drag_strip(model, Some(id), 0, &[pitch], drag, tx);
                        });
                    follow(tab, block_dx, block_dragged)
                }
                Some(id) => {
                    let lane = Rc::clone(&inside);
                    let dx = block_dx + slide(drag, Some(id), at_in, pitch);
                    let held = block_dragged || dragged(drag, Some(id), at_in);
                    let tab =
                        tab.on_drag(move |model: &mut Model, drag: &mut Drag, tx: &mut Tx| {
                            drag_strip(model, Some(id), at_in, &lane, drag, tx);
                        });
                    follow(tab, dx, held)
                }
                None => follow(tab.on_drag(on_block.clone()), block_dx, block_dragged),
            };
            strip = strip.child(tab);
        }
    }
    strip.child(new_tab_button(tab_h, p))
}

fn tab_el(model: &Model, index: usize, tab_h: f32, p: &Palette) -> El {
    let tab_group = model.tabs[index].group;
    let active = index == model.active;
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
            if std::mem::take(&mut model.strip_dropped) {
                return;
            }
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
    tab.child(label).child(close)
}

/// Points a block takes in the strip, its trailing gap included.
fn block_width(model: &Model, block: &Range<usize>, pitch: f32, cx: &mut Cx<Model>) -> f32 {
    let Some(group) = model.tabs[block.start]
        .group
        .and_then(|id| group_of(model, id))
    else {
        return pitch;
    };
    let label = cx
        .layout_size(&format!("tab-group-{}", group.label))
        .map_or(0.0, |size| size[0] + STRIP_GAP);
    let shown = if group.collapsed {
        usize::from(block.contains(&model.active))
    } else {
        block.len()
    };
    label + shown as f32 * pitch
}

/// How far item `me` of the drag's lane slides: the dragged one with the pointer, the ones
/// it passed by its width the other way.
fn slide(drag: Option<&StripDrag>, group: Option<TabGroupId>, me: usize, moved: f32) -> f32 {
    let Some(drag) = drag.filter(|drag| drag.group == group) else {
        return 0.0;
    };
    if me == drag.from {
        drag.dx
    } else if drag.from < me && me <= drag.to {
        -moved
    } else if drag.to <= me && me < drag.from {
        moved
    } else {
        0.0
    }
}

fn dragged(drag: Option<&StripDrag>, group: Option<TabGroupId>, me: usize) -> bool {
    drag.is_some_and(|drag| drag.group == group && drag.from == me)
}

/// The dragged item follows the pointer at once, above the rest; the others spring aside.
fn follow(el: El, dx: f32, dragged: bool) -> El {
    let el = el.offset(dx, 0.0);
    if dragged {
        el.raise().offset_motion(rune::Anim::INSTANT)
    } else {
        el
    }
}

/// A press and release without travel stays a click: only a move starts the drag.
fn drag_strip(
    model: &mut Model,
    group: Option<TabGroupId>,
    from: usize,
    widths: &[f32],
    drag: &mut Drag,
    tx: &mut Tx,
) {
    match drag.phase {
        Phase::Down => {
            model.strip_dropped = false;
            tx.live_only();
        }
        Phase::Move => {
            // Held inside its lane, as a group's tab cannot leave the group.
            let before: f32 = widths[..from].iter().sum();
            let after: f32 = widths[from + 1..].iter().sum();
            let dx = drag.dx.clamp(-before, after);
            model.strip_drag = Some(StripDrag {
                group,
                from,
                to: crate::tabs::landing(widths, from, dx),
                dx,
            });
        }
        Phase::Up => {
            let Some(done) = model.strip_drag.take() else {
                return;
            };
            model.strip_dropped = true;
            if drag.cancelled() {
                return;
            }
            match done.group {
                None => crate::tabs::move_block(model, done.from, done.to),
                Some(group) => crate::tabs::move_in_group(model, group, done.from, done.to),
            }
            crate::tabs::focus_active(model, tx);
        }
    }
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
            if std::mem::take(&mut model.strip_dropped) {
                return;
            }
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

    fn order(rt: &Runtime<Model>) -> Vec<librio::SurfaceId> {
        rt.state
            .tabs
            .iter()
            .map(|tab| crate::tabs::surface_of(tab).unwrap())
            .collect()
    }

    fn drag(rt: &mut Runtime<Model>, key: &str, dx: f32) {
        let (x, y) = rt.find(key).expect(key);
        rt.pointer_down(x, y);
        for step in 1..=4 {
            rt.pointer_move(x + dx * step as f32 / 4.0, y);
            rt.frame(0.0);
        }
        rt.pointer_up(x + dx, y);
        rt.frame(0.0);
    }

    #[test]
    fn a_dropped_tab_lands_past_the_neighbours_it_half_covered() {
        assert_eq!(crate::tabs::landing(&[100.0, 40.0, 100.0], 0, 30.0), 1);
        assert_eq!(crate::tabs::landing(&[100.0, 40.0, 100.0], 0, 19.0), 0);
        assert_eq!(crate::tabs::landing(&[100.0, 40.0, 100.0], 0, 500.0), 2);
        assert_eq!(crate::tabs::landing(&[100.0, 40.0, 100.0], 2, -71.0), 1);
    }

    #[test]
    fn dragging_a_tab_reorders_the_strip_and_a_click_still_selects() {
        let mut model = host();
        crate::tabs::open(&mut model);
        crate::tabs::open(&mut model);
        let mut rt = Runtime::new(model, view, (900.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let [a, b, c] = order(&rt)[..] else {
            panic!("three tabs")
        };
        drag(&mut rt, "tab-0", (TAB_W + STRIP_GAP) * 1.2);
        assert_eq!(order(&rt), [b, a, c]);
        assert_eq!(rt.state.active, 2, "the selected tab stays selected");
        let (x, y) = rt.find("tab-0").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.active, 0);
        assert_eq!(order(&rt), [b, a, c]);
    }

    #[test]
    fn a_group_moves_as_one_and_its_tabs_stay_inside_it() {
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
        let mut rt = Runtime::new(model, view, (900.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        rt.frame(0.0);
        let [a, b, c] = order(&rt)[..] else {
            panic!("three tabs")
        };
        let pitch = TAB_W + STRIP_GAP;
        drag(&mut rt, "tab-2", -pitch * 4.0);
        assert_eq!(
            order(&rt),
            [a, c, b],
            "the tab stops at its group's first place"
        );
        drag(&mut rt, "tab-group-dev", -pitch);
        assert_eq!(order(&rt), [c, b, a], "the label takes its tabs along");
        assert_eq!(rt.state.tabs[0].group, Some(group));
        assert_eq!(rt.state.tabs[1].group, Some(group));
    }

    #[test]
    fn the_name_prompt_takes_the_typed_name_and_cancel_drops_it() {
        let mut model = host();
        crate::mux::ask_name(&mut model);
        let mut rt = Runtime::new(model, view, (800.0, 500.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("session-name").is_some(), "the field is drawn");
        rt.key(rune::KeyEvent {
            key: rune::Key::Text("work".into()),
            mods: rune::Mods::default(),
        });
        rt.frame(0.0);
        assert_eq!(rt.state.session_name.as_ref().unwrap().text, "work");
        let (x, y) = rt.find("cancel-name-session").expect("the cancel button");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(rt.state.session_name.is_none());
        assert!(rt.state.muxes.is_empty(), "cancel starts nothing");
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
