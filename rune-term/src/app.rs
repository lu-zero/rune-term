//! Window shell. `perform_resize` is frozen; the winsize change does not edit it.

use std::collections::HashMap;

use rune::compose::DeviceKey;
#[cfg(all(unix, feature = "pty"))]
use rune::fast_exit;
use rune::render::{Instance, KIND_EFFECT};
use rune::{App, AppSpec, Compositor, Frame, Titlebar, Tx};
use rune_term_core::terminal::PaneGrid;
use term_grid::images::ImageBucket;
use term_grid::{DrawSlot, GridRenderer, SolidSlot};
use term_vt::SurfaceId;

use crate::chrome;
use crate::delegate;
use crate::grid_pass;
use crate::images::Images;
use crate::ime;
use crate::model::{self, Model};
use crate::pane_input;
use crate::resize::{self, ResizeEffect};
use crate::scrollback::{self, Bars};

/// Open the terminal window and run the event loop.
pub fn run() {
    with_settings_window(app()).run();
}

/// Run with explicit arguments forwarded to Rune's tooling layer, so Rune does
/// not parse the process argv.
pub fn run_with_rune_args(args: impl IntoIterator<Item = String>) {
    with_settings_window(app().args(args)).run();
}

/// Adds the settings window. It is the terminal's to open, and it holds the
/// terminal's handle so an applied change reaches the running grid.
fn with_settings_window(mut app: App<Model>) -> App<Model> {
    let terminal = app.handle();
    let menu = app.handle();
    let (settings, _window) = app.window(crate::settings_ui::window(terminal));
    app.init(move || {
        let mut model = model::init();
        #[cfg(all(unix, feature = "pty"))]
        if crate::handover::requested() {
            crate::handover::adopt(&mut model);
        }
        model.settings = Some(settings.clone());
        model.terminal_handle = Some(menu.clone());
        crate::mux::prefetch(&model);
        term_grid::monospace_families();
        model
    })
}

/// Build the app with its settings window attached.
pub fn app() -> App<Model> {
    #[cfg(all(unix, feature = "pty"))]
    fast_exit(delegate::handover_all);
    App::new(spec())
        .compositor(GridCompositor::default())
        .on_ime(ime::on_ime)
        .on_modifiers(|model, mods, tx| {
            if model
                .terminal
                .set_held_mods(rune_term_core::input::mods_of(mods))
                && model.terminal.any_hover_link()
            {
                tx.wake_after(std::time::Duration::ZERO);
            }
        })
        .titlebar(Titlebar::new(chrome::TITLEBAR_H))
}

fn spec() -> AppSpec<Model> {
    AppSpec {
        setup: Some(delegate::setup),
        poll: Some(delegate::poll),
        on_key: Some(pane_input::on_key),
        ..AppSpec::new("rune-term", (800.0, 500.0), model::init, chrome::view)
    }
}

/// Applies `window_points`, terminal surface resize, and `live_only`.
/// The caller stores `cols`, `rows`, and `cell_px` on the host when needed.
pub fn perform_resize(tx: &mut Tx, host: &crate::surfaces::HostSurface, effect: ResizeEffect) {
    if let Some(size) = effect.window_points {
        tx.resize_window_instant(size);
    }
    rune_term_core::resize::apply_to_host(host, &effect);
    if effect.live_only {
        tx.live_only();
    }
}

pub fn on_pane_layout(model: &mut Model, slot: (f32, f32), scale: f32, tx: &mut Tx) {
    if scale.is_finite() && scale > 0.0 {
        model.scale = scale;
    }
    let Some(effect) = fit_layout(model, slot, scale) else {
        return;
    };
    let Some(host) = model.terminal.primary() else {
        return;
    };
    // `fit_layout` already resized the surface. `perform_resize` still owns
    // the window snap and `live_only`.
    let mut delivered = effect;
    delivered.resize_surface = false;
    perform_resize(tx, host, delivered);
}

