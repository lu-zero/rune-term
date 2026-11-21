//! The font files the grid draws with: bundled Maple Mono NF or Maple Mono NF CN, or an installed family.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use fontique::{Blob, Collection, CollectionOptions, FontStyle, FontWeight, FontWidth};
use swash::FontRef;

/// The family the grid draws with when a profile names none, or one that is not installed.
pub const BUNDLED_FAMILY: &str = "Maple Mono NF";

/// The bundled face's family, kept as a name for callers that ask for Maple.
pub const MAPLE_FAMILY: &str = BUNDLED_FAMILY;

/// The other bundled face, kept so the two designs can be compared.
pub const CASCADIA_FAMILY: &str = "Cascadia Code NF";

/// The static regular face from the v8.0-beta.3 Nerd Font release of the
/// variable branch. The grid draws one instance, so a variable file would
/// only add bytes for axes it never sets.
pub(crate) const BUNDLED: &[u8] = include_bytes!("../MapleMonoNF.ttf");
pub(crate) const CASCADIA: &[u8] = include_bytes!("../CascadiaCodeNF.ttf");

/// A compact subset of Maple Mono NF CN for codepoints the profile's face misses.
pub(crate) const FAILOVER: &[u8] = include_bytes!("../CjkFallback.ttf");
pub(crate) const FAILOVER_FAMILY: &str = "rune-term CJK Fallback";

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
        Self::bundled_named(BUNDLED_FAMILY, BUNDLED, &DATA)
    }

    /// The bundled Maple Mono NF face, with the same Nerd Font glyphs and wider language coverage.
    pub fn bundled_maple() -> Self {
        Self::bundled()
    }

    /// The bundled Cascadia Code NF face, selectable instead of the default.
    pub fn bundled_cascadia() -> Self {
        static DATA: OnceLock<Blob<u8>> = OnceLock::new();
        Self::bundled_named(CASCADIA_FAMILY, CASCADIA, &DATA)
    }

    /// The compact face used for glyphs the profile's face does not cover.
    pub(crate) fn failover() -> Self {
        static DATA: OnceLock<Blob<u8>> = OnceLock::new();
        Self::bundled_named(FAILOVER_FAMILY, FAILOVER, &DATA)
    }

    fn bundled_named(
        family: &'static str,
        bytes: &'static [u8],
        cache: &'static OnceLock<Blob<u8>>,
    ) -> Self {
        Self {
            family: family.into(),
            data: cache.get_or_init(|| Blob::new(Arc::new(bytes))).clone(),
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
        if family == CASCADIA_FAMILY {
            return Self::bundled_cascadia();
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
        .filter(|name| {
            !name.starts_with('.') && *name != BUNDLED_FAMILY && *name != CASCADIA_FAMILY
        })
        .filter(|name| installed_in(&mut collection, name).is_some_and(|face| face.monospaced()))
        .collect();
    families.sort_by_key(|name| name.to_lowercase());
    families.insert(0, CASCADIA_FAMILY.to_owned());
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
    fn bundled_families_load_their_own_bytes() {
        assert_eq!(Face::load(MAPLE_FAMILY), Face::bundled());
        let cascadia = Face::load(CASCADIA_FAMILY);
        assert_eq!(cascadia, Face::bundled_cascadia());
        assert_ne!(cascadia, Face::bundled());
        assert_eq!(cascadia.family(), CASCADIA_FAMILY);
        assert!(cascadia.monospaced());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_list_is_the_bundled_families_then_the_monospaced_ones() {
        let families = scan();
        assert_eq!(&families[..2], [BUNDLED_FAMILY, CASCADIA_FAMILY]);
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
