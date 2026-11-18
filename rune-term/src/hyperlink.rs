//! OSC 8 hyperlinks and implicitly detected URLs.

#[cfg(all(test, not(feature = "pty")))]
pub use rune_term_core::hyperlink::{capture_opens, take_captured_opens, HoverLink};
pub use rune_term_core::hyperlink::{open_mods, open_uri};

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::grid_source::SurfaceGrid;
    use crate::model;
    use librio::Modifiers;
    use rune_term_core::hyperlink::{open_decision, resolve, tint, OpenDecision};
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
            let host = model.terminal.primary_mut().unwrap();
            host.surface
                .inject_output(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07");
            host.render.update();
        }
        let host = model.terminal.primary().unwrap();
        let link = resolve(host, 0, 1).expect("osc8 link");
        assert_eq!(link.uri, "https://example.com");
        assert_eq!(link.start, 0);
        assert!(link.end >= 3);
    }

    #[test]
    fn hover_tint_underlines_the_run() {
        let mut model = host();
        {
            let host = model.terminal.primary_mut().unwrap();
            host.surface.inject_output(b"plain text");
            host.render.update();
        }
        let palette = model.palette;
        let accent = palette.accent;
        let host = model.terminal.primary_mut().unwrap();
        let hover = HoverLink {
            line: 0,
            start: 0,
            end: 4,
            uri: "https://example.com".to_string(),
        };
        let source = SurfaceGrid {
            render: &mut host.render,
            palette,
        };
        let tinted = tint(source, Some(&hover), accent);
        assert!(tinted.cell(0, 0).underline);
        assert!(tinted.cell(0, 4).underline);
        assert_eq!(tinted.cell(0, 0).fg[..3], accent.to_be_bytes()[1..]);
        assert!(!tinted.cell(0, 6).underline);
    }

    #[test]
    fn implicit_url_resolves_when_no_osc8_link() {
        let mut model = host();
        {
            let host = model.terminal.primary_mut().unwrap();
            host.surface.inject_output(b"go https://example.com ok");
            host.render.update();
        }
        let host = model.terminal.primary().unwrap();
        let link = resolve(host, 0, 5).expect("implicit url");
        assert!(link.uri.starts_with("https://example.com"), "{}", link.uri);
    }

    #[test]
    fn echo_output_url_is_detected() {
        let mut model = host();
        {
            let host = model.terminal.primary_mut().unwrap();
            host.surface.inject_output(b"https://example.com\r\n");
            host.render.update();
        }
        let host = model.terminal.primary().unwrap();
        let link = resolve(host, 0, 0).expect("url from echo output");
        assert!(link.uri.starts_with("https://example.com"), "{}", link.uri);
        assert_eq!(link.start, 0);
        assert!(link.end >= 18);
    }

    #[test]
    fn web_links_open_others_ask_and_unopenable_ones_are_refused() {
        assert_eq!(open_decision("https://example.com"), OpenDecision::Open);
        assert_eq!(open_decision("mailto:test@example.com"), OpenDecision::Open);
        assert_eq!(open_decision("ssh://host"), OpenDecision::Confirm);
        assert_eq!(open_decision("file:///tmp/x"), OpenDecision::Confirm);
        assert_eq!(
            open_decision("file://localhost/tmp/x"),
            OpenDecision::Confirm
        );
        assert_eq!(
            open_decision("file://elsewhere/tmp/x"),
            OpenDecision::Refuse
        );
        assert_eq!(open_decision("-a Calculator"), OpenDecision::Refuse);
        assert_eq!(open_decision("/tmp/x:1"), OpenDecision::Refuse);
    }
}
