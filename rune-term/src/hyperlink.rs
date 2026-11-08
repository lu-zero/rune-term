//! OSC 8 hyperlinks and implicitly detected URLs.

use librio::Modifiers;
use term_grid::{CellSample, CursorSample, GridSource};

use crate::surfaces::HostSurface;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverLink {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub uri: String,
}

/// Resolve a link under a viewport cell. OSC 8 takes precedence.
pub fn resolve(host: &HostSurface, line: usize, col: usize) -> Option<HoverLink> {
    if let Some(uri) = host.render.link_at(line, col) {
        let (start, end) = host.render.link_run(line, col).unwrap_or((col, col));
        return Some(HoverLink {
            line,
            start,
            end,
            uri: uri.to_string(),
        });
    }
    let (uri, start, end) = host
        .surface
        .url_at(u16::try_from(line).ok()?, u16::try_from(col).ok()?)?;
    Some(HoverLink {
        line,
        start: usize::from(start),
        end: usize::from(end),
        uri,
    })
}

/// Modifier+click opens links. Plain interaction stays available for selection
/// and mouse-reporting applications.
pub fn open_mods(mods: Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        mods.contains(Modifiers::SUPER)
            && !mods.intersects(Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT)
    } else {
        mods.contains(Modifiers::CTRL)
            && !mods.intersects(Modifiers::ALT | Modifiers::SHIFT | Modifiers::SUPER)
    }
}

pub fn open_uri(uri: &str) {
    if scheme_policy(uri).is_warn() {
        eprintln!("opening non-web link scheme: {uri}");
    }
    let uri = uri.to_owned();
    std::thread::spawn(move || {
        let result = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&uri).spawn()
        } else if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", &uri])
                .spawn()
        } else {
            std::process::Command::new("xdg-open").arg(&uri).spawn()
        };
        if let Err(err) = result {
            eprintln!("open link failed: {err}");
        }
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SchemePolicy {
    Common,
    Warn,
}

impl SchemePolicy {
    fn is_warn(self) -> bool {
        self == Self::Warn
    }
}

fn scheme_policy(uri: &str) -> SchemePolicy {
    let Some((scheme, _)) = uri.split_once(':') else {
        return SchemePolicy::Warn;
    };
    if scheme.eq_ignore_ascii_case("http")
        || scheme.eq_ignore_ascii_case("https")
        || scheme.eq_ignore_ascii_case("mailto")
    {
        SchemePolicy::Common
    } else if scheme.eq_ignore_ascii_case("file") {
        SchemePolicy::Warn
    } else {
        SchemePolicy::Warn
    }
}
pub fn tint<S: GridSource>(source: S, hover: Option<&HoverLink>) -> Tint<S> {
    Tint {
        source,
        hover: hover.cloned(),
    }
}

pub struct Tint<S> {
    source: S,
    hover: Option<HoverLink>,
}

impl<S: GridSource> GridSource for Tint<S> {
    fn columns(&self) -> usize {
        self.source.columns()
    }

    fn lines(&self) -> usize {
        self.source.lines()
    }

    fn row_dirty(&self, line: usize) -> bool {
        self.source.row_dirty(line)
    }

    fn cell(&self, line: usize, col: usize) -> CellSample {
        let mut sample = self.source.cell(line, col);
        if let Some(hover) = &self.hover {
            if hover.line == line && col >= hover.start && col <= hover.end {
                sample.underline = true;
            }
        }
        sample
    }

    fn cursor(&self) -> CursorSample {
        self.source.cursor()
    }

    fn reset_dirty(&mut self) {
        self.source.reset_dirty();
    }
}

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::grid_source::SurfaceGrid;
    use crate::model;
    use term_grid::GridSource;

    fn host() -> crate::model::Model {
        let mut model = model::init();
        delegate::setup(&mut model, std::sync::Arc::new(|| {}));
        model
    }

    #[test]
    fn open_mods_follow_platform_convention() {
        let cmd = Modifiers::SUPER;
        let ctrl = Modifiers::CTRL;
        if cfg!(target_os = "macos") {
            assert!(open_mods(cmd));
            assert!(!open_mods(ctrl));
        } else {
            assert!(open_mods(ctrl));
            assert!(!open_mods(cmd));
        }
        assert!(!open_mods(Modifiers::empty()));
    }

    #[test]
    fn osc8_link_resolves_from_render_state() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.surface
                .inject_output(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07");
            host.render.update();
        }
        let host = model.surfaces.primary().unwrap();
        let link = resolve(host, 0, 1).expect("osc8 link");
        assert_eq!(link.uri, "https://example.com");
        assert_eq!(link.start, 0);
        assert!(link.end >= 3);
    }

    #[test]
    fn hover_tint_underlines_the_run() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.surface.inject_output(b"plain text");
            host.render.update();
        }
        let host = model.surfaces.primary_mut().unwrap();
        let hover = HoverLink {
            line: 0,
            start: 0,
            end: 4,
            uri: "https://example.com".to_string(),
        };
        let source = SurfaceGrid {
            render: &mut host.render,
        };
        let tinted = tint(source, Some(&hover));
        assert!(tinted.cell(0, 0).underline);
        assert!(tinted.cell(0, 4).underline);
        assert!(!tinted.cell(0, 6).underline);
    }
}
