//! The font files the grid draws with: bundled Maple Mono NF or Maple Mono NF CN, or an installed family.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use fontique::{Blob, Collection, CollectionOptions, FontStyle, FontWeight, FontWidth};
use swash::{FontRef, StringId};

/// The family the grid draws with when a profile names none, or one that is not installed.
pub const BUNDLED_FAMILY: &str = "Maple Mono NF";

/// The bundled face's family, kept as a name for callers that ask for Maple.
pub const MAPLE_FAMILY: &str = BUNDLED_FAMILY;

/// The other selectable face, when the `cascadia` feature is on.
#[cfg(feature = "cascadia")]
pub const CASCADIA_FAMILY: &str = "Cascadia Code NF";

/// The v8.0-beta.3 Nerd Font regulars, zstd-compressed in the data crates.
#[cfg(feature = "bundled-font")]
pub(crate) const BUNDLED_ZSTD: &[u8] = term_font_data_maple::ZSTD;
#[cfg(feature = "cascadia")]
pub(crate) const CASCADIA_ZSTD: &[u8] = term_font_data_cascadia::ZSTD;

/// Compact fallback faces for codepoints the profile's face misses.
#[cfg(feature = "fallback-common")]
pub(crate) const FAILOVER_ZSTD: &[u8] = term_font_data_common::ZSTD;
#[cfg(feature = "fallback-ja")]
pub(crate) const FAILOVER_JA_ZSTD: &[u8] = term_font_data_ja::ZSTD;
#[cfg(feature = "fallback-zh-full")]
pub(crate) const FAILOVER_ZH_FULL_ZSTD: &[u8] = term_font_data_zh_full::ZSTD;

/// The default face's bytes, decoded once for CoreText's descriptor.
#[cfg(all(feature = "bundled-font", target_os = "macos"))]
pub(crate) fn bundled_data() -> &'static [u8] {
    static DATA: OnceLock<Vec<u8>> = OnceLock::new();
    DATA.get_or_init(|| term_font::data::decompress(BUNDLED_ZSTD))
}

/// The embedded fallback faces, smallest tier first, decoded once. Empty in
/// a build with no fallback features: callers then rely on system fonts alone.
pub fn fallback_bytes() -> &'static [Blob<u8>] {
    static FACES: OnceLock<Vec<Blob<u8>>> = OnceLock::new();
    FACES.get_or_init(|| {
        [
            #[cfg(feature = "fallback-common")]
            FAILOVER_ZSTD,
            #[cfg(feature = "fallback-ja")]
            FAILOVER_JA_ZSTD,
            #[cfg(feature = "fallback-zh-full")]
            FAILOVER_ZH_FULL_ZSTD,
        ]
        .into_iter()
        .map(|zstd| Blob::new(Arc::new(term_font::data::decompress(zstd))))
        .collect()
    })
}

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
    /// The one embedded face. In a build without `bundled-font` there is none;
    /// callers must check [`Self::valid`] and report the missing font.
    pub fn bundled() -> Self {
        #[cfg(feature = "bundled-font")]
        {
            static DATA: OnceLock<Blob<u8>> = OnceLock::new();
            Self::bundled_named(BUNDLED_FAMILY, BUNDLED_ZSTD, &DATA)
        }
        #[cfg(not(feature = "bundled-font"))]
        {
            Self {
                family: "no bundled font".into(),
                data: Blob::new(Arc::new(&[][..])),
                index: 0,
            }
        }
    }

    /// The bundled Maple Mono NF face, with the same Nerd Font glyphs and wider language coverage.
    pub fn bundled_maple() -> Self {
        Self::bundled()
    }

    /// The bundled Cascadia Code NF face, selectable instead of the default.
    #[cfg(feature = "cascadia")]
    pub fn bundled_cascadia() -> Self {
        static DATA: OnceLock<Blob<u8>> = OnceLock::new();
        Self::bundled_named(CASCADIA_FAMILY, CASCADIA_ZSTD, &DATA)
    }

    #[cfg(any(feature = "bundled-font", feature = "cascadia"))]
    fn bundled_named(
        family: &'static str,
        zstd: &'static [u8],
        cache: &'static OnceLock<Blob<u8>>,
    ) -> Self {
        Self {
            family: family.into(),
            data: cache
                .get_or_init(|| Blob::new(Arc::new(term_font::data::decompress(zstd))))
                .clone(),
            index: 0,
        }
    }

    /// A face from font bytes at TTC index `index`. The family name comes from
    /// the font's own name table. Malformed bytes give an unusable face; check
    /// [`Self::valid`] before drawing with it.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>, index: u32) -> Self {
        let data = Blob::new(Arc::new(bytes.into()));
        let family = FontRef::from_index(data.data(), index as usize)
            .and_then(|font| {
                font.localized_strings()
                    .find_by_id(StringId::Family, None)
                    .map(|name| name.chars().collect::<String>())
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unnamed font".to_owned());
        Self {
            family: family.into(),
            data,
            index,
        }
    }

    /// The first face in `path`.
    pub fn from_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self::from_bytes(std::fs::read(path)?, 0))
    }

    /// The regular face of `family`, or the bundled one when it is not installed. Bundled
    /// families stay bundled even when installed, so those profiles draw the same everywhere.
    /// A build with no bundled face uses any installed font instead.
    pub fn load(family: &str) -> Self {
        let family = family.trim();
        #[cfg(feature = "bundled-font")]
        if family.is_empty() || family == BUNDLED_FAMILY {
            return Self::bundled();
        }
        #[cfg(not(feature = "bundled-font"))]
        if family.is_empty() {
            return installed_default().unwrap_or_else(Self::bundled);
        }
        if family == MAPLE_FAMILY {
            return Self::bundled_maple();
        }
        #[cfg(feature = "cascadia")]
        if family == CASCADIA_FAMILY {
            return Self::bundled_cascadia();
        }
        static LOADED: Mutex<Option<HashMap<String, Face>>> = Mutex::new(None);
        let mut loaded = LOADED.lock().expect("faces");
        let loaded = loaded.get_or_insert_with(HashMap::new);
        if let Some(face) = loaded.get(family) {
            return face.clone();
        }
        let face = installed(family)
            .or_else(|| {
                #[cfg(not(feature = "bundled-font"))]
                {
                    installed_default()
                }
                #[cfg(feature = "bundled-font")]
                {
                    None
                }
            })
            .unwrap_or_else(Self::bundled);
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

    /// Whether this face has font data that can be drawn.
    pub fn valid(&self) -> bool {
        self.font().is_some()
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
            !name.starts_with('.') && *name != BUNDLED_FAMILY && {
                #[cfg(feature = "cascadia")]
                {
                    *name != CASCADIA_FAMILY
                }
                #[cfg(not(feature = "cascadia"))]
                {
                    true
                }
            }
        })
        .filter(|name| installed_in(&mut collection, name).is_some_and(|face| face.monospaced()))
        .collect();
    families.sort_by_key(|name| name.to_lowercase());
    #[cfg(feature = "cascadia")]
    families.insert(0, CASCADIA_FAMILY.to_owned());
    #[cfg(feature = "bundled-font")]
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