/// Floors the pane and resizes the surface when the cell count changed.
/// The layout callback reports the rect on the following frame, so the
/// paint path calls this with the rect it is about to draw.
pub fn fit_layout(model: &mut Model, slot: (f32, f32), scale: f32) -> Option<ResizeEffect> {
    if scale.is_finite() && scale > 0.0 {
        model.scale = scale;
    }
    let cell_points = model.cell_points;
    let remote = model
        .terminal
        .primary()
        .is_some_and(|host| crate::mux::owns(model, host.surface.id()));
    let host = model.terminal.primary_mut()?;
    if !remote {
        return Some(rune_term_core::resize::fit_layout_host(
            host,
            slot,
            scale,
            cell_points,
        ));
    }
    // tmux sizes its panes. The fitted size is what gets reported to it.
    let effect = resize::apply(resize::ResizeRequest::Layout {
        slot_points: slot,
        scale,
        cell_points,
        current: resize::GridMetrics {
            cols: host.cols,
            rows: host.rows,
            cell_px: host.cell_px,
        },
    });
    host.cols = effect.cols;
    host.rows = effect.rows;
    host.cell_px = effect.cell_px;
    Some(ResizeEffect {
        resize_surface: false,
        ..effect
    })
}

/// One pane's grid, kept while its mark is on screen.
struct PaneDraw {
    surface: SurfaceId,
    grid: GridRenderer,
    stale: bool,
}

#[derive(Default)]
struct GridCompositor {
    slot: Option<SolidSlot>,
    /// By effect mark.
    panes: HashMap<u32, PaneDraw>,
    images: Option<Images>,
    bars: Option<Bars>,
    key: Option<DeviceKey>,
    format: Option<wgpu::TextureFormat>,
}

impl GridCompositor {
    fn ensure(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, key: DeviceKey) {
        let same = self.key == Some(key)
            && self.format == Some(format)
            && self.slot.is_some()
            && self.images.is_some()
            && self.bars.is_some();
        if same {
            return;
        }
        self.slot = Some(SolidSlot::new(device, format));
        self.panes.clear();
        self.images = Some(Images::new(device, format));
        self.bars = Some(Bars::new(device, format));
        self.key = Some(key);
        self.format = Some(format);
    }
}

