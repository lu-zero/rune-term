//! tmux control mode (`tmux -C`): one line per notification, and a
//! `%begin` … `%end` block per command. Lines are bytes: `%output` escapes
//! control bytes and backslash, but passes bytes above 0x7f through raw.
//! rmux writes `%output` inside a block too, so there it is output, not reply.

use crate::layout::{Axis, Layout};

mod client;

pub use client::{
    capture, kill_window, new_window, panes, refresh_client, resize_pane, select, send_keys, sizes,
    split_window, Client, Update, Window, LIST_WINDOWS,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The answer to one command, in the order the commands were sent.
    /// `client` is false for the block tmux writes for the attach itself.
    Reply {
        ok: bool,
        client: bool,
        lines: Vec<Vec<u8>>,
    },
    Output {
        pane: u32,
        bytes: Vec<u8>,
    },
    WindowAdd(u32),
    WindowClose(u32),
    WindowRenamed {
        window: u32,
        name: String,
    },
    Layout {
        window: u32,
        layout: Layout,
    },
    /// The active pane of a window.
    WindowPaneChanged {
        window: u32,
        pane: u32,
    },
    SessionChanged {
        session: u32,
        name: String,
    },
    SessionRenamed(String),
    /// The active window of a session.
    SessionWindowChanged {
        session: u32,
        window: u32,
    },
    Exit(Option<String>),
}

#[derive(Default)]
pub struct Parser {
    partial: Vec<u8>,
    block: Option<Block>,
}

struct Block {
    client: bool,
    lines: Vec<Vec<u8>>,
}

impl Parser {
    /// Bytes as they arrive from tmux's stdout; a line split across two reads
    /// waits for its end.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        self.partial.extend_from_slice(bytes);
        let mut start = 0;
        while let Some(end) = self.partial[start..].iter().position(|b| *b == b'\n') {
            let line = self.partial[start..start + end].to_vec();
            start += end + 1;
            if let Some(event) = self.line(&line) {
                events.push(event);
            }
        }
        self.partial.drain(..start);
        events
    }

    fn line(&mut self, line: &[u8]) -> Option<Event> {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if let Some(rest) = line.strip_prefix(b"%output ") {
            let (pane, value) = split_word(rest);
            if let Some(pane) = id(pane, b'%') {
                return Some(Event::Output {
                    pane,
                    bytes: unescape(value),
                });
            }
        }
        if let Some(block) = &mut self.block {
            // A command's own output may start with `%` (`list-panes` prints `%0`).
            let ok = if line.starts_with(b"%end ") {
                true
            } else if line.starts_with(b"%error ") {
                false
            } else {
                block.lines.push(line.to_vec());
                return None;
            };
            let block = self.block.take()?;
            return Some(Event::Reply {
                ok,
                client: block.client,
                lines: block.lines,
            });
        }
        let text = String::from_utf8_lossy(line);
        let (name, rest) = text.split_once(' ').unwrap_or((&text, ""));
        let mut args = rest.split(' ');
        match name {
            "%begin" => {
                let flags = args.nth(2).unwrap_or("0");
                self.block = Some(Block {
                    client: flags != "0",
                    lines: Vec::new(),
                });
                None
            }
            "%window-add" => Some(Event::WindowAdd(id(args.next()?.as_bytes(), b'@')?)),
            // A killed window can be unlinked from the session before it closes.
            "%window-close" | "%unlinked-window-close" => {
                Some(Event::WindowClose(id(args.next()?.as_bytes(), b'@')?))
            }
            "%window-renamed" => {
                let (window, name) = rest.split_once(' ').unwrap_or((rest, ""));
                Some(Event::WindowRenamed {
                    window: id(window.as_bytes(), b'@')?,
                    name: name.to_owned(),
                })
            }
            "%layout-change" => Some(Event::Layout {
                window: id(args.next()?.as_bytes(), b'@')?,
                layout: parse_layout(args.next()?)?,
            }),
            "%window-pane-changed" => Some(Event::WindowPaneChanged {
                window: id(args.next()?.as_bytes(), b'@')?,
                pane: id(args.next()?.as_bytes(), b'%')?,
            }),
            "%session-changed" => {
                let (session, name) = rest.split_once(' ').unwrap_or((rest, ""));
                Some(Event::SessionChanged {
                    session: id(session.as_bytes(), b'$')?,
                    name: name.to_owned(),
                })
            }
            "%session-renamed" => Some(Event::SessionRenamed(rest.to_owned())),
            "%session-window-changed" => Some(Event::SessionWindowChanged {
                session: id(args.next()?.as_bytes(), b'$')?,
                window: id(args.next()?.as_bytes(), b'@')?,
            }),
            "%exit" => Some(Event::Exit((!rest.is_empty()).then(|| rest.to_owned()))),
            _ => None,
        }
    }
}

