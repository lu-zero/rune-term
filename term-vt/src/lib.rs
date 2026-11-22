//! Terminal state and host transport, extracted from Rio's `librio`.
//! The C ABI stays in Rio; key events come from `term-input`.
//! Copyright (c) 2022-present Raphael Amorim. MIT licensed; see
//! `THIRD-PARTY-NOTICES.md`.

mod render_state;

pub use render_state::{ImageLayer, ImagePlacement, ImageProtocol, RenderState, ViewportSelection};
/// What a host that draws Glyph Protocol glyphs needs: the registry's
/// entries, their placement (§8.5), and the `glyf` and `vg` decoders.
#[cfg(feature = "graphics")]
pub use rio_graphics::{
    glyph::{glyf_decode, glyph_registry},
    glyph_layout, glyph_vector,
};
pub use rio_vt::ansi::glyph_protocol::GlyphFormat;
pub use rio_vt::clipboard::ClipboardType;
pub use rio_vt::config::colors::term::TermColors;
pub use rio_vt::config::colors::{AnsiColor, ColorRgb, NamedColor};
pub use rio_vt::crosswords::grid::row::Row;
pub use rio_vt::crosswords::pos::Column;
pub use rio_vt::crosswords::square::{Extras, Square};
pub use rio_vt::crosswords::style::{Style, StyleFlags};
pub use rio_vt::event::{BlockedKind, ProgramState, ProgramStatus, ProgramStatusEvent};
pub use rio_vt::grapheme_lut::cluster_width;
pub use rio_vt::selection::SelectionRange;
pub use term_input::{EncodeContext, Key, KeyAction, KeyEvent, KittyFlags, Modifiers};

pub use rio_vt::ansi::CursorShape;
pub use rio_vt::crosswords::pos::Side;
use rio_vt::crosswords::pos::{Column as PosColumn, Line, Pos};
use rio_vt::crosswords::{Crosswords, Mode};
use rio_vt::event::sync::TerminalLock;
#[cfg(feature = "pty")]
use rio_vt::event::Msg;
#[cfg(feature = "pty")]
use rio_vt::event::WindowSize;
use rio_vt::event::{EventListener, RioEvent, WindowId};
#[cfg(feature = "pty")]
use rio_vt::performer::Machine;
use rio_vt::selection::{Selection, SelectionType};
use std::borrow::Cow;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(feature = "pty")]
use std::sync::Mutex;
#[cfg(all(feature = "pty", target_os = "windows"))]
use teletypewriter::create_pty;
#[cfg(all(feature = "pty", not(target_os = "windows")))]
use teletypewriter::create_pty_with_spawn;

pub type SurfaceId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Simple,
    Word,
    Line,
    Block,
}

impl SelectionKind {
    fn to_type(self) -> SelectionType {
        match self {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
            SelectionKind::Block => SelectionType::Block,
        }
    }
}

struct GridSize {
    rows: usize,
    cols: usize,
    cell_width: f32,
    cell_height: f32,
}

impl GridSize {
    /// Cell metrics come from the host's pixel size; graphics protocols
    /// (kitty image placements) need them to map pixels onto cells, so a
    /// zero pixel size would silently drop every placement.
    fn new(cols: usize, rows: usize, pixel_width: u16, pixel_height: u16) -> Self {
        let cell = |pixels: u16, cells: usize| {
            if pixels == 0 || cells == 0 {
                0.
            } else {
                pixels as f32 / cells as f32
            }
        };
        Self {
            rows,
            cols,
            cell_width: cell(pixel_width, cols),
            cell_height: cell(pixel_height, rows),
        }
    }
}

impl rio_vt::crosswords::grid::Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }

    fn square_width(&self) -> f32 {
        self.cell_width
    }

    fn square_height(&self) -> f32 {
        self.cell_height
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    SetTitle {
        title: String,
        subtitle: Option<String>,
    },
    RingBell,
    CursorBlinkingChange,
    /// OSC 9;4 progress (ConEmu numbering): 0 remove, 1 set, 2 error,
    /// 3 indeterminate, 4 paused. `value` is 0-100 where the state
    /// carries one.
    Progress {
        state: u8,
        value: u8,
    },
    /// OSC 7501 program status: a validated report, or a lifecycle event
    /// (new prompt, full reset) after which the embedder must drop
    /// records. Records themselves live in the embedder.
    ProgramStatus(ProgramStatusEvent),
    /// A shell command finished (OSC 133 `D` after a `C`). `exit_status`
    /// is `None` when the shell sent none; `duration` runs from the
    /// start of the command's output.
    CommandFinished {
        exit_status: Option<i32>,
        duration: std::time::Duration,
    },
}

/// `Send + Sync` everywhere threads exist. On wasm there is one thread and
/// delegates hold JS callbacks (which are `!Send`), so the bound relaxes to
/// nothing rather than forcing unsafe impls on the embedder.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSendSync: Send + Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> MaybeSendSync for T {}
#[cfg(target_arch = "wasm32")]
pub trait MaybeSendSync {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSendSync for T {}

pub trait SurfaceDelegate: MaybeSendSync + 'static {
    /// The surface changed, and a [`RenderState::update`] would show it.
    ///
    /// It may not be repeated for later output until the next `update` of
    /// the surface. A host that keeps its own "needs render" flag clears it
    /// before calling `update`, not after: a wakeup that lands between the
    /// update and the clear would otherwise be lost, and with it the output
    /// that follows.
    ///
    /// Called with the surface's terminal locked, on the reader thread
    /// or from inside any surface call that changes what is shown, such as
    /// [`Surface::inject_output`], [`Surface::resize`] or [`Surface::scroll`].
    /// Only flag or schedule work here; calling back into the surface
    /// deadlocks.
    fn wakeup(&self, surface: SurfaceId);
    fn action(&self, _surface: SurfaceId, _action: Action) {}
    fn clipboard_write(&self, _surface: SurfaceId, _kind: ClipboardType, _text: String) {}
    fn close_surface(&self, _surface: SurfaceId) {}
    /// Whether this delegate acts on `Action::ProgramStatus` (OSC 7501).
    /// Read once per surface at creation. While false, the terminal
    /// leaves the support query unanswered, so programs don't report to
    /// a terminal that ignores them, and no program-status actions are
    /// sent.
    fn handles_program_status(&self) -> bool {
        false
    }
    /// The Glyph Protocol payload formats this delegate draws. Read once
    /// per surface at creation. Programs are offered exactly these, and
    /// what they register is read with [`RenderState::glyph_version`]
    /// and [`Surface::glyph`]. None by default, which leaves the
    /// protocol off: a program probing for it gets no answer, as from a
    /// terminal without it. Needs the `graphics` feature, which stores
    /// registrations.
    fn glyph_formats(&self) -> &'static [GlyphFormat] {
        &[]
    }
    /// A program asked what draws `cp` (Glyph Protocol `q`). Only called
    /// for a delegate that draws some format, and like [`Self::wakeup`]
    /// with the terminal locked, so the only surface call allowed here is
    /// [`Surface::glyph_query_reply`]. Answer with it once per query, in
    /// order: right here, or later from any thread when looking the
    /// codepoint up in the fonts is slow. Replies the terminal makes
    /// after the query wait for that answer, so the program gets them in
    /// the order it asked; a query never answered holds them forever.
    fn glyph_query(&self, _surface: SurfaceId, _cp: u32) {}
    /// Bytes the terminal wants delivered to the child process. Only called
    /// on non-`pty` builds, where the host owns the transport (a WebSocket
    /// to a real shell, an in-page demo interpreter, ...); with a PTY the
    /// bytes go straight to it and this never fires. Called with the
    /// terminal locked, or from inside [`Surface::glyph_query_reply`]
    /// while it keeps the replies in order, so only hand the bytes on
    /// here; calling back into the surface deadlocks.
    fn output(&self, _surface: SurfaceId, _bytes: &[u8]) {}
}

