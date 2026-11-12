//! A small font picker/list helper for Rune UIs.
//!
//! `rune-term` currently renders with a bundled/known font configuration, so
//! this crate focuses on selecting a font family name from available choices.

use rune::{col, el, text, Color, El, Key, KeyEvent, ScrollSpec, TextStyle, Tx};

/// A simple in-memory font source.
#[derive(Clone, Debug, Default)]
pub struct FontCatalog {
    pub families: Vec<String>,
}

impl FontCatalog {
    pub fn new(mut families: Vec<String>) -> Self {
        families.sort();
        families.dedup();
        Self { families }
    }

    /// Built-in families known to rune-term today.
    pub fn builtin() -> Self {
        Self::new(vec![
            "Cascadia Code NF".to_owned(),
            "Monaco".to_owned(),
            "Menlo".to_owned(),
            "SF Mono".to_owned(),
        ])
    }

    /// Enumerate system font families (when available), with common slot names
    /// prepended. Falls back to [`Self::builtin`] on unsupported platforms or if
    /// enumeration yields nothing.
    pub fn system() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::sync::OnceLock;
            static CATALOG: OnceLock<FontCatalog> = OnceLock::new();
            CATALOG
                .get_or_init(|| {
                    use fontique::{Collection, CollectionOptions};
                    let mut collection = Collection::new(CollectionOptions {
                        shared: false,
                        system_fonts: true,
                    });
                    let mut families: Vec<String> =
                        collection.family_names().map(str::to_owned).collect();
                    families.sort();
                    families.dedup();
                    if !families.is_empty() {
                        let mut all = vec![
                            "System Mono (platform)".to_owned(),
                            "Mono (app slot)".to_owned(),
                            "Sans (app slot)".to_owned(),
                        ];
                        all.extend(families);
                        Self::new(all)
                    } else {
                        let mut catalog = Self::builtin();
                        catalog
                            .families
                            .insert(0, "System Mono (platform)".to_owned());
                        catalog
                    }
                })
                .clone()
        }
        #[cfg(target_arch = "wasm32")]
        {
            let mut catalog = Self::builtin();
            catalog
                .families
                .insert(0, "System Mono (platform)".to_owned());
            catalog
        }
    }

    pub fn filter(&self, query: &str) -> Vec<String> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.families.clone();
        }
        self.families
            .iter()
            .filter(|f| f.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }
}

fn family_for(name: &str) -> rune::tok::Family {
    match name {
        "System Mono (platform)" => rune::tok::Family::SystemMono,
        "Mono (app slot)" => rune::tok::Family::Mono,
        "Sans (app slot)" => rune::tok::Family::Sans,
        _ => {
            let name: &'static str = Box::leak(name.to_owned().into_boxed_str());
            rune::tok::Family::Named(name)
        }
    }
}

/// Render a list of font choices where each row previews its font.
///
/// `active` is the highlighted item. Click or Enter confirms (`on_pick`),
/// arrow keys move the highlight (`on_active`).
pub fn family_list<S, Pick, Active>(
    families: &[String],
    active: &str,
    on_pick: Pick,
    on_active: Active,
) -> El
where
    S: 'static,
    Pick: Fn(&mut S, &str) + 'static + Clone,
    Active: Fn(&mut S, &str) + 'static + Clone,
{
    let mut items = col().w_full().gap(2.0);
    for family in families {
        let family_name = family.clone();
        let is_active = family == active;
        let pick = on_pick.clone();
        let activate = on_active.clone();
        let label = family.clone();
        let style =
            TextStyle::new(14.0, if is_active { 500.0 } else { 400.0 }).family(family_for(&label));
        let row = el()
            .key(format!("font-item-{}", label.replace(' ', "_")))
            .role("option")
            .w_full()
            .h(32.0)
            .items_center()
            .pl(10.0)
            .pr(10.0)
            .radius(4.0)
            .fill(if is_active {
                Color {
                    r: 0.2,
                    g: 0.35,
                    b: 0.55,
                    a: 1.0,
                }
            } else {
                Color::TRANSPARENT
            })
            .hover(if is_active {
                Color {
                    r: 0.2,
                    g: 0.35,
                    b: 0.55,
                    a: 1.0,
                }
            } else {
                Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 0.06,
                }
            })
            .on_hover(move |s: &mut S| activate(s, &family_name))
            .on_press({
                let pick = pick.clone();
                let label = label.clone();
                move |s: &mut S| pick(s, &label)
            })
            .child(
                text(format!("{label}  —  AaBbCc 0123 λ"))
                    .font(style)
                    .color(Color::WHITE)
                    .truncate(),
            );
        items = items.child(row);
    }

    let families_nav = families.to_vec();
    let active_nav = active.to_owned();
    let on_pick_nav = on_pick;
    let on_active_nav = on_active;
    col()
        .key("font-list")
        .role("listbox")
        .w_full()
        .min_h(0.0)
        .grow(1.0)
        .focusable(true)
        .on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut Tx| {
            if families_nav.is_empty() {
                return false;
            }
            let current = families_nav
                .iter()
                .position(|f| f == &active_nav)
                .unwrap_or(0);
            match ev.key {
                Key::Down => {
                    let next = (current + 1).min(families_nav.len() - 1);
                    on_active_nav(s, &families_nav[next]);
                    true
                }
                Key::Up => {
                    let next = current.saturating_sub(1);
                    on_active_nav(s, &families_nav[next]);
                    true
                }
                Key::Home => {
                    on_active_nav(s, &families_nav[0]);
                    true
                }
                Key::End => {
                    on_active_nav(s, &families_nav[families_nav.len() - 1]);
                    true
                }
                Key::PageDown => {
                    let next = (current + 10).min(families_nav.len() - 1);
                    on_active_nav(s, &families_nav[next]);
                    true
                }
                Key::PageUp => {
                    let next = current.saturating_sub(10);
                    on_active_nav(s, &families_nav[next]);
                    true
                }
                Key::Enter => {
                    on_pick_nav(s, &families_nav[current]);
                    true
                }
                _ => false,
            }
        })
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(items)
}

/// A compact preview row showing sample text in the given style label context.
pub fn preview(family: &str) -> El {
    let style = TextStyle::new(14.0, 400.0).family(family_for(family));
    text(format!("{family} — AaBbCc 0123 λ"))
        .font(style)
        .color(Color::WHITE)
}