fn split_word(bytes: &[u8]) -> (&[u8], &[u8]) {
    match bytes.iter().position(|b| *b == b' ') {
        Some(at) => (&bytes[..at], &bytes[at + 1..]),
        None => (bytes, &[]),
    }
}

/// `%3`, `@1`, `$0`.
fn id(word: &[u8], sigil: u8) -> Option<u32> {
    std::str::from_utf8(word.strip_prefix(&[sigil])?)
        .ok()?
        .parse()
        .ok()
}

/// `\ooo` is one byte. Everything else is itself.
fn unescape(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    let mut i = 0;
    while i < value.len() {
        if value[i] == b'\\' {
            if let Some(code) = value.get(i + 1..i + 4).and_then(octal) {
                out.push(code);
                i += 4;
                continue;
            }
        }
        out.push(value[i]);
        i += 1;
    }
    out
}

fn octal(digits: &[u8]) -> Option<u8> {
    digits.iter().try_fold(0u8, |acc, d| match d {
        b'0'..=b'7' => acc.checked_mul(8)?.checked_add(d - b'0'),
        _ => None,
    })
}

/// `d67e,80x24,0,0{40x24,0,0,0,39x24,41,0[…]}`: a checksum, then the root.
/// A cell is `WxH,X,Y` followed by `,id` for a pane, `{…}` for side by side,
/// or `[…]` for stacked.
pub fn parse_layout(text: &str) -> Option<Layout> {
    let (_checksum, root) = text.split_once(',')?;
    let mut rest = root.as_bytes();
    let layout = cell(&mut rest)?;
    rest.is_empty().then_some(layout)
}

fn cell(rest: &mut &[u8]) -> Option<Layout> {
    let cols = number(rest)?;
    expect(rest, b'x')?;
    let rows = number(rest)?;
    // The offset is implied: children follow each other with one-cell dividers.
    expect(rest, b',')?;
    number::<u16>(rest)?;
    expect(rest, b',')?;
    number::<u16>(rest)?;
    let (axis, close) = match rest.first() {
        Some(b',') => {
            expect(rest, b',')?;
            let id = number(rest)?;
            return Some(Layout::Pane { id, cols, rows });
        }
        Some(b'{') => (Axis::Horizontal, b'}'),
        Some(b'[') => (Axis::Vertical, b']'),
        _ => return None,
    };
    *rest = &rest[1..];
    let mut children = vec![cell(rest)?];
    while rest.first() == Some(&b',') {
        *rest = &rest[1..];
        children.push(cell(rest)?);
    }
    expect(rest, close)?;
    Some(Layout::Split {
        axis,
        cols,
        rows,
        children,
    })
}

fn number<T: std::str::FromStr>(rest: &mut &[u8]) -> Option<T> {
    let len = rest.iter().take_while(|b| b.is_ascii_digit()).count();
    let value = std::str::from_utf8(&rest[..len]).ok()?.parse().ok()?;
    *rest = &rest[len..];
    Some(value)
}