impl Compositor<Model> for GridCompositor {
    fn render(&mut self, f: &mut Frame<Model>) {
        self.ensure(f.device, f.format, f.device_key());
        let srgb_blend = f.srgb_blend();
        let color = chrome::slot_premul(f.rt.state.palette.bg, srgb_blend);
        let blinking = f.rt.state.terminal.primary().is_some_and(|host| host.blink);
        let cursor_on = f.rt.state.terminal.blink_phase_on() || !blinking;
        let visible = crate::chrome_view::visible(&f.rt.state);
        let rects = pane_rects(&f.rt.instances);
        // A split's leaves fit themselves on layout. The lone pane is fitted to
        // the rect it is about to draw, which layout reports a frame late.
        if let [(chrome::PANE_MARK, _)] = visible.as_slice() {
            if let Some((rect, scale)) = rects.get(&chrome::PANE_MARK) {
                if scale.is_finite() && *scale > 0.0 {
                    fit_layout(&mut f.rt.state, (rect[2] / scale, rect[3] / scale), *scale);
                }
            }
        }
        self.panes
            .retain(|mark, _| visible.iter().any(|(shown, _)| shown == mark));
        for (mark, surface) in &visible {
            let pane = self.panes.entry(*mark).or_insert_with(|| PaneDraw {
                surface: *surface,
                grid: GridRenderer::new(f.device, f.queue, f.format),
                stale: true,
            });
            if pane.surface != *surface {
                pane.surface = *surface;
                pane.stale = true;
            }
        }
        let mut grids: Vec<PaneGrid<'_>> = self
            .panes
            .values_mut()
            .map(|pane| PaneGrid {
                surface: pane.surface,
                grid: &mut pane.grid,
                stale: std::mem::take(&mut pane.stale),
            })
            .collect();
        grid_pass::sync(
            &mut f.rt.state,
            &mut grids,
            self.images.as_mut().expect("images"),
            f.device,
            f.queue,
        );
        let clear = f.rt.clear_color();
        f.ui.prepare(
            f.device,
            f.queue,
            &mut f.rt.text.atlas,
            &f.rt.instances,
            f.size,
        );
        let shown = f.rt.state.terminal.shown_id();
        let mut shown_mark = None;
        let mut pane_origin = [0.0, 0.0];
        for (mark, pane) in &mut self.panes {
            let Some((rect, scale)) = rects.get(mark) else {
                continue;
            };
            let margin = if scale.is_finite() && *scale > 0.0 {
                f32::from(resize::margin_px(*scale))
            } else {
                0.0
            };
            let origin = [rect[0] + margin, rect[1] + margin];
            if Some(pane.surface) == shown {
                shown_mark = Some(*mark);
                pane_origin = origin;
            }
            pane.grid.publish(
                f.queue,
                DrawSlot {
                    origin,
                    size: [
                        (rect[2] - margin * 2.0).max(0.0),
                        (rect[3] - margin * 2.0).max(0.0),
                    ],
                    target: [f.size.0, f.size.1],
                    pass_origin: [0.0, 0.0],
                },
                srgb_blend,
                cursor_on,
            );
        }
        let slot = self.slot.as_ref().expect("solid slot");
        slot.write_color(f.queue, color);
        let bars = self.bars.as_ref().expect("scrollbar");
        let panes = &self.panes;
        let images = self.images.as_mut().expect("images");
        let queue = f.queue;
        let device = f.device;
        f.ui.draw_with(f.encoder, f.target, clear, &mut |pass, site| {
            if scrollback::is_scroll(site.mark) {
                bars.draw(queue, srgb_blend, pass, site, &f.rt.state);
                return;
            }
            let Some(pane) = panes.get(&site.mark) else {
                return;
            };
            // Rune's scissor is the ancestor clip. With no scroll view that is
            // the window, so a fullscreen triangle would cover the tab strip.
            let Some((x, y, w, h)) = pane_scissor(site) else {
                return;
            };
            pass.set_scissor_rect(x, y, w, h);
            slot.draw(pass);
            pane.grid.draw_bg(pass);
            let shows_images = shown_mark == Some(site.mark);
            if shows_images {
                images.draw_bucket(
                    device,
                    queue,
                    pass,
                    ImageBucket::UnderText,
                    pane_origin,
                    site.origin,
                    site.size,
                    srgb_blend,
                );
            }
            pane.grid.draw_text(pass);
            if shows_images {
                images.draw_bucket(
                    device,
                    queue,
                    pass,
                    ImageBucket::OverText,
                    pane_origin,
                    site.origin,
                    site.size,
                    srgb_blend,
                );
            }
        });
    }
}

/// Each effect's rect and scale, by mark.
fn pane_rects(instances: &[Instance]) -> HashMap<u32, ([f32; 4], f32)> {
    instances
        .iter()
        .filter(|inst| inst.params[1] == KIND_EFFECT)
        .map(|inst| {
            let mark = (inst.border[0] as u32 & 0xFFFF) | ((inst.border[1] as u32 & 0xFFFF) << 16);
            (mark, (inst.rect, inst.params[3]))
        })
        .collect()
}

/// Device-pixel scissor of the pane rect cut by its clip, in this pass's target.
fn pane_scissor(site: &rune::render::EffectSite<'_>) -> Option<(u32, u32, u32, u32)> {
    let rect = site.instance.rect;
    let clip = site.instance.clip;
    let x0 = rect[0].max(clip[0]);
    let y0 = rect[1].max(clip[1]);
    let x1 = (rect[0] + rect[2]).min(clip[2]);
    let y1 = (rect[1] + rect[3]).min(clip[3]);
    let edge = |v: f32, origin: f32, max: u32| (v - origin).clamp(0.0, max as f32) as u32;
    let (ox, oy) = (site.origin[0], site.origin[1]);
    let (sx0, sy0) = (
        edge(x0.floor(), ox, site.size[0]),
        edge(y0.floor(), oy, site.size[1]),
    );
    let (sx1, sy1) = (
        edge(x1.ceil(), ox, site.size[0]),
        edge(y1.ceil(), oy, site.size[1]),
    );
    if sx1 <= sx0 || sy1 <= sy0 {
        return None;
    }
    Some((sx0, sy0, sx1 - sx0, sy1 - sy0))
}
