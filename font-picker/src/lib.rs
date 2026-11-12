//! A small font picker/list helper for Rune UIs.
//!
//! `rune-term` currently renders with a bundled/known font configuration, so
//! this crate focuses on selecting a font family name from available choices
//! (and can later be extended with system font enumeration).

use rune::{col, el, row, text, Color, El, ScrollSpec};

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

/// Render a simple vertical list of font choices.
///
/// `on_pick` is invoked with the chosen family name. Styling is intentionally
/// minimal so callers can embed it in their own settings chrome.
pub fn family_list<S, Pick>(families: &[String], selected: &str, on_pick: Pick) -> El
where
    S: 'static,
    Pick: Fn(&mut S, &str) + 'static + Clone,
{
    let mut list = col().w_full().gap(2.0);
    for family in families {
        let family_name = family.clone();
        let active = family == selected;
        let pick = on_pick.clone();
        let label = family.clone();
        list = list.child(
            el().w_full()
                .h(28.0)
                .items_center()
                .pl(8.0)
                .pr(8.0)
                .radius(4.0)
                .fill(if active {
                    Color {
                        r: 0.2,
                        g: 0.35,
                        b: 0.55,
                        a: 1.0,
                    }
                } else {
                    Color::TRANSPARENT
                })
                .on_press(move |s: &mut S| pick(s, &family_name))
                .child(text(label).color(Color::WHITE)),
        );
    }
    col()
        .w_full()
        .min_h(0.0)
        .grow(1.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(list)
}

/// A compact preview row showing sample text in the given style label context.
pub fn preview(family: &str) -> El {
    row()
        .w_full()
        .h(32.0)
        .items_center()
        .child(text(format!("{family} — AaBbCc 0123 λ")).color(Color::WHITE))
}