/// The pixels of the sixel/iTerm2 images on screen, by image key. The
/// terminal hands new pixels to its renderer once, through its upload
/// queues; here that renderer is the host, so the surface keeps them
/// (kitty images stay in the terminal's own store) until the terminal
/// says an image's last placement is gone. The listener applies each
/// queue as it arrives, and every render state of the surface reads it.
pub(crate) struct InlineImageStore {
    images: std::sync::Mutex<rustc_hash::FxHashMap<u64, Arc<rio_graphics::GraphicData>>>,
    /// Baseline for image change stamps (Instants aren't representable
    /// over the C ABI; nanoseconds relative to this are). One per
    /// surface, so all of its render states give an image the same
    /// stamp.
    epoch: rio_vt::time::Instant,
}

impl Default for InlineImageStore {
    fn default() -> Self {
        Self {
            images: std::sync::Mutex::default(),
            epoch: rio_vt::time::Instant::now(),
        }
    }
}

impl InlineImageStore {
    /// Every write is one insert or remove, so a thread that panicked
    /// while holding the lock left the map whole: a poisoned lock is
    /// taken all the same rather than taking a PTY thread down.
    pub(crate) fn lock(
        &self,
    ) -> std::sync::MutexGuard<'_, rustc_hash::FxHashMap<u64, Arc<rio_graphics::GraphicData>>> {
        self.images
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn epoch(&self) -> rio_vt::time::Instant {
        self.epoch
    }
}

pub(crate) type InlineImages = Arc<InlineImageStore>;

/// What goes to the terminal behind a glyph query the host has yet to
/// answer, in request order. The front is always a query.
#[cfg(feature = "graphics")]
enum Held {
    Query(u32),
    Reply(Cow<'static, [u8]>),
}

#[cfg(feature = "graphics")]
type HeldReplies = Arc<std::sync::Mutex<std::collections::VecDeque<Held>>>;

#[derive(Clone)]
pub(crate) struct Listener {
    surface_id: SurfaceId,
    delegate: Arc<dyn SurfaceDelegate>,
    inline_images: InlineImages,
    /// The delegate answers glyph queries: it draws some format.
    #[cfg(feature = "graphics")]
    glyph_queries: bool,
    #[cfg(feature = "graphics")]
    held: HeldReplies,
    #[cfg(feature = "pty")]
    pty_writer: Arc<Mutex<Option<corcovado::channel::Sender<Msg>>>>,
}

impl Listener {
    fn dispatch(&self, event: RioEvent) {
        match event {
            RioEvent::TerminalDamaged(_) | RioEvent::Render | RioEvent::RenderRoute(_) => {
                self.delegate.wakeup(self.surface_id);
            }
            RioEvent::Title(_, title) => {
                self.delegate.action(
                    self.surface_id,
                    Action::SetTitle {
                        title,
                        subtitle: None,
                    },
                );
            }
            RioEvent::TitleWithSubtitle(title, subtitle) => {
                self.delegate.action(
                    self.surface_id,
                    Action::SetTitle {
                        title,
                        subtitle: Some(subtitle),
                    },
                );
            }
            RioEvent::Bell(_) => {
                self.delegate.action(self.surface_id, Action::RingBell);
            }
            RioEvent::CursorBlinkingChange | RioEvent::CursorBlinkingChangeOnRoute(_) => {
                self.delegate
                    .action(self.surface_id, Action::CursorBlinkingChange);
            }
            RioEvent::ClipboardStore(kind, text) => {
                self.delegate.clipboard_write(self.surface_id, kind, text);
            }
            // A reply the terminal routed through here, behind a query
            // only the owner answers; passed back as an answer so the
            // terminal knows when it has every one of them.
            RioEvent::PtyWrite(_, text) => {
                self.pass_back(Cow::Owned(text.into_bytes()));
            }
            #[cfg(feature = "graphics")]
            RioEvent::GlyphProtocolQuery { cp, .. } if self.glyph_queries => {
                self.held.lock().unwrap().push_back(Held::Query(cp));
                self.delegate.glyph_query(self.surface_id, cp);
            }
            // Not answered here. The terminal is told the query is
            // settled, so the replies behind it take the direct path.
            RioEvent::ColorRequest(..)
            | RioEvent::TextAreaSizeRequest(..)
            | RioEvent::ClipboardLoad(..)
            | RioEvent::GlyphProtocolQuery { .. }
            | RioEvent::GlyphProtocolGraphemeQuery { .. } => {
                self.pass_back(Cow::Borrowed(&[]));
            }
            // Delivered under the terminal lock, from the parser, so the
            // state is current and this orders with `Surface::set_focus`,
            // which also writes under the lock.
            RioEvent::FocusReport(_, focused) => {
                self.write_to_child(Cow::Borrowed(
                    rio_vt::crosswords::focus_report(focused).as_bytes(),
                ));
            }
            RioEvent::CloseTerminal(_) | RioEvent::Exit => {
                self.delegate.close_surface(self.surface_id);
            }
            RioEvent::ProgressReport(report) => {
                use rio_vt::event::ProgressState;
                let state = match report.state {
                    ProgressState::Remove => 0,
                    ProgressState::Set => 1,
                    ProgressState::Error => 2,
                    ProgressState::Indeterminate => 3,
                    ProgressState::Pause => 4,
                };
                self.delegate.action(
                    self.surface_id,
                    Action::Progress {
                        state,
                        value: report.progress.unwrap_or(0),
                    },
                );
            }
            RioEvent::ProgramStatus(event) => {
                self.delegate
                    .action(self.surface_id, Action::ProgramStatus(event));
            }
            RioEvent::UpdateGraphics { queues, .. } => {
                render_state::apply_graphics_updates(&self.inline_images, queues);
                self.delegate.wakeup(self.surface_id);
            }
            RioEvent::CommandFinished {
                exit_status,
                duration,
                ..
            } => {
                self.delegate.action(
                    self.surface_id,
                    Action::CommandFinished {
                        exit_status,
                        duration,
                    },
                );
            }
            _ => {}
        }
    }
}

impl Listener {
    /// Bytes the terminal wants the child to see: the PTY on `pty` builds,
    /// the delegate otherwise.
    fn write_to_child(&self, bytes: Cow<'static, [u8]>) {
        #[cfg(feature = "pty")]
        if let Some(channel) = self.pty_writer.lock().unwrap().as_ref() {
            let _ = channel.send(Msg::Input(bytes));
        }
        #[cfg(not(feature = "pty"))]
        self.delegate.output(self.surface_id, &bytes);
    }

