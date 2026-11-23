//! The client side of tmux control mode: what to send, and the windows and
//! pane output the replies and notifications add up to.

use super::{parse_layout, Event, Parser};
use crate::layout::{Axis, Layout};

/// The first line of a window listing. tmux numbers its replies but does not
/// say which command they answer, so each listing names itself.
const WINDOW: &str = "rune-window ";
/// Announces the `capture-pane` reply that follows it on the same line.
const CAPTURE: &str = "rune-capture ";

pub const LIST_WINDOWS: &str =
    "list-windows -F \"rune-window #{window_id} #{window_index} #{window_active} \
                                #{pane_id} #{window_layout} #{window_name}\"\n";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub id: u32,
    /// The number tmux shows for the window, which the user can change.
    pub index: u32,
    pub name: String,
    /// The session's current window.
    pub active: bool,
    pub active_pane: u32,
    pub layout: Layout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Update {
    /// Bytes for a pane's terminal, in the order tmux wrote them.
    Output {
        pane: u32,
        bytes: Vec<u8>,
    },
    /// Every window of the attached session, in tmux's order.
    Windows(Vec<Window>),
    Session(String),
    Exit(Option<String>),
}

#[derive(Default)]
pub struct Client {
    parser: Parser,
    windows: Vec<Window>,
    /// Redraw every pane from the next listing: their screens predate this client.
    capture_all: bool,
    /// The pane the next reply is a capture of, and its cursor.
    capture: Option<(u32, u16, u16)>,
    commands: String,
}

impl Client {
    /// Advance the parser with control-mode bytes; returns the updates.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Update> {
        let mut updates = Vec::new();
        for event in self.parser.feed(bytes) {
            self.event(event, &mut updates);
        }
        updates
    }

    /// Commands the notifications called for, to write to tmux's stdin.
    pub fn take_commands(&mut self) -> String {
        std::mem::take(&mut self.commands)
    }

    fn event(&mut self, event: Event, updates: &mut Vec<Update>) {
        match event {
            Event::Reply { client: false, .. } => {}
            Event::Reply { ok, lines, .. } => {
                if let Some((pane, x, y)) = self.capture.take() {
                    if ok {
                        updates.push(Update::Output {
                            pane,
                            bytes: redraw(&lines, x, y),
                        });
                    }
                    return;
                }
                let Some(first) = lines.first() else {
                    return;
                };
                if let Some(rest) = first.strip_prefix(CAPTURE.as_bytes()) {
                    self.capture = parse_capture(rest);
                } else if first.starts_with(WINDOW.as_bytes()) {
                    self.windows = lines.iter().filter_map(|line| parse_window(line)).collect();
                    if std::mem::take(&mut self.capture_all) {
                        for window in &self.windows {
                            for pane in panes(&window.layout) {
                                self.commands.push_str(&capture(pane));
                            }
                        }
                    }
                    updates.push(Update::Windows(self.windows.clone()));
                }
            }
            Event::Output { pane, bytes } => updates.push(Update::Output { pane, bytes }),
            Event::WindowAdd(_) => self.commands.push_str(LIST_WINDOWS),
            Event::SessionChanged { name, .. } => {
                self.capture_all = true;
                self.commands.push_str(LIST_WINDOWS);
                updates.push(Update::Session(name));
            }
            Event::SessionRenamed(name) => updates.push(Update::Session(name)),
            Event::WindowClose(id) => {
                let before = self.windows.len();
                self.windows.retain(|window| window.id != id);
                if self.windows.len() != before {
                    updates.push(Update::Windows(self.windows.clone()));
                }
            }
            Event::WindowRenamed { window, name } => {
                self.change(window, updates, |w| w.name = name);
            }
            Event::Layout { window, layout } => {
                // A resized pane is redrawn from tmux: this copy's own reflow can
                // pull back history tmux does not.
                if let Some(old) = self.windows.iter().find(|one| one.id == window) {
                    let before = sizes(&old.layout);
                    for (pane, size) in sizes(&layout) {
                        if before.iter().any(|(id, was)| *id == pane && *was != size) {
                            self.commands.push_str(&capture(pane));
                        }
                    }
                }
                self.change(window, updates, |w| w.layout = layout);
            }
            Event::WindowPaneChanged { window, pane } => {
                self.change(window, updates, |w| w.active_pane = pane);
            }
            Event::SessionWindowChanged { window, .. } => {
                for one in &mut self.windows {
                    one.active = one.id == window;
                }
                updates.push(Update::Windows(self.windows.clone()));
            }
            Event::Exit(reason) => updates.push(Update::Exit(reason)),
        }
    }

    /// A window the listing has not brought yet waits for it.
    fn change(&mut self, id: u32, updates: &mut Vec<Update>, f: impl FnOnce(&mut Window)) {
        if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
            f(window);
            updates.push(Update::Windows(self.windows.clone()));
        }
    }
}