fn expect(rest: &mut &[u8], byte: u8) -> Option<()> {
    let (first, tail) = rest.split_first()?;
    if *first != byte {
        return None;
    }
    *rest = tail;
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: u32, cols: u16, rows: u16) -> Layout {
        Layout::Pane { id, cols, rows }
    }

    #[test]
    fn a_nested_layout_from_tmux_3_4_parses() {
        let layout =
            parse_layout("d67e,80x24,0,0{40x24,0,0,0,39x24,41,0[39x12,41,0,1,39x11,41,13,2]}");
        assert_eq!(
            layout,
            Some(Layout::Split {
                axis: Axis::Horizontal,
                cols: 80,
                rows: 24,
                children: vec![
                    pane(0, 40, 24),
                    Layout::Split {
                        axis: Axis::Vertical,
                        cols: 39,
                        rows: 24,
                        children: vec![pane(1, 39, 12), pane(2, 39, 11)],
                    },
                ],
            })
        );
    }

    #[test]
    fn a_truncated_layout_is_refused() {
        assert_eq!(parse_layout("d67e,80x24,0,0{40x24,0,0,0"), None);
        assert_eq!(parse_layout("b25f,80x24,0,0,2junk"), None);
    }

    #[test]
    fn output_unescapes_and_keeps_raw_high_bytes() {
        let mut parser = Parser::default();
        let mut line = b"%output %0 hi\\015\\012\\134 \xe2\x9d\xaf\n".to_vec();
        line.extend_from_slice(b"%output %12 \\033[K\n");
        assert_eq!(
            parser.feed(&line),
            vec![
                Event::Output {
                    pane: 0,
                    bytes: b"hi\r\n\\ \xe2\x9d\xaf".to_vec(),
                },
                Event::Output {
                    pane: 12,
                    bytes: b"\x1b[K".to_vec(),
                },
            ]
        );
    }

    #[test]
    fn output_inside_a_reply_is_output_as_rmux_writes_it() {
        let mut parser = Parser::default();
        let events = parser
            .feed(b"%begin 1 4 1\n%output %0 \\015\\033[K\nrune-capture %0 2 2\n%end 1 4 1\n");
        assert_eq!(
            events,
            vec![
                Event::Output {
                    pane: 0,
                    bytes: b"\r\x1b[K".to_vec(),
                },
                Event::Reply {
                    ok: true,
                    client: true,
                    lines: vec![b"rune-capture %0 2 2".to_vec()],
                },
            ]
        );
    }

    #[test]
    fn a_line_split_across_reads_waits_for_its_end() {
        let mut parser = Parser::default();
        assert_eq!(parser.feed(b"%window-ad"), vec![]);
        assert_eq!(parser.feed(b"d @3\n%win"), vec![Event::WindowAdd(3)]);
        assert_eq!(parser.feed(b"dow-close @3\n"), vec![Event::WindowClose(3)]);
    }

    #[test]
    fn a_reply_keeps_its_percent_lines_and_knows_the_attach_block() {
        let mut parser = Parser::default();
        let events = parser.feed(
            b"%begin 1794742135 279 0\n%end 1794742135 279 0\n\
              %session-changed $0 my session\n\
              %begin 1794742135 284 1\n%0\n%1\n%end 1794742135 284 1\n\
              %begin 1794742135 285 1\nno such pane\n%error 1794742135 285 1\n",
        );
        assert_eq!(
            events,
            vec![
                Event::Reply {
                    ok: true,
                    client: false,
                    lines: vec![],
                },
                Event::SessionChanged {
                    session: 0,
                    name: "my session".to_owned(),
                },
                Event::Reply {
                    ok: true,
                    client: true,
                    lines: vec![b"%0".to_vec(), b"%1".to_vec()],
                },
                Event::Reply {
                    ok: false,
                    client: true,
                    lines: vec![b"no such pane".to_vec()],
                },
            ]
        );
    }

    #[test]
    fn a_layout_change_carries_the_window_layout() {
        let mut parser = Parser::default();
        let events = parser.feed(
            b"%layout-change @0 77dd,100x30,0,0,0 77dd,100x30,0,0,0 *\n\
              %window-renamed @0 vim notes.md\n%exit\n%exit server exited\n",
        );
        assert_eq!(
            events,
            vec![
                Event::Layout {
                    window: 0,
                    layout: pane(0, 100, 30),
                },
                Event::WindowRenamed {
                    window: 0,
                    name: "vim notes.md".to_owned(),
                },
                Event::Exit(None),
                Event::Exit(Some("server exited".to_owned())),
            ]
        );
    }
}