    /// A query from the program that this embedding does not answer.
    /// An answer, a decline (no bytes) or a routed reply going back to
    /// the terminal: the PTY's writer settles one fenced item with it
    /// on `pty` builds, the delegate gets the bytes otherwise.
    fn pass_back(&self, bytes: Cow<'static, [u8]>) {
        #[cfg(feature = "graphics")]
        {
            let mut held = self.held.lock().unwrap();
            if !held.is_empty() {
                held.push_back(Held::Reply(bytes));
                return;
            }
            self.deliver(bytes);
        }
        #[cfg(not(feature = "graphics"))]
        self.deliver(bytes);
    }

    /// Hand one answer or reply to the terminal, now. Callers hold
    /// `held`, so a reply can't overtake one held behind a query.
    fn deliver(&self, bytes: Cow<'static, [u8]>) {
        #[cfg(feature = "pty")]
        if let Some(channel) = self.pty_writer.lock().unwrap().as_ref() {
            let _ = channel.send(Msg::Reply(bytes));
        }
        #[cfg(not(feature = "pty"))]
        if !bytes.is_empty() {
            self.delegate.output(self.surface_id, &bytes);
        }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: RioEvent, _id: WindowId) {
        self.dispatch(event);
    }

    fn send_event_with_high_priority(&self, event: RioEvent, _id: WindowId) {
        self.dispatch(event);
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceDesc {
    pub shell: Option<String>,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
    pub scrollback: usize,
}

impl Default for SurfaceDesc {
    fn default() -> Self {
        Self {
            shell: None,
            args: Vec::new(),
            working_dir: None,
            cols: 80,
            rows: 24,
            pixel_width: 720,
            pixel_height: 432,
            scrollback: 10_000,
        }
    }
}

pub struct Engine {
    delegate: Arc<dyn SurfaceDelegate>,
    next_surface_id: AtomicUsize,
}

impl Engine {
    pub fn new(delegate: Arc<dyn SurfaceDelegate>) -> Self {
        Self {
            delegate,
            next_surface_id: AtomicUsize::new(1),
        }
    }

    /// A surface running the shell `desc` names, or the user's login
    /// shell, on a new pty. Without the `pty` feature the host carries
    /// the bytes instead, and `desc`'s shell, args and working_dir are
    /// unused.
    pub fn create_surface(
        &self,
        desc: &SurfaceDesc,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        #[cfg(feature = "pty")]
        return Surface::new(self, desc, spawn_pty(desc)?);
        #[cfg(not(feature = "pty"))]
        Surface::new(self, desc)
    }

    /// A surface over a pty master the host already holds, instead of a
    /// spawned shell, like foot's `--pty`. `desc`'s shell, args and
    /// working_dir are unused. Nothing of ours runs on it, so nothing is
    /// signaled or reaped and [`Surface::child_pid`] is 0, and the
    /// surface ends when the other side closes: once no process holds
    /// the slave, which on Linux includes one that has never been
    /// opened. Anything but a pty master is refused. The descriptor is
    /// made nonblocking, which a duplicate the host keeps shares, and
    /// closed on failure.
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    pub fn adopt_surface(
        &self,
        desc: &SurfaceDesc,
        main: std::os::fd::OwnedFd,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        let pty = teletypewriter::create_pty_from_fd(
            main,
            desc.cols,
            desc.rows,
            desc.pixel_width,
            desc.pixel_height,
        )?;
        Surface::new(self, desc, pty)
    }
}

/// Translate the terminal's kitty keyboard flags into the encoder's own set.
fn kitty_flags(modes: rio_vt::ansi::KeyboardModes) -> KittyFlags {
    use rio_vt::ansi::KeyboardModes;
    let mut flags = KittyFlags::empty();
    if modes.contains(KeyboardModes::DISAMBIGUATE_ESC_CODES) {
        flags |= KittyFlags::DISAMBIGUATE;
    }
    if modes.contains(KeyboardModes::REPORT_EVENT_TYPES) {
        flags |= KittyFlags::REPORT_EVENT_TYPES;
    }
    if modes.contains(KeyboardModes::REPORT_ALL_KEYS_AS_ESC) {
        flags |= KittyFlags::REPORT_ALL_AS_ESC;
    }
    if modes.contains(KeyboardModes::REPORT_ALTERNATE_KEYS) {
        flags |= KittyFlags::REPORT_ALTERNATE_KEYS;
    }
    if modes.contains(KeyboardModes::REPORT_ASSOCIATED_TEXT) {
        flags |= KittyFlags::REPORT_ASSOCIATED_TEXT;
    }
    flags
}

/// Scheme-prefixed URL pattern for plain-text link detection. The engine
/// is regex-automata (no lookbehind), so trailing prose punctuation is
/// trimmed afterwards by `trailing_url_punctuation`.
const URL_REGEX: &str = "(?:https://|http://|mailto:|ftp://|file:|ssh://|ssh:|git://|tel:|magnet:|ipfs://|ipns://|gemini://|gopher://|news:)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{|}\\^⟨⟩`]+";

/// How many trailing characters of a URL match are prose punctuation
/// rather than URL: `.,;:!?'"` always, and a closing paren/bracket only
/// when its opener is not part of the match.
fn trailing_url_punctuation(text: &str) -> usize {
    let mut len = text.len();
    let mut trimmed = 0;
    while let Some(c) = text[..len].chars().last() {
        let cut = match c {
            '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' => true,
            ')' => text[..len].matches('(').count() < text[..len].matches(')').count(),
            ']' => text[..len].matches('[').count() < text[..len].matches(']').count(),
            _ => false,
        };
        if !cut {
            break;
        }
        len -= c.len_utf8();
        trimmed += 1;
    }
    trimmed
}

pub struct Surface {
    id: SurfaceId,
    /// The terminal's glyph registry, when the delegate draws glyphs;
    /// see [`Surface::glyph`].
    #[cfg(feature = "graphics")]
    glyphs: Option<rio_graphics::glyph::glyph_registry::GlyphRegistry>,
    /// Delivers the glyph query answers and the replies held behind them.
    #[cfg(feature = "graphics")]
    replies: Listener,
    alt_is_meta: std::sync::atomic::AtomicBool,
    terminal: Arc<TerminalLock<Crosswords<Listener>>>,
    /// Compiled URL detector, built on first hover. Hit-testing runs per
    /// pointer event, so the four lazy DFAs must not be rebuilt each time.
    url_regex: std::sync::Mutex<Option<rio_vt::crosswords::search::RegexSearch>>,
    /// VT parser for host-injected output. Persistent so escape sequences
    /// split across `inject_output` calls resume mid-sequence instead of
    /// mis-parsing (and so its sync buffer is allocated once, not per
    /// write).
    processor: std::sync::Mutex<Option<rio_vt::performer::handler::Processor>>,
    /// Sixel/iTerm2 pixels for the render states to read.
    inline_images: InlineImages,
    /// Whether the host breaks shaping runs at the cursor cell; read by
    /// the render states to mark the rows a cursor move reshapes.
    cursor_shaping_break: Arc<std::sync::atomic::AtomicBool>,
    /// Non-pty transport: `write` hands the bytes to the delegate instead
    /// of a PTY channel.
    #[cfg(not(feature = "pty"))]
    delegate: Arc<dyn SurfaceDelegate>,
    #[cfg(feature = "pty")]
    channel: corcovado::channel::Sender<Msg>,
    #[cfg(feature = "pty")]
    shell_pid: u32,
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    main_fd: std::os::fd::RawFd,
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    child_terminator: teletypewriter::ChildTerminator,
    #[cfg(feature = "pty")]
    _io_thread: std::thread::JoinHandle<(
        Machine<teletypewriter::Pty, Listener>,
        rio_vt::performer::State,
    )>,
}

/// Encode one mouse report. SGR (`CSI < b ; x ; y M`) when the program
/// asked for it, else the original X10 form, whose coordinates are
/// offset by 32 and cannot exceed 223 without the UTF-8 extension.
fn mouse_report(button: u8, col: u16, row: u16, pressed: bool, sgr: bool, utf8: bool) -> Vec<u8> {
    let x = col.saturating_add(1);
    let y = row.saturating_add(1);
    if sgr {
        // SGR keeps the real button and marks a release with lowercase `m`.
        let end = if pressed { 'M' } else { 'm' };
        return format!("\x1b[<{button};{x};{y}{end}").into_bytes();
    }
    // The X10/normal form has no release code: the button field collapses to
    // 3, but the modifier/motion bits (bit 3 and up) are kept, matching xterm.
    let encoded = if pressed { button } else { button | 3 };
    let mut out = vec![0x1b, b'[', b'M', 32u8.saturating_add(encoded)];
    for value in [x, y] {
        if utf8 && value >= 95 {
            // Two-byte UTF-8 for the extended range.
            let encoded = char::from_u32(32 + value as u32).unwrap_or('\u{20}');
            let mut buffer = [0u8; 4];
            out.extend_from_slice(encoded.encode_utf8(&mut buffer).as_bytes());
        } else {
            out.push(32u8.saturating_add(value.min(223) as u8));
        }
    }
    out
}

fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    if bracketed {
        let filtered = text.replace(['\x1b', '\x03', '\u{9b}'], "");
        format!("\x1b[200~{filtered}\x1b[201~").into_bytes()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

/// A pty with the shell `desc` names, or the user's login shell.
#[cfg(feature = "pty")]
fn spawn_pty(desc: &SurfaceDesc) -> std::io::Result<teletypewriter::Pty> {
    // No shell in the descriptor means "whatever the user's default is",
    // which teletypewriter resolves (and starts as a login shell).
    let shell = desc.shell.as_deref();

    // The child inherits the host process's environment, which for GUI
    // hosts has no TERM at all (or a stale one). Resolve it the way rio
    // does: prefer rio's terminfo when it's installed, else fall back to
    // the universally known xterm-256color so local prompts and remote
    // ssh sessions both keep working.
    #[cfg(not(target_os = "windows"))]
    {
        let terminfo = match (
            teletypewriter::terminfo_exists("xterm-rio"),
            teletypewriter::terminfo_exists("rio"),
        ) {
            (true, _) => "xterm-rio",
            (false, true) => "rio",
            (false, false) => "xterm-256color",
        };
        let env = vec![
            ("TERM".to_string(), terminfo.to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
        ];
        create_pty_with_spawn(
            shell,
            desc.args.clone(),
            &desc.working_dir,
            Some(env),
            desc.cols,
            desc.rows,
            desc.pixel_width,
            desc.pixel_height,
        )
    }

    #[cfg(target_os = "windows")]
    create_pty(
        shell,
        desc.args.clone(),
        &desc.working_dir,
        None,
        desc.cols,
        desc.rows,
    )
}

impl Surface {
    /// A surface over `pty`, which this crate reads and writes on its own
    /// threads: [`Engine::create_surface`] spawns it and
    /// [`Engine::adopt_surface`] wraps the host's. Without the `pty`
    /// feature the host carries the bytes: it feeds the program's output
    /// with [`Surface::inject_output`] and gets the terminal's through
    /// [`SurfaceDelegate::output`].
    fn new(
        engine: &Engine,
        desc: &SurfaceDesc,
        #[cfg(feature = "pty")] pty: teletypewriter::Pty,
    ) -> Result<Surface, Box<dyn Error + Send + Sync>> {
        let id = engine.next_surface_id.fetch_add(1, Ordering::SeqCst);
        #[cfg(feature = "pty")]
        let pty_writer = Arc::new(Mutex::new(None));
        let inline_images = InlineImages::default();
        // Without `graphics` nothing stores a registration to draw.
        let glyph_formats = if cfg!(feature = "graphics") {
            engine.delegate.glyph_formats()
        } else {
            &[]
        };
        let listener = Listener {
            surface_id: id,
            delegate: engine.delegate.clone(),
            inline_images: inline_images.clone(),
            #[cfg(feature = "graphics")]
            glyph_queries: !glyph_formats.is_empty(),
            #[cfg(feature = "graphics")]
            held: HeldReplies::default(),
            #[cfg(feature = "pty")]
            pty_writer: pty_writer.clone(),
        };
        #[cfg(feature = "graphics")]
        let replies = listener.clone();

        let mut terminal = Crosswords::new(
            GridSize::new(
                desc.cols as usize,
                desc.rows as usize,
                desc.pixel_width,
                desc.pixel_height,
            ),
            CursorShape::Block,
            listener.clone(),
            WindowId::from(id as u64),
            id,
            desc.scrollback,
        );
        terminal.set_program_status_enabled(engine.delegate.handles_program_status());
        terminal.set_glyph_formats(glyph_formats);
        // Made now so the surface holds it; a full reset empties it in
        // place.
        #[cfg(feature = "graphics")]
        let glyphs = (!glyph_formats.is_empty()).then(|| {
            terminal
                .glyph_registry
                .get_or_insert_with(Default::default)
                .clone()
        });
        // On wasm the delegate (and so the whole graph) is single-threaded
        // by design; Arc stays because the pty build shares it with the IO
        // thread and the API is one type on every target.
        #[allow(clippy::arc_with_non_send_sync)]
        let terminal = Arc::new(TerminalLock::new(terminal));

        #[cfg(not(feature = "pty"))]
        {
            Ok(Surface {
                id,
                #[cfg(feature = "graphics")]
                glyphs,
                #[cfg(feature = "graphics")]
                replies,
                // Terminals default alt to meta; the host may override it.
                alt_is_meta: std::sync::atomic::AtomicBool::new(true),
                terminal,
                url_regex: std::sync::Mutex::new(None),
                processor: std::sync::Mutex::new(None),
                inline_images,
                cursor_shaping_break: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                delegate: engine.delegate.clone(),
            })
        }

        #[cfg(feature = "pty")]
        {
            #[cfg(not(target_os = "windows"))]
            let shell_pid = *pty.child.pid.clone() as u32;
            #[cfg(target_os = "windows")]
            let shell_pid = pty.child_watcher().pid().map(|pid| pid.get()).unwrap_or(0);
            #[cfg(not(target_os = "windows"))]
            let main_fd = *pty.child.id;
            #[cfg(not(target_os = "windows"))]
            let child_terminator = pty.child.terminator();

            let machine = Machine::new(
                Arc::clone(&terminal),
                pty,
                listener,
                WindowId::from(id as u64),
                id,
            )
            .map_err(|err| std::io::Error::other(err.to_string()))?;
            let channel = machine.channel();
            *pty_writer.lock().unwrap() = Some(channel.clone());
            let io_thread = machine.spawn();

            Ok(Surface {
                id,
                #[cfg(feature = "graphics")]
                glyphs,
                #[cfg(feature = "graphics")]
                replies,
                // Terminals default alt to meta; the host may override it.
                alt_is_meta: std::sync::atomic::AtomicBool::new(true),
                terminal,
                url_regex: std::sync::Mutex::new(None),
                processor: std::sync::Mutex::new(None),
                inline_images,
                cursor_shaping_break: Arc::new(std::sync::atomic::AtomicBool::new(true)),
                channel,
                shell_pid,
                #[cfg(not(target_os = "windows"))]
                main_fd,
                #[cfg(not(target_os = "windows"))]
                child_terminator,
                _io_thread: io_thread,
            })
        }
    }

    pub fn id(&self) -> SurfaceId {
        self.id
    }

    pub fn write<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        // Input snaps the view back to the live screen and drops any
        // selection, the scroll-to-bottom / clear-on-typing convention
        // (only reached when a key actually produced PTY bytes).
        {
            use rio_vt::crosswords::grid::Scroll;
            let mut term = self.terminal.lock();
            if term.display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
            if term.selection.is_some() {
                term.selection = None;
            }
        }
        self.send(bytes);
    }

    /// The Glyph Protocol registration a cell was printed with, by the
    /// version [`RenderState::glyph_version`] gives for it. A cell keeps
    /// that registration (spec §6.3): registering or clearing the
    /// codepoint again changes only text printed afterwards, and doesn't
    /// damage the screen. A version always means the same glyph, so it
    /// can key a raster cache; versions are unique within one surface.
    /// `None` without a delegate that draws glyphs, or once no cell
    /// shows the registration any more. This reads the registry under
    /// its lock and clones the entry, so look a version up once and
    /// cache what it draws, not once per cell and frame.
    #[cfg(feature = "graphics")]
    pub fn glyph(
        &self,
        version: u32,
    ) -> Option<rio_graphics::glyph::glyph_registry::RegisteredGlyph> {
        self.glyphs.as_ref()?.get_version(version)
    }

    /// Answer the oldest unanswered [`SurfaceDelegate::glyph_query`]:
    /// `system` says whether the host's fonts cover its codepoint, and
    /// whether a program registered it is filled in here. The replies
    /// the terminal made after the query follow it. Returns `false`, and
    /// sends nothing, when no query is waiting.
    #[cfg(feature = "graphics")]
    pub fn glyph_query_reply(&self, system: bool) -> bool {
        use rio_vt::ansi::glyph_protocol::{format_query_response, QueryStatus};
        let Some(glyphs) = &self.glyphs else {
            return false;
        };
        let mut held = self.replies.held.lock().unwrap();
        let Some(&Held::Query(cp)) = held.front() else {
            return false;
        };
        held.pop_front();
        let status = QueryStatus::new(glyphs.contains(cp), system);
        let answer = format_query_response(cp, status).into_bytes();
        self.replies.deliver(Cow::Owned(answer));
        while let Some(Held::Reply(_)) = held.front() {
            if let Some(Held::Reply(bytes)) = held.pop_front() {
                self.replies.deliver(bytes);
            }
        }
        true
    }

    /// Bytes for the child, as they are: the PTY on `pty` builds, the
    /// delegate otherwise.
    fn send<B: Into<Cow<'static, [u8]>>>(&self, bytes: B) {
        #[cfg(feature = "pty")]
        let _ = self.channel.send(Msg::Input(bytes.into()));
        #[cfg(not(feature = "pty"))]
        self.delegate.output(self.id, &bytes.into());
    }

    pub fn text(&self, text: &str) {
        self.write(text.as_bytes().to_vec());
    }

    /// Paste text the way terminals do: when the program asked for
    /// bracketed paste (mode 2004) the text is sent verbatim inside
    /// ESC[200~/ESC[201~ markers, minus ESC, ETX and the 8-bit CSI so
    /// the payload can never close the bracket early and inject
    /// keystrokes; otherwise newlines are normalized to CR, what the
    /// Enter key produces.
    pub fn paste(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let bracketed = self.terminal.lock().mode().contains(Mode::BRACKETED_PASTE);
        self.write(encode_paste(text, bracketed));
    }

    /// A stable, C-friendly view of the terminal modes an embedder needs
    /// for input decisions it makes on its own (touch scrolling, key bars).
    /// Bit 0 mouse reporting, bit 1 application cursor keys, bit 2
    /// alternate screen, bit 3 bracketed paste.
    pub fn mode_bits(&self) -> u32 {
        let mode = self.terminal.lock().mode();
        let mut bits = 0;
        if mode.intersects(Mode::MOUSE_MODE) {
            bits |= 1;
        }
        if mode.contains(Mode::APP_CURSOR) {
            bits |= 1 << 1;
        }
        if mode.contains(Mode::ALT_SCREEN) {
            bits |= 1 << 2;
        }
        if mode.contains(Mode::BRACKETED_PASTE) {
            bits |= 1 << 3;
        }
        bits
    }

    /// Whether alt acts as meta, prefixing with ESC, instead of letting the
    /// platform's text through. See [`EncodeContext::alt_is_meta`].
    pub fn set_alt_is_meta(&self, enabled: bool) {
        self.alt_is_meta.store(enabled, Ordering::Relaxed);
    }

    pub fn alt_is_meta(&self) -> bool {
        self.alt_is_meta.load(Ordering::Relaxed)
    }

    /// Tell the terminal whether this surface has keyboard focus. A change
    /// is reported to a program that asked for focus events (DECSET 1004)
    /// as CSI I or CSI O, and a program that enables them is told the
    /// current state right away; nothing else is sent. A surface starts
    /// unfocused, and a repeated value is ignored. Call it for window focus
    /// and for the host's own tab or split switches alike: to the program,
    /// a surface in a background tab is unfocused. Unlike [`Surface::write`]
    /// this leaves the view and the selection alone; losing focus must not
    /// scroll or deselect.
    pub fn set_focus(&self, focused: bool) {
        // Written under the lock, like the reply to a program enabling
        // focus events, so the two cannot cross.
        let mut terminal = self.terminal.lock();
        if let Some(report) = terminal.set_focus(focused) {
            self.send(report.as_bytes());
        }
    }

    /// Configure the default for grapheme cluster processing (DEC
    /// private mode 2027). On by default; embedders whose renderers
    /// assume legacy wcwidth cell layout can turn it off. Applied
    /// immediately and restored by RIS; a program's DECSET/DECRST
    /// still wins at runtime.
    pub fn set_grapheme_clustering(&self, enabled: bool) {
        self.terminal.lock().set_grapheme_clustering(enabled);
    }

    /// Whether the host breaks shaping runs around the cursor cell, as
    /// a renderer does so a ligature under the cursor shows its
    /// characters one by one. When it does, a render state update marks
    /// the row the cursor left and the row it is on dirty on a cursor
    /// move, without copying either from the terminal, since neither
    /// changed. A host that shapes rows the same wherever the cursor is
    /// turns this off and a cursor move marks no row. On by default.
    pub fn set_cursor_shaping_break(&self, enabled: bool) {
        self.cursor_shaping_break
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn key(&self, event: &KeyEvent) -> bool {
        // The encoding depends on terminal state the embedder does not track,
        // which is the reason this lives here and not in the host.
        let ctx = {
            let terminal = self.terminal.lock();
            EncodeContext {
                app_cursor: terminal.mode().contains(Mode::APP_CURSOR),
                app_keypad: terminal.mode().contains(Mode::APP_KEYPAD),
                kitty: kitty_flags(terminal.keyboard_mode()),
                modify_other_keys: terminal.modify_other_keys(),
                alt_is_meta: self.alt_is_meta.load(Ordering::Relaxed),
            }
        };
        match term_input::key::encode(event, &ctx) {
            Some(bytes) => {
                self.write(bytes);
                true
            }
            None => false,
        }
    }

    pub fn resize(&self, cols: u16, rows: u16, pixel_width: u16, pixel_height: u16) {
        self.terminal.lock().resize(GridSize::new(
            cols as usize,
            rows as usize,
            pixel_width,
            pixel_height,
        ));
        #[cfg(feature = "pty")]
        let _ = self.channel.send(Msg::Resize(WindowSize {
            rows,
            cols,
            width: pixel_width,
            height: pixel_height,
        }));
    }

    /// A wheel scroll, dispatched the way terminals do it: the program
    /// running in the terminal gets first claim.
    ///
    /// Three cases, in order:
    /// mouse reporting on, so the wheel is a mouse event; the alternate
    /// screen with alternate-scroll on, where there is no scrollback to
    /// move so the wheel becomes cursor keys and pagers scroll; and
    /// otherwise the host's scrollback view. Holding shift always means
    /// "give me the scrollback", overriding the first two.
    ///
    /// `lines` is positive for scrolling up (towards history). `col` and
    /// `row` are the cell under the pointer, needed by mouse reports.
    /// Returns true when the program consumed it, false when the
    /// scrollback moved instead.
    pub fn scroll_wheel(&self, lines: i32, col: u16, row: u16, mods: Modifiers) -> bool {
        if lines == 0 {
            return false;
        }
        let (mouse_mode, alt_screen, alt_scroll, app_cursor, sgr, utf8) = {
            let terminal = self.terminal.lock();
            let mode = terminal.mode();
            (
                mode.intersects(Mode::MOUSE_MODE),
                mode.contains(Mode::ALT_SCREEN),
                mode.contains(Mode::ALTERNATE_SCROLL),
                mode.contains(Mode::APP_CURSOR),
                mode.contains(Mode::SGR_MOUSE),
                mode.contains(Mode::UTF8_MOUSE),
            )
        };
        let shift = mods.contains(Modifiers::SHIFT);

        if mouse_mode && !shift {
            // Wheel buttons are 64 (up) and 65 (down), with the modifier
            // bits every mouse report carries.
            let mut button = if lines > 0 { 64 } else { 65 };
            if mods.contains(Modifiers::SHIFT) {
                button += 4;
            }
            if mods.contains(Modifiers::ALT) {
                button += 8;
            }
            if mods.contains(Modifiers::CTRL) {
                button += 16;
            }
            let mut out = Vec::new();
            for _ in 0..lines.abs() {
                out.extend_from_slice(&mouse_report(button, col, row, true, sgr, utf8));
            }
            self.write(out);
            return true;
        }

        if alt_screen && alt_scroll && !shift {
            let up = lines > 0;
            let seq: &[u8] = match (app_cursor, up) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1bOB",
                (false, true) => b"\x1b[A",
                (false, false) => b"\x1b[B",
            };
            let mut out = Vec::with_capacity(seq.len() * lines.unsigned_abs() as usize);
            for _ in 0..lines.abs() {
                out.extend_from_slice(seq);
            }
            self.write(out);
            return true;
        }

        self.scroll(lines);
        false
    }

    /// Report a mouse button press/release to the program when it asked for
    /// mouse events (DEC 1000/1002/1003, or X10/9). `button` is 0=left,
    /// 1=middle, 2=right. Returns true when a report was written, so the host
    /// should not start a local selection. Returns false when no program
    /// is grabbing the mouse, or shift is held to force a local selection
    /// (shift-to-bypass).
    pub fn mouse_button(
        &self,
        col: u16,
        row: u16,
        button: u8,
        pressed: bool,
        mods: Modifiers,
    ) -> bool {
        let (mouse_mode, x10, sgr, utf8) = {
            let mode = self.terminal.lock().mode();
            (
                mode.intersects(Mode::MOUSE_MODE),
                mode.contains(Mode::MOUSE_REPORT_X10),
                mode.contains(Mode::SGR_MOUSE),
                mode.contains(Mode::UTF8_MOUSE),
            )
        };
        if !mouse_mode || mods.contains(Modifiers::SHIFT) {
            return false;
        }
        // X10 (mode 9) reports only presses of the three main buttons, with
        // no modifiers and no release.
        if x10 && (!pressed || button > 2) {
            return false;
        }
        let mut encoded = button;
        if !x10 {
            if mods.contains(Modifiers::ALT) {
                encoded += 8;
            }
            if mods.contains(Modifiers::CTRL) {
                encoded += 16;
            }
        }
        self.write(mouse_report(encoded, col, row, pressed, sgr, utf8));
        true
    }

    /// Report pointer motion for button-event (1002, a button held) and
    /// any-event (1003, bare motion) modes. `button` is 0/1/2 for the button
    /// held during a drag, or 3 when none is held. Returns true when a report
    /// was written.
    pub fn mouse_motion(&self, col: u16, row: u16, button: u8, mods: Modifiers) -> bool {
        let (drag, motion, sgr, utf8) = {
            let mode = self.terminal.lock().mode();
            (
                mode.contains(Mode::MOUSE_DRAG),
                mode.contains(Mode::MOUSE_MOTION),
                mode.contains(Mode::SGR_MOUSE),
                mode.contains(Mode::UTF8_MOUSE),
            )
        };
        if mods.contains(Modifiers::SHIFT) {
            return false;
        }
        // 1002 reports motion only while a button is down; 1003 reports all.
        let wanted = if button >= 3 { motion } else { drag || motion };
        if !wanted {
            return false;
        }
        // The motion bit (32) rides on top of the button.
        let mut encoded = button.saturating_add(32);
        if mods.contains(Modifiers::ALT) {
            encoded += 8;
        }
        if mods.contains(Modifiers::CTRL) {
            encoded += 16;
        }
        self.write(mouse_report(encoded, col, row, true, sgr, utf8));
        true
    }

    pub fn scroll(&self, delta_lines: i32) {
        use rio_vt::crosswords::grid::Scroll;
        self.terminal
            .lock()
            .scroll_display(Scroll::Delta(delta_lines));
    }

    /// Begin a selection. `side` says which half of the cell the pointer
    /// is on, which is what decides whether that cell is inside the
    /// selection; assuming a side makes cells at the ends of a drag
    /// unreachable in one direction.
    pub fn selection_begin(&self, viewport_line: i32, col: usize, kind: SelectionKind, side: Side) {
        let mut term = self.terminal.lock();
        let offset = term.display_offset() as i32;
        let pos = Pos::new(Line(viewport_line - offset), PosColumn(col));
        term.selection = Some(Selection::new(kind.to_type(), pos, side));
        term.mark_fully_damaged();
        term.request_render();
    }

    pub fn selection_update(&self, viewport_line: i32, col: usize, side: Side) {
        let mut term = self.terminal.lock();
        let offset = term.display_offset() as i32;
        let pos = Pos::new(Line(viewport_line - offset), PosColumn(col));
        if let Some(selection) = &mut term.selection {
            selection.update(pos, side);
            term.mark_fully_damaged();
            term.request_render();
        }
    }

    pub fn selection_clear(&self) {
        let mut term = self.terminal.lock();
        if term.selection.take().is_some() {
            term.mark_fully_damaged();
            term.request_render();
        }
    }

    pub fn selection_text(&self) -> Option<String> {
        self.terminal.lock().selection_to_string()
    }

    /// The shell's current working directory: OSC 7 when the shell reports
    /// it, otherwise the OS's view of the foreground process's cwd (so it
    /// works without any shell integration). Used by session persistence to
    /// restore each surface in the directory it was left in.
    pub fn working_dir(&self) -> Option<String> {
        let reported = self
            .terminal
            .lock()
            .current_directory
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());
        if reported.is_some() {
            return reported;
        }
        #[cfg(all(feature = "pty", not(target_os = "windows")))]
        {
            teletypewriter::foreground_process_path(self.main_fd, self.shell_pid)
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        }
        #[cfg(any(not(feature = "pty"), target_os = "windows"))]
        None
    }

    /// Lines currently held in scrollback (the ring, not counting
    /// eviction). Search coordinates are relative to the top of this ring.
    pub fn history_size(&self) -> usize {
        self.terminal.lock().history_size()
    }

    /// Serialize scrollback + screen to a byte stream that reconstructs
    /// content, SGR styling, and OSC 8 hyperlinks when replayed into a
    /// same-width terminal (`inject_output`). The style state machine is
    /// reset-then-set per change: verbose but unambiguous.
    pub fn serialize(&self) -> String {
        use rio_vt::crosswords::grid::{Dimensions, GridSquare};
        use rio_vt::crosswords::square::{CellFlags, Wide};

        let term = self.terminal.lock();
        let grid = &term.grid;
        let cols = grid.columns();
        let history = term.history_size() as i32;
        let rows = term.screen_lines() as i32;

        // Trim trailing all-empty screen rows, like dump().
        let mut last = rows - 1;
        'trim: while last > -history {
            let row = &grid[Line(last)];
            for col in 0..cols {
                let square = row[PosColumn(col)];
                if !square.is_empty() {
                    break 'trim;
                }
            }
            last -= 1;
        }

        let mut out = String::new();
        let mut style = Style::default();
        let mut link: Option<String> = None;
        out.push_str("\x1b[0m");

        for line in -history..=last {
            let row = &grid[Line(line)];
            let wrapped = row[PosColumn(cols - 1)]
                .cell_flags()
                .contains(CellFlags::WRAPLINE);

            // Within a row, drop the trailing run of default empty cells
            // (unless the row wraps, where every cell is content).
            let mut end = cols;
            if !wrapped {
                while end > 0 {
                    let square = row[PosColumn(end - 1)];
                    if !square.is_empty() {
                        break;
                    }
                    end -= 1;
                }
            }

            for col in 0..end {
                let square = row[PosColumn(col)];
                match square.wide() {
                    Wide::Spacer | Wide::LeadingSpacer => continue,
                    _ => {}
                }

                let cell_style = grid.style_of(&square);
                if cell_style != style {
                    rio_vt::crosswords::formatter::write_sgr(&mut out, &cell_style);
                    style = cell_style;
                }

                let cell_link = square
                    .extras_id_checked()
                    .and_then(|id| grid.extras_table.get(id))
                    .and_then(|extras| extras.hyperlink.as_ref())
                    .map(|h| h.uri().to_string());
                if cell_link != link {
                    match &cell_link {
                        Some(uri) => {
                            out.push_str("\x1b]8;;");
                            out.push_str(uri);
                            out.push_str("\x1b\\");
                        }
                        None => out.push_str("\x1b]8;;\x1b\\"),
                    }
                    link = cell_link;
                }

                let c = square.c();
                out.push(if c == '\0' { ' ' } else { c });
                if let Some(extras) = square
                    .extras_id_checked()
                    .and_then(|id| grid.extras_table.get(id))
                {
                    for z in &extras.zerowidth {
                        out.push(*z);
                    }
                }
            }

            if !wrapped && line < last {
                out.push_str("\r\n");
            }
        }

        if link.is_some() {
            out.push_str("\x1b]8;;\x1b\\");
        }
        out.push_str("\x1b[0m");
        out
    }

