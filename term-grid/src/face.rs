//! The font file the grid draws with: the bundled Cascadia Code NF, or an installed family.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use fontique::{Blob, Collection, CollectionOptions, FontStyle, FontWeight, FontWidth};
use swash::FontRef;

/// The family the grid draws with when a profile names none, or one that is not installed.
pub const BUNDLED_FAMILY: &str = "Cascadia Code NF";

/// Same bytes Sugarloaf embeds. Vendored so a clone builds without the Rio tree.
pub(crate) const BUNDLED: &[u8] = include_bytes!("../CascadiaCodeNF.ttf");

#[derive(Clone, Debug)]
pub struct Face {
    family: Arc<str>,
    data: Blob<u8>,
    index: u32,
}

impl PartialEq for Face {
    fn eq(&self, other: &Self) -> bool {
        self.data.id() == other.data.id() && self.index == other.index
    }
}

impl Face {
    pub fn bundled() -> Self {
        static DATA: OnceLock<Blob<u8>> = OnceLock::new();
        Self {
            family: BUNDLED_FAMILY.into(),
            data: DATA.get_or_init(|| Blob::new(Arc::new(BUNDLED))).clone(),
            index: 0,
        }
    }

    /// The regular face of `family`, or the bundled one when it is not installed. The bundled
    /// family stays bundled even when installed, so the default draws the same everywhere.
    pub fn load(family: &str) -> Self {
        let family = family.trim();
        if family.is_empty() || family == BUNDLED_FAMILY {
            return Self::bundled();
        }
        static LOADED: Mutex<Option<HashMap<String, Face>>> = Mutex::new(None);
        let mut loaded = LOADED.lock().expect("faces");
        let loaded = loaded.get_or_insert_with(HashMap::new);
        if let Some(face) = loaded.get(family) {
            return face.clone();
        }
        let face = installed(family).unwrap_or_else(Self::bundled);
        loaded.insert(family.to_owned(), face.clone());
        face
    }

    /// The family this face is, which for a family that is not installed is the bundled one.
    pub fn family(&self) -> &str {
        &self.family
    }

    pub(crate) fn font(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(self.data.data(), self.index as usize)
    }
}

fn installed(family: &str) -> Option<Face> {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let info = collection.family_by_name(family)?;
    let font = info.match_font(
        FontWidth::NORMAL,
        FontStyle::Normal,
        FontWeight::NORMAL,
        false,
    )?;
    let face = Face {
        family: info.name().into(),
        data: font.load(None)?,
        index: font.index(),
    };
    face.font().is_some().then_some(face)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_family_that_is_not_installed_draws_with_the_bundled_face() {
        let face = Face::load("rune-term no such family");
        assert_eq!(face, Face::bundled());
        assert_eq!(face.family(), BUNDLED_FAMILY);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_installed_family_loads_its_own_file() {
        let face = Face::load("Menlo");
        assert_ne!(face, Face::bundled());
        assert_eq!(face.family(), "Menlo");
    }
}