/// `@0 1 %0 a87d,100x30,0,0,0 vim notes.md` after the marker.
fn parse_window(line: &[u8]) -> Option<Window> {
    let text = std::str::from_utf8(line.strip_prefix(WINDOW.as_bytes())?).ok()?;
    let mut words = text.splitn(6, ' ');
    let id = words.next()?.strip_prefix('@')?.parse().ok()?;
    let index = words.next()?.parse().ok()?;
    let active = words.next()? == "1";
    let active_pane = words.next()?.strip_prefix('%')?.parse().ok()?;
    let layout = parse_layout(words.next()?)?;
    let name = words.next().unwrap_or("").to_owned();
    Some(Window {
        id,
        index,
        name,
        active,
        active_pane,
        layout,
    })
}

/// `%3 12 4`. A pane that is gone prints its formats empty.
fn parse_capture(rest: &[u8]) -> Option<(u32, u16, u16)> {
    let text = std::str::from_utf8(rest).ok()?;
    let mut words = text.split(' ');
    let pane = words.next()?.strip_prefix('%')?.parse().ok()?;
    Some((
        pane,
        words.next()?.parse().ok()?,
        words.next()?.parse().ok()?,
    ))
}

/// The captured screen, then the cursor where tmux has it. ED 0 from the
/// top: ED 2 would push the old screen into this copy's history.
fn redraw(lines: &[Vec<u8>], x: u16, y: u16) -> Vec<u8> {
    let mut bytes = b"\x1b[H\x1b[J".to_vec();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(line);
    }
    bytes.extend_from_slice(format!("\x1b[0m\x1b[{};{}H", y + 1, x + 1).as_bytes());
    bytes
}

/// Pane ids in stream order from a layout.
pub fn panes(layout: &Layout) -> Vec<u32> {
    sizes(layout).into_iter().map(|(pane, _)| pane).collect()
}

/// Each pane and its size in cells.
pub fn sizes(layout: &Layout) -> Vec<(u32, (u16, u16))> {
    match layout {
        Layout::Pane { id, cols, rows } => vec![(*id, (*cols, *rows))],
        Layout::Split { children, .. } => children.iter().flat_map(sizes).collect(),
    }
}

/// The pane's screen as it is now, announced by its marker. Two lines, not a
/// `;` list: rmux answers a list in one block.
pub fn capture(pane: u32) -> String {
    format!(
        "display -p -t %{pane} \"rune-capture #{{pane_id}} #{{cursor_x}} #{{cursor_y}}\"\n\
         capture-pane -p -e -t %{pane}\n"
    )
}

/// Input for a pane, as hex so no byte is taken for tmux syntax. Long input is
/// several commands.
pub fn send_keys(pane: u32, bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(256) {
        out.push_str(&format!("send-keys -t %{pane} -H"));
        for byte in chunk {
            out.push_str(&format!(" {byte:02x}"));
        }
        out.push('\n');
    }
    out
}

/// Command reporting the cell size to tmux.
pub fn refresh_client(cols: u16, rows: u16) -> String {
    format!("refresh-client -C {cols}x{rows}\n")
}

/// Command selecting a window and pane.
pub fn select(window: u32, pane: u32) -> String {
    format!("select-window -t @{window} ; select-pane -t %{pane}\n")
}

/// The pane's width for a side-by-side split, or its height for a stacked one.
/// tmux moves the pane's right or bottom edge, and the column or row it heads
/// goes with it.
pub fn resize_pane(pane: u32, axis: Axis, cells: u16) -> String {
    let flag = match axis {
        Axis::Horizontal => 'x',
        Axis::Vertical => 'y',
    };
    format!("resize-pane -t %{pane} -{flag} {cells}\n")
}

/// Command selecting a window with a layout.
pub fn select_layout(window: u32, layout: &Layout) -> String {
    format!(
        "select-layout -t @{window} '{}'\n",
        super::layout_text(layout)
    )
}

/// Command closing a pane.
pub fn kill_pane(pane: u32) -> String {
    format!("kill-pane -t %{pane}\n")
}

/// Command closing a window.
pub fn kill_window(window: u32) -> String {
    format!("kill-window -t @{window}\n")
}

/// Command creating a window.
pub fn new_window() -> String {
    "new-window\n".to_owned()
}