    /// All regex matches across scrollback + screen, top to bottom, as
    /// `(start_line, start_col, end_line, end_col)` with lines relative to
    /// the top of the scrollback ring. `None` when the pattern is invalid.
    pub fn search(&self, pattern: &str, max: usize) -> Option<Vec<(u32, u16, u32, u16)>> {
        use rio_vt::crosswords::pos::Direction;
        use rio_vt::crosswords::search::{RegexIter, RegexSearch};

        let mut regex = RegexSearch::new(pattern).ok()?;
        let term = self.terminal.lock();
        let history = term.history_size() as i32;
        let rows = term.screen_lines() as i32;
        let cols = term.columns();
        let start = Pos::new(Line(-history), PosColumn(0));
        let end = Pos::new(Line(rows - 1), PosColumn(cols - 1));

        let mut matches = Vec::new();
        for m in RegexIter::new(start, end, Direction::Right, &term, &mut regex) {
            let (s, e) = (m.start(), m.end());
            matches.push((
                (s.row.0 + history) as u32,
                s.col.0 as u16,
                (e.row.0 + history) as u32,
                e.col.0 as u16,
            ));
            if matches.len() >= max {
                break;
            }
        }
        Some(matches)
    }

    /// The pid of the program this surface spawned (the shell, or the
    /// configured `shell` program), for identity and diagnostics. Do not
    /// signal it on teardown: dropping the surface already hangs up the
    /// process group, and the reader thread escalates to SIGKILL and
    /// reaps, so a host-side killpg would race that escalation and can
    /// hit a recycled pid; a host that wants an extra signal must use
    /// [`Surface::hangup_child`], which goes through the lifecycle
    /// guard. On Windows it is the conpty child's process id; 0 if the
    /// pid was unavailable.
    #[cfg(feature = "pty")]
    pub fn child_pid(&self) -> u32 {
        self.shell_pid
    }

