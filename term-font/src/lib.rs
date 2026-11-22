//! System font discovery and colour glyph rasterization for rune-term's grid.
//! No Rune and no librio: fontique finds a font that covers a codepoint, swash
//! and skrifa turn its outlines into pixels.

use fontique::{
    Blob, Collection, CollectionOptions, FallbackKey, GenericFamily, QueryFamily, QueryStatus,
    Script, SourceCache, SourceCacheOptions,
};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Mutex, OnceLock};

pub mod color;
pub mod data;

/// A system font that covers a codepoint, with the face index inside its file.
#[derive(Clone)]
pub struct FallbackFont {
    pub data: Blob<u8>,
    pub index: u32,
}

/// The result of a font scan is stable for the life of the process. Cache by
/// family and codepoint so a host with many novel cells does not rebuild a
/// fontique collection for every one.
type Discovered = HashMap<(u64, char), (Box<str>, Option<FallbackFont>)>;

static DISCOVERED: OnceLock<Mutex<Discovered>> = OnceLock::new();

/// Find an installed font covering `ch`, preferring `primary_family` and then
/// monospace, UI and sans families before the platform's script fallbacks.
/// Both hits and misses are cached; the family string is kept to rule out a
/// hash collision returning another family's font.
pub fn discover(primary_family: &str, ch: char) -> Option<FallbackFont> {
    let mut hasher = DefaultHasher::new();
    primary_family.hash(&mut hasher);
    let key = (hasher.finish(), ch);
    {
        let cache = DISCOVERED
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((known, hit)) = cache.get(&key) {
            if known.as_ref() == primary_family {
                return hit.clone();
            }
        }
    }
    let found = discover_uncached(primary_family, ch);
    DISCOVERED
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, (primary_family.into(), found.clone()));
    found
}

/// One font collection and source cache for the life of the process: building
/// either scans installed fonts, so doing it per codepoint costs tens of
/// milliseconds per glyph on a first render of non-Latin text.
struct Sources {
    collection: Collection,
    cache: SourceCache,
}

static SOURCES: OnceLock<Mutex<Sources>> = OnceLock::new();

fn sources() -> &'static Mutex<Sources> {
    SOURCES.get_or_init(|| {
        Mutex::new(Sources {
            collection: Collection::new(CollectionOptions {
                shared: false,
                system_fonts: true,
            }),
            cache: SourceCache::new(SourceCacheOptions::default()),
        })
    })
}

fn discover_uncached(primary_family: &str, ch: char) -> Option<FallbackFont> {
    let mut sources = sources()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Sources { collection, cache } = &mut *sources;
    let mut query = collection.query(cache);
    query.set_families([
        QueryFamily::Named(primary_family),
        QueryFamily::Generic(GenericFamily::Monospace),
        QueryFamily::Generic(GenericFamily::SystemUi),
        QueryFamily::Generic(GenericFamily::SansSerif),
    ]);
    query.set_fallbacks(FallbackKey::new(script_of(ch), None));

    let mut found = None;
    query.matches_with(|font| {
        if font
            .charmap()
            .and_then(|charmap| charmap.map(ch))
            .is_some_and(|glyph| glyph != 0)
        {
            found = Some(FallbackFont {
                data: font.blob.clone(),
                index: font.index,
            });
            QueryStatus::Stop
        } else {
            QueryStatus::Continue
        }
    });
    found
}

/// The script whose fallback families are most likely to cover `ch`. Fontique
/// has no char-to-script routine; these are the blocks a terminal meets most.
fn script_of(ch: char) -> Script {
    let cp = u32::from(ch);
    let tag = if (0x3040..=0x30FF).contains(&cp) {
        *b"Kana"
    } else if (0x4E00..=0x9FFF).contains(&cp) {
        *b"Hani"
    } else if (0xAC00..=0xD7AF).contains(&cp) {
        *b"Hang"
    } else if (0x0400..=0x052F).contains(&cp) {
        *b"Cyrl"
    } else if (0x0370..=0x03FF).contains(&cp) {
        *b"Grek"
    } else if (0x0590..=0x05FF).contains(&cp) {
        *b"Hebr"
    } else if (0x0600..=0x06FF).contains(&cp) {
        *b"Arab"
    } else if (0x0900..=0x097F).contains(&cp) {
        *b"Deva"
    } else if (0x0E00..=0x0E7F).contains(&cp) {
        *b"Thai"
    } else {
        *b"Zyyy"
    };
    Script::from_bytes(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_returns_a_font_that_covers_the_codepoint() {
        let Some(font) = discover("Cascadia Code NF", '中') else {
            return;
        };
        let bytes: &[u8] = font.data.as_ref();
        let face = swash::FontRef::from_index(bytes, font.index as usize).expect("face");
        assert_ne!(face.charmap().map('中'), 0);
    }
}
