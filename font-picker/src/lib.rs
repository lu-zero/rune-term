//! A font family picker in rune-kit's shape: a [`Combobox`] over the families an app can draw,
//! which keeps the chosen family listed while the families are still being found.

use std::rc::Rc;

use rune::{El, Tx};
use rune_kit::{Combobox, ComboboxOption, ComboboxState, Kit};

type Access<S> = Rc<dyn for<'a> Fn(&'a mut S) -> &'a mut ComboboxState>;
type Choose<S> = Rc<dyn Fn(&mut S, &str, &mut Tx)>;

/// `FontPicker::new("font", &s.fonts, families, |s: &mut App| &mut s.fonts)`, where
/// `families` is `None` while the app is still finding them.
pub struct FontPicker<S> {
    key: String,
    state: ComboboxState,
    families: Option<Vec<String>>,
    access: Access<S>,
    current: String,
    notes: Vec<(String, String)>,
    width: f32,
    choose: Option<Choose<S>>,
}

impl<S: 'static> FontPicker<S> {
    /// Create a font picker for the current family.
    pub fn new(
        key: impl Into<String>,
        state: &ComboboxState,
        families: Option<&[String]>,
        access: impl for<'a> Fn(&'a mut S) -> &'a mut ComboboxState + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            state: state.clone(),
            families: families.map(<[String]>::to_vec),
            access: Rc::new(access),
            current: String::new(),
            notes: Vec::new(),
            width: 280.0,
            choose: None,
        }
    }

    /// The chosen family. It is listed even when `families` lacks it.
    pub fn current(mut self, family: &str) -> Self {
        self.current = family.to_owned();
        self
    }

    /// Text after `family`'s name in the list ("Bundled").
    pub fn note(mut self, family: &str, note: &str) -> Self {
        self.notes.push((family.to_owned(), note.to_owned()));
        self
    }

    /// The field's and the list's width, 280 by default.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Call `choose` with the family the user picks.
    pub fn on_choose(mut self, choose: impl Fn(&mut S, &str, &mut Tx) + 'static) -> Self {
        self.choose = Some(Rc::new(choose));
        self
    }

    /// Append the picker elements to `layer`.
    pub fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let listing = self.families.is_none();
        let mut families = self.families.unwrap_or_default();
        if !self.current.is_empty() && !families.contains(&self.current) {
            families.insert(0, self.current.clone());
        }
        let mut options: Vec<ComboboxOption> = families
            .into_iter()
            .map(|family| {
                let mut option = ComboboxOption::new(&family, &family);
                if let Some((_, note)) = self.notes.iter().find(|(f, _)| *f == family) {
                    option.detail = note.clone();
                }
                option
            })
            .collect();
        if listing {
            let mut row = ComboboxOption::new(format!("{}-listing", self.key), "Listing fonts…");
            row.disabled = true;
            options.push(row);
        }
        let access = self.access;
        let mut combobox =
            Combobox::new(&self.key, &self.state, &options, move |s: &mut S| access(s))
                .label("Font family")
                .current(Some(&self.current))
                .width(self.width)
                .search_placeholder("Search fonts")
                .empty_message("No font matches");
        if let Some(choose) = self.choose {
            combobox = combobox.on_choose(move |s: &mut S, family, tx| choose(s, family, tx));
        }
        combobox.build(k, layer)
    }
}
