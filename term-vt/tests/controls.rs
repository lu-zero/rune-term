use std::sync::Arc;
use term_vt::{Engine, RenderState, SurfaceDelegate, SurfaceDesc, SurfaceId};

struct Quiet;
impl SurfaceDelegate for Quiet {
    fn wakeup(&self, _: SurfaceId) {}
}

fn cursor_after(bytes: &[u8]) -> (usize, usize) {
    let engine = Engine::new(Arc::new(Quiet));
    let surface = engine
        .create_surface(&SurfaceDesc {
            cols: 20,
            rows: 4,
            ..SurfaceDesc::default()
        })
        .expect("surface");
    surface.inject_output(bytes);
    let mut render = RenderState::new(&surface);
    render.update();
    render.cursor()
}

#[test]
fn tab_moves_to_the_next_stop_without_erasing() {
    assert_eq!(cursor_after(b"abc\tX"), (0, 9));
    assert_eq!(cursor_after(b"\x1b[1;4H\x1b[0gY"), (0, 4));
}

#[test]
fn carriage_return_backspace_and_vertical_moves() {
    assert_eq!(cursor_after(b"A\tB\rC\nD\x08E"), (1, 2));
    assert_eq!(cursor_after(b"\x1b[1;1Hone\x0btwo\x0cthree"), (2, 11));
}