/// Any installed face, preferring a monospaced one. Used only by builds with
/// no bundled font: a profile naming a missing family still gets text.
#[cfg(not(feature = "bundled-font"))]
fn installed_default() -> Option<Face> {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let names: Vec<String> = collection.family_names().map(str::to_owned).collect();
    let mut any = None;
    for name in names {
        if name.starts_with('.') {
            continue;
        }
        if let Some(face) = installed_in(&mut collection, &name) {
            if face.monospaced() {
                return Some(face);
            }
            if any.is_none() {
                any = Some(face);
            }
        }
    }
    any
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

    #[cfg(feature = "bundled-font")]
    #[test]
    fn a_face_from_bytes_and_a_file_keeps_the_fonts_own_name() {
        let bytes = term_font::data::decompress(BUNDLED_ZSTD);
        let from_bytes = Face::from_bytes(bytes.clone(), 0);
        assert_eq!(from_bytes.family(), BUNDLED_FAMILY);
        assert!(from_bytes.valid());

        let path = std::env::temp_dir().join(format!("term-atlas-{}.ttf", std::process::id()));
        std::fs::write(&path, &bytes).expect("write font");
        let from_file = Face::from_file(&path).expect("read font");
        let _ = std::fs::remove_file(&path);
        assert_eq!(from_file.family(), BUNDLED_FAMILY);
        assert!(from_file.valid());
    }

    #[cfg(feature = "bundled-font")]
    #[test]
    fn a_family_that_is_not_installed_draws_with_the_bundled_face() {
        let face = Face::load("rune-term no such family");
        assert_eq!(face, Face::bundled());
        assert_eq!(face.family(), BUNDLED_FAMILY);
    }

    #[cfg(all(feature = "bundled-font", feature = "cascadia"))]
    #[test]
    fn bundled_families_load_their_own_bytes() {
        assert_eq!(Face::load(MAPLE_FAMILY), Face::bundled());
        let cascadia = Face::load(CASCADIA_FAMILY);
        assert_eq!(cascadia, Face::bundled_cascadia());
        assert_ne!(cascadia, Face::bundled());
        assert_eq!(cascadia.family(), CASCADIA_FAMILY);
        assert!(cascadia.monospaced());
    }

    #[cfg(all(target_os = "macos", feature = "bundled-font", feature = "cascadia"))]
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
