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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenDecision {
    Open,
    WarnAndOpen,
}

#[cfg(test)]
thread_local! {
    static TEST_OPENER: std::cell::RefCell<Option<Vec<String>>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(test)]
pub(crate) struct OpenCapture;

#[cfg(test)]
impl Drop for OpenCapture {
    fn drop(&mut self) {
        TEST_OPENER.with(|slot| *slot.borrow_mut() = None);
    }
}

#[cfg(test)]
pub(crate) fn capture_opens() -> (OpenCapture, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let shared = opened.clone();
    TEST_OPENER.with(|slot| {
        *slot.borrow_mut() = Some(Vec::new());
    });
    (OpenCapture, shared)
}

#[cfg(test)]
pub(crate) fn take_captured_opens() -> Vec<String> {
    TEST_OPENER.with(|slot| slot.borrow_mut().take().unwrap_or_default())
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

pub fn open_decision(uri: &str) -> OpenDecision {
    let Some((scheme, _)) = uri.split_once(':') else {
        return OpenDecision::WarnAndOpen;
    };
    if scheme.eq_ignore_ascii_case("http")
        || scheme.eq_ignore_ascii_case("https")
        || scheme.eq_ignore_ascii_case("mailto")
    {
        OpenDecision::Open
    } else {
        OpenDecision::WarnAndOpen
    }
}

pub fn open_uri(uri: &str) -> bool {
    open_uri_with(uri, |_| true)
}

/// Opens `uri` when `confirm_risky` allows it. `WarnAndOpen` schemes are logged
/// and passed to the hook; safe web schemes open directly.
pub fn open_uri_with(uri: &str, confirm_risky: impl FnOnce(&str) -> bool) -> bool {
    let decision = open_decision(uri);
    if decision == OpenDecision::WarnAndOpen {
        eprintln!("opening non-web link scheme: {uri}");
        if !confirm_risky(uri) {
            return false;
        }
    }
    #[cfg(test)]
    {
        let captured = TEST_OPENER.with(|slot| {
            let mut slot = slot.borrow_mut();
            if let Some(uris) = slot.as_mut() {
                uris.push(uri.to_owned());
                true
            } else {
                false
            }
        });
        if captured {
            return true;
        }
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
    true
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

    #[test]
    fn implicit_url_resolves_when_no_osc8_link() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.surface.inject_output(b"go https://example.com ok");
            host.render.update();
        }
        let host = model.surfaces.primary().unwrap();
        let link = resolve(host, 0, 5).expect("implicit url");
        assert!(link.uri.starts_with("https://example.com"), "{}", link.uri);
    }

    #[test]
    fn open_decision_warns_for_non_web_schemes() {
        assert_eq!(open_decision("https://example.com"), OpenDecision::Open);
        assert_eq!(open_decision("http://example.com"), OpenDecision::Open);
        assert_eq!(open_decision("mailto:test@example.com"), OpenDecision::Open);
        assert_eq!(open_decision("file:///tmp/x"), OpenDecision::WarnAndOpen);
        assert_eq!(open_decision("ssh://host"), OpenDecision::WarnAndOpen);
    }

    #[test]
    fn open_policy_hook_can_block_risky_schemes() {
        let (_capture, _opened) = capture_opens();
        assert!(!open_uri_with("file:///tmp/x", |_| false));
        assert!(take_captured_opens().is_empty());
        let (_capture, _opened) = capture_opens();
        assert!(open_uri_with("file:///tmp/x", |_| true));
        assert_eq!(take_captured_opens(), vec!["file:///tmp/x".to_string()]);
    }
}
