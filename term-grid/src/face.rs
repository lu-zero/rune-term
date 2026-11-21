//! The font files the grid draws with: bundled Cascadia Code NF or Maple Mono NF, or an installed family.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use fontique::{Blob, Collection, CollectionOptions, FontStyle, FontWeight, FontWidth};
use swash::FontRef;

/// The family the grid draws with when a profile names none, or one that is not installed.
pub const BUNDLED_FAMILY: &str = "Cascadia Code NF";

/// The other bundled family, a Nerd Font with wider language coverage.
pub const MAPLE_FAMILY: &str = "Maple Mono NF";

/// Same bytes Sugarloaf embeds. Vendored so a clone builds without the Rio tree.
pub(crate) const BUNDLED: &[u8] = include_bytes!("../CascadiaCodeNF.ttf");

/// The regular Maple Mono NF face, from the font's v7.9 Nerd Font release.
pub(crate) const MAPLE: &[u8] = include_bytes!("../MapleMonoNF.ttf");

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

    /// The bundled Maple Mono NF face, with the same Nerd Font glyphs and wider language coverage.
    pub fn bundled_maple() -> Self {
        static DATA: OnceLock<Blob<u8>> = OnceLock::new();
        Self {
            family: MAPLE_FAMILY.into(),
            data: DATA.get_or_init(|| Blob::new(Arc::new(MAPLE))).clone(),
            index: 0,
        }
    }

    /// The regular face of `family`, or the bundled one when it is not installed. Bundled
    /// families stay bundled even when installed, so those profiles draw the same everywhere.
    pub fn load(family: &str) -> Self {
        let family = family.trim();
        if family.is_empty() || family == BUNDLED_FAMILY {
            return Self::bundled();
        }
        if family == MAPLE_FAMILY {
            return Self::bundled_maple();
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

    /// Whether `i`, `M` and `0` advance alike. The `post` table's fixed-pitch flag is not
    /// trusted: some monospaced families leave it unset.
    fn monospaced(&self) -> bool {
        let Some(font) = self.font() else {
            return false;
        };
        let (charmap, metrics) = (font.charmap(), font.glyph_metrics(&[]));
        let glyphs = ['i', 'M', '0'].map(|ch| charmap.map(ch));
        if glyphs.contains(&0) {
            return false;
        }
        let [i, m, zero] = glyphs.map(|glyph| metrics.advance_width(glyph));
        i > 0.0 && i == m && m == zero
    }
}

/// The bundled families, then the installed families whose regular face is monospaced, sorted.
/// None until the scan the first call starts in the background has finished.
pub fn monospace_families() -> Option<&'static [String]> {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    static STARTED: AtomicBool = AtomicBool::new(false);
    if !STARTED.swap(true, Ordering::AcqRel) {
        thread::spawn(|| FAMILIES.get_or_init(scan));
    }
    FAMILIES.get().map(Vec::as_slice)
}

fn scan() -> Vec<String> {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let names: Vec<String> = collection.family_names().map(str::to_owned).collect();
    let mut families: Vec<String> = names
        .into_iter()
        // A dot names a face the system keeps for itself.
        .filter(|name| !name.starts_with('.') && *name != BUNDLED_FAMILY && *name != MAPLE_FAMILY)
        .filter(|name| installed_in(&mut collection, name).is_some_and(|face| face.monospaced()))
        .collect();
    families.sort_by_key(|name| name.to_lowercase());
    families.insert(0, MAPLE_FAMILY.to_owned());
    families.insert(0, BUNDLED_FAMILY.to_owned());
    families
}

fn installed(family: &str) -> Option<Face> {
    installed_in(
        &mut Collection::new(CollectionOptions {
            shared: false,
            system_fonts: true,
        }),
        family,
    )
}

fn installed_in(collection: &mut Collection, family: &str) -> Option<Face> {
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

    #[test]
    fn maple_loads_its_own_bundled_face() {
        let face = Face::load(MAPLE_FAMILY);
        assert_eq!(face, Face::bundled_maple());
        assert_ne!(face, Face::bundled());
        assert_eq!(face.family(), MAPLE_FAMILY);
        assert!(face.monospaced());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_list_is_the_bundled_families_then_the_monospaced_ones() {
        let families = scan();
        assert_eq!(&families[..2], [BUNDLED_FAMILY, MAPLE_FAMILY]);
        assert!(families.iter().any(|f| f == "Menlo"), "{families:?}");
        assert!(!families.iter().any(|f| f == "Helvetica"), "{families:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_installed_family_loads_its_own_file() {
        let face = Face::load("Menlo");
        assert_ne!(face, Face::bundled());
        assert_eq!(face.family(), "Menlo");
    }
}
