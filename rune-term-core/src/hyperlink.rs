//! Hyperlink types and helpers shared by the terminal component.

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

pub fn open_mods_rune(mods: rune::Mods) -> bool {
    if cfg!(target_os = "macos") {
        mods.logo && !mods.ctrl && !mods.alt && !mods.shift
    } else {
        mods.ctrl && !mods.alt && !mods.shift && !mods.logo
    }
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

#[cfg(not(feature = "pty"))]
thread_local! {
    static TEST_OPENER: std::cell::RefCell<Option<Vec<String>>> = const {
        std::cell::RefCell::new(None)
    };
}

#[cfg(not(feature = "pty"))]
pub struct OpenCapture;

#[cfg(not(feature = "pty"))]
impl Drop for OpenCapture {
    fn drop(&mut self) {
        TEST_OPENER.with(|slot| *slot.borrow_mut() = None);
    }
}

#[cfg(not(feature = "pty"))]
pub fn capture_opens() -> (OpenCapture, std::rc::Rc<std::cell::RefCell<Vec<String>>>) {
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let shared = opened.clone();
    TEST_OPENER.with(|slot| {
        *slot.borrow_mut() = Some(Vec::new());
    });
    (OpenCapture, shared)
}

#[cfg(not(feature = "pty"))]
pub fn take_captured_opens() -> Vec<String> {
    TEST_OPENER.with(|slot| slot.borrow_mut().take().unwrap_or_default())
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
    #[cfg(not(feature = "pty"))]
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
                sample.fg = solid(0x0043_85BE);
                sample.fg[3] = 255;
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

fn solid(value: u32) -> [u8; 4] {
    [
        ((value >> 16) & 0x0000_00FF) as u8,
        ((value >> 8) & 0x0000_00FF) as u8,
        (value & 0x0000_00FF) as u8,
        255,
    ]
}