/// Command splitting a pane on an axis.
pub fn split_window(pane: u32, axis: Axis) -> String {
    let flag = match axis {
        Axis::Horizontal => 'h',
        Axis::Vertical => 'v',
    };
    format!("split-window -{flag} -t %{pane}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: u32, cols: u16, rows: u16) -> Layout {
        Layout::Pane { id, cols, rows }
    }

    /// tmux 3.4 attaching to a session with one window, as a pipe sees it.
    const ATTACH: &[u8] = b"%begin 1794750235 265 0\n%end 1794750235 265 0\n\
        %window-add @0\n%sessions-changed\n%session-changed $0 probe\n";

    fn attached() -> Client {
        let mut client = Client::default();
        assert_eq!(
            client.feed(ATTACH),
            vec![Update::Session("probe".to_owned())]
        );
        assert_eq!(
            client.take_commands(),
            format!("{LIST_WINDOWS}{LIST_WINDOWS}")
        );
        client
    }

    #[test]
    fn attaching_lists_the_windows_and_redraws_every_pane_once() {
        let mut client = attached();
        let windows = client.feed(
            b"%begin 1 274 1\nrune-window @0 0 1 %1 6b8b,100x30,0,0{50x30,0,0,0,49x30,51,0,1} vim a b\n\
              %end 1 274 1\n",
        );
        let first = Window {
            id: 0,
            index: 0,
            name: "vim a b".to_owned(),
            active: true,
            active_pane: 1,
            layout: Layout::Split {
                axis: Axis::Horizontal,
                cols: 100,
                rows: 30,
                children: vec![pane(0, 50, 30), pane(1, 49, 30)],
            },
        };
        assert_eq!(windows, vec![Update::Windows(vec![first.clone()])]);
        assert_eq!(
            client.take_commands(),
            format!("{}{}", capture(0), capture(1))
        );
        // The second listing the attach asked for redraws nothing.
        client.feed(
            b"%begin 1 275 1\nrune-window @0 0 1 %1 6b8b,100x30,0,0,0 vim a b\n%end 1 275 1\n",
        );
        assert_eq!(client.take_commands(), "");
    }

    #[test]
    fn a_capture_redraws_the_pane_under_its_cursor() {
        let mut client = attached();
        let updates = client.feed(
            b"%begin 1 272 1\nrune-capture %0 9 1\n%end 1 272 1\n\
              %begin 1 273 1\na\tb\n\x1b[1mc\n%end 1 273 1\n",
        );
        assert_eq!(
            updates,
            vec![Update::Output {
                pane: 0,
                bytes: b"\x1b[H\x1b[Ja\tb\r\n\x1b[1mc\x1b[0m\x1b[2;10H".to_vec(),
            }]
        );
    }

    #[test]
    fn a_capture_of_a_pane_that_is_gone_is_not_drawn() {
        let mut client = attached();
        let updates = client.feed(
            b"%begin 1 275 1\nrune-capture   \n%end 1 275 1\n\
              %begin 1 276 1\ncan't find pane: %9\n%error 1 276 1\n",
        );
        assert_eq!(updates, vec![]);
    }

    #[test]
    fn a_new_window_waits_for_its_listing_and_a_close_drops_it() {
        let mut client = attached();
        client.feed(b"%begin 1 274 1\nrune-window @0 0 1 %0 a87d,80x24,0,0,0 sh\n%end 1 274 1\n");
        client.take_commands();
        // No layout is reported for a new window, and a change to it waits too.
        assert_eq!(
            client.feed(b"%session-window-changed $0 @1\n%window-add @1\n%window-renamed @1 top\n"),
            vec![Update::Windows(vec![Window {
                id: 0,
                index: 0,
                name: "sh".to_owned(),
                active: false,
                active_pane: 0,
                layout: pane(0, 80, 24),
            }])]
        );
        assert_eq!(client.take_commands(), LIST_WINDOWS);
        // Another session's window is not this one's to drop.
        assert_eq!(client.feed(b"%unlinked-window-close @7\n"), vec![]);
        let Some(Update::Windows(windows)) = client.feed(b"%unlinked-window-close @0\n").pop()
        else {
            panic!("a close is a new window list");
        };
        assert!(windows.is_empty());
    }

    #[test]
    fn input_is_hex_in_commands_of_at_most_256_bytes() {
        assert_eq!(send_keys(3, b"a;\n"), "send-keys -t %3 -H 61 3b 0a\n");
        let long = send_keys(0, &[b'x'; 300]);
        assert_eq!(long.lines().count(), 2);
        assert_eq!(long.lines().nth(1).unwrap().matches(" 78").count(), 44);
    }

    #[test]
    fn a_layout_that_resizes_a_pane_redraws_it_from_tmux() {
        let mut client = attached();
        client.feed(
            b"%begin 1 274 1\nrune-window @0 0 1 %1 6b8b,100x30,0,0{50x30,0,0,0,49x30,51,0,1} sh\n\
              %end 1 274 1\n",
        );
        client.take_commands();
        client.feed(b"%layout-change @0 6b8b,100x30,0,0{30x30,0,0,0,69x30,31,0,1} x *\n");
        assert_eq!(
            client.take_commands(),
            format!("{}{}", capture(0), capture(1))
        );
        // The same sizes again redraw nothing.
        client.feed(b"%layout-change @0 6b8b,100x30,0,0{30x30,0,0,0,69x30,31,0,1} x *\n");
        assert_eq!(client.take_commands(), "");
    }
}
