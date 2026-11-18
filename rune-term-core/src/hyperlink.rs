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
    Confirm,
    Refuse,
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

/// Web and mail links open, other schemes ask first. A link with no scheme, or a file on
/// another machine, is refused: the opener would read it as a local path.
pub fn open_decision(uri: &str) -> OpenDecision {
    let Some((scheme, rest)) = uri.split_once(':') else {
        return OpenDecision::Refuse;
    };
    let valid = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid {
        return OpenDecision::Refuse;
    }
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "https" | "mailto" => OpenDecision::Open,
        "file" if !local_file(rest) => OpenDecision::Refuse,
        _ => OpenDecision::Confirm,
    }
}

fn local_file(rest: &str) -> bool {
    let Some(authority) = rest.strip_prefix("//") else {
        return true;
    };
    let host = authority.split('/').next().unwrap_or_default();
    host.is_empty() || host.eq_ignore_ascii_case("localhost") || this_host(host)
}

#[cfg(unix)]
fn this_host(host: &str) -> bool {
    let mut name = [0u8; 256];
    // SAFETY: the buffer is writable for its whole length.
    if unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len()) } != 0 {
        return false;
    }
    let len = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    std::str::from_utf8(&name[..len]).is_ok_and(|name| name.eq_ignore_ascii_case(host))
}

#[cfg(not(unix))]
fn this_host(_host: &str) -> bool {
    false
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

/// Hands `uri` to the desktop. The caller has taken [`open_decision`] into account.
pub fn open_uri(uri: &str) {
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
            return;
        }
    }
    let uri = uri.to_owned();
    std::thread::spawn(move || {
        let result = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(&uri).spawn()
        } else if cfg!(target_os = "windows") {
            // Not `cmd /C start`: cmd would split the URL at `&`.
            std::process::Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", &uri])
                .spawn()
        } else {
            std::process::Command::new("xdg-open").arg(&uri).spawn()
        };
        if let Err(err) = result {
            eprintln!("open link failed: {err}");
        }
    });
}

/// Resolve a link under a viewport cell. OSC 8 takes precedence.
pub fn resolve(host: &HostSurface, line: usize, col: usize) -> Option<HoverLink> {
    resolve_surface(&host.surface, &host.render, line, col)
}

pub fn resolve_surface(
    surface: &librio::Surface,
    render: &librio::RenderState,
    line: usize,
    col: usize,
) -> Option<HoverLink> {
    if let Some(uri) = render.link_at(line, col) {
        let (start, end) = render.link_run(line, col).unwrap_or((col, col));
        return Some(HoverLink {
            line,
            start,
            end,
            uri: uri.to_string(),
        });
    }
    let (uri, start, end) = surface.url_at(u16::try_from(line).ok()?, u16::try_from(col).ok()?)?;
    Some(HoverLink {
        line,
        start: usize::from(start),
        end: usize::from(end),
        uri,
    })
}

/// Underlines the hovered link in the theme's accent.
pub fn tint<S: GridSource>(source: S, hover: Option<&HoverLink>, accent: u32) -> Tint<S> {
    Tint {
        source,
        hover: hover.cloned(),
        accent: solid(accent),
    }
}

pub struct Tint<S> {
    source: S,
    hover: Option<HoverLink>,
    accent: [u8; 4],
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
                sample.fg = self.accent;
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