    /// Hang up the child's process group through the lifecycle guard:
    /// a no-op once the child was reaped, so a host can signal on its
    /// own teardown without racing the reader thread's escalation or
    /// hitting a recycled pid. Returns false when delivery failed.
    #[cfg(all(feature = "pty", not(target_os = "windows")))]
    pub fn hangup_child(&self) -> bool {
        self.child_terminator.hangup().is_ok()
    }

    /// The foreground process's name (the program the user is running
    /// right now: `claude`, `vim`, or the shell itself), from the kernel.
    /// Hosts use it to tell what a pane is running without any shell
    /// integration.
    #[cfg(feature = "pty")]
    pub fn foreground_process_name(&self) -> String {
        #[cfg(target_os = "windows")]
        return String::new();

        #[cfg(not(target_os = "windows"))]
        teletypewriter::foreground_process_name(self.main_fd, self.shell_pid)
    }

    /// Inject bytes into the terminal's DISPLAY (the VT parser), as if they
    /// came from the child process — NOT into the PTY input. Used to replay
    /// saved scrollback on restore; the shell never sees these bytes, so it
    /// can't execute them. Bytes are plain output (convert `\n` to `\r\n`
    /// upstream if you want proper line starts).
    ///
    /// The parser is persistent: an escape sequence split across two
    /// calls resumes where it left off, exactly like PTY chunking.
    pub fn inject_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut guard = self.processor.lock().unwrap();
        let processor = guard.get_or_insert_with(Default::default);
        let mut term = self.terminal.lock();
        processor.advance(&mut *term, bytes);
        // With a PTY, the replies these bytes produce go to its writer
        // thread as the parser produces them, in their place among
        // everything else bound for it; without one they are events.
        // No PTY thread announces injected bytes; the host that
        // injected them is told what the reader thread would tell it.
        if term.peek_damage_event().is_some() {
            term.request_render();
        }
    }

