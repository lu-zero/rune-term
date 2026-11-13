//! OSC 8 hyperlinks and implicitly detected URLs.

#[cfg(all(test, not(feature = "pty")))]
pub use rune_term_core::hyperlink::{capture_opens, take_captured_opens};
#[allow(unused_imports)]
pub use rune_term_core::hyperlink::{
    open_decision, open_mods, open_mods_rune, open_uri, open_uri_with, resolve, tint, HoverLink,
    OpenDecision, Tint,
};

#[cfg(all(test, not(feature = "pty")))]
mod tests {
    use super::*;
    use crate::delegate;
    use crate::grid_source::SurfaceGrid;
    use crate::model;
    use librio::Modifiers;
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
        let palette = model.palette;
        let host = model.surfaces.primary_mut().unwrap();
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
    fn echo_output_url_is_detected() {
        let mut model = host();
        {
            let host = model.surfaces.primary_mut().unwrap();
            host.surface.inject_output(b"https://example.com\r\n");
            host.render.update();
        }
        let host = model.surfaces.primary().unwrap();
        let link = resolve(host, 0, 0).expect("url from echo output");
        assert!(link.uri.starts_with("https://example.com"), "{}", link.uri);
        assert_eq!(link.start, 0);
        assert!(link.end >= 18);
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