    /// Plain-text URL under a viewport cell, with the run to underline on
    /// the hovered row: `(uri, start_col, end_col)`. Detection is regex
    /// over the logical (unwrapped) line containing the cell, so wrapped
    /// URLs resolve whole. Hit-test shaped: call it from pointer events;
    /// OSC 8 links should be checked first, they are explicit.
    pub fn url_at(&self, viewport_line: u16, col: u16) -> Option<(String, u16, u16)> {
        use rio_vt::crosswords::pos::Direction;
        use rio_vt::crosswords::search::{RegexIter, RegexSearch};
        use rio_vt::crosswords::square::CellFlags;

        // A fully wrapped scrollback (one endless logical line) must not
        // turn a hover into a whole-buffer regex scan.
        const MAX_WRAP_SCAN: i32 = 100;

        let mut guard = self.url_regex.lock().unwrap();
        let regex = match guard.as_mut() {
            Some(regex) => regex,
            None => guard.insert(RegexSearch::new(URL_REGEX).expect("static URL pattern compiles")),
        };

        let term = self.terminal.lock();
        let cols = term.columns();
        let rows = term.screen_lines() as i32;
        let history = term.history_size() as i32;
        let display_offset = term.display_offset() as i32;
        let grid_line = viewport_line as i32 - display_offset;
        if grid_line >= rows || col as usize >= cols {
            return None;
        }
        let point = Pos::new(Line(grid_line), PosColumn(col as usize));

        let wraps = |line: i32| {
            term.grid[Line(line)][PosColumn(cols - 1)]
                .cell_flags()
                .contains(CellFlags::WRAPLINE)
        };
        let mut start_line = grid_line;
        while start_line > -history
            && grid_line - start_line < MAX_WRAP_SCAN
            && wraps(start_line - 1)
        {
            start_line -= 1;
        }
        let mut end_line = grid_line;
        while end_line < rows - 1 && end_line - grid_line < MAX_WRAP_SCAN && wraps(end_line) {
            end_line += 1;
        }

        let from = Pos::new(Line(start_line), PosColumn(0));
        let to = Pos::new(Line(end_line), PosColumn(cols - 1));
        for m in RegexIter::new(from, to, Direction::Right, &term, regex) {
            let (s, mut e) = (*m.start(), *m.end());
            if point < s {
                break;
            }
            let text = term.bounds_to_string(s, e);
            // The pattern is greedy about trailing punctuation; prose is
            // not: `https://rio.dev,` links without the comma, and a paren
            // only belongs to the URL when it was opened inside it.
            let trim = trailing_url_punctuation(&text);
            for _ in 0..trim {
                if e.col.0 == 0 {
                    e.row.0 -= 1;
                    e.col = PosColumn(cols - 1);
                } else {
                    e.col.0 -= 1;
                }
            }
            if point > e {
                continue;
            }
            let uri: String = {
                let chars = text.chars().count() - trim;
                text.chars().take(chars).collect()
            };
            let start_col = if s.row.0 < grid_line {
                0
            } else {
                s.col.0 as u16
            };
            let end_col = if e.row.0 > grid_line {
                (cols - 1) as u16
            } else {
                e.col.0 as u16
            };
            return Some((uri, start_col, end_col));
        }
        None
    }

    /// Dump the whole buffer (scrollback + screen) to plain text, so a
    /// frontend can persist it and replay it as inert scrollback on
    /// restore. Trailing blank rows are trimmed by `bounds_to_string`.
    pub fn dump(&self) -> String {
        let term = self.terminal.lock();
        let rows = term.screen_lines() as i32;
        let cols = term.columns();
        if rows == 0 || cols == 0 {
            return String::new();
        }
        let history = term.history_size() as i32;
        // Scrollback lives in negative line coordinates above the screen.
        let start = Pos::new(Line(-history), PosColumn(0));
        let end = Pos::new(Line(rows - 1), PosColumn(cols - 1));
        term.bounds_to_string(start, end)
    }

    pub(crate) fn terminal(&self) -> Arc<TerminalLock<Crosswords<Listener>>> {
        self.terminal.clone()
    }

    pub(crate) fn inline_images(&self) -> InlineImages {
        self.inline_images.clone()
    }

    pub(crate) fn cursor_shaping_break(&self) -> Arc<std::sync::atomic::AtomicBool> {
        self.cursor_shaping_break.clone()
    }
}

#[cfg(feature = "pty")]
impl Drop for Surface {
    fn drop(&mut self) {
        let _ = self.channel.send(Msg::Shutdown);
        // Also hang up synchronously: an embedder may exit the process
        // right after dropping the surface, before the reader thread can
        // run its shutdown escalation. The handle is a no-op once the
        // child was reaped, so no stale PID is ever signaled.
        #[cfg(not(target_os = "windows"))]
        let _ = self.child_terminator.hangup();
    }
}
