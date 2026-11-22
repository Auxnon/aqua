//! Aqua for the desktop: the same Rust engine as the Android and web apps,
//! straight on wgpu (Vulkan or GL on Linux, Metal on macOS), with an egui UI
//! drawn into the same frame over the painting.

mod icons;
mod store;
mod studio;
mod sync;
mod ui;
mod view;

use std::sync::Arc;
use std::time::{Duration, Instant};

use aqua_engine::engine::Engine;
use aqua_engine::gpu::Gpu;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::store::{Settings, Store};
use crate::studio::{Screen, Studio};

/// A mouse has no pressure; this paints like a medium press of a pen.
const MOUSE_PRESSURE: f32 = 0.55;
/// Checked this often while idle: autosave, sync status.
const IDLE_TICK: Duration = Duration::from_secs(2);

struct Gfx {
    window: Arc<Window>,
    egui: egui::Context,
    egui_state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    studio: Studio,
    /// `--snapshot PATH`: where to write the picture, and frames drawn so far.
    snapshot: Option<(std::path::PathBuf, u32)>,
}

#[derive(Default)]
struct Pointer {
    pos: (f32, f32),
    /// Painting with the left button (or a touch / pen).
    drawing: bool,
    /// Moving the painting: middle or right button, or space + left.
    panning: bool,
    touch: Option<u64>,
}

struct App {
    proxy: EventLoopProxy<()>,
    gfx: Option<Gfx>,
    pointer: Pointer,
    next_wake: Option<Instant>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("Aqua").with_inner_size(winit::dpi::LogicalSize::new(1280.0, 860.0)).with_min_inner_size(winit::dpi::LogicalSize::new(420.0, 420.0))
            .with_window_icon(icon());
        // The Wayland app id / X11 class, which ties the window to aqua.desktop.
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs.with_name("aqua", "aqua")
        };
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let instance = Gpu::new_instance_with_display(Box::new(el.owned_display_handle()));
        let surface = instance.create_surface(window.clone()).expect("surface");
        let gpu = match Gpu::new(instance, Some(&surface)) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("aqua: no usable GPU: {e}");
                el.exit();
                return;
            }
        };
        let size = window.inner_size();
        let mut engine = Engine::new(gpu.clone());
        engine.attach_surface(surface, size.width, size.height);
        let format = engine.surface_format().expect("surface format");
        let renderer = egui_wgpu::Renderer::new(&gpu.device, format, egui_wgpu::RendererOptions::default());

        let egui = egui::Context::default();
        ui::style(&egui);
        let max_tex = gpu.device.limits().max_texture_dimension_2d as usize;
        let egui_state = egui_winit::State::new(egui.clone(), egui::ViewportId::ROOT, &window, Some(window.scale_factor() as f32), window.theme(), Some(max_tex));

        let proxy = self.proxy.clone();
        let mut sync = sync::Sync::default();
        sync.wake = Some(Arc::new(move || {
            let _ = proxy.send_event(());
        }));
        let settings = Settings::load();
        let dip = settings.dip;
        // AQUA_PAINTINGS points the gallery somewhere else (for trying things out).
        let root = std::env::var_os("AQUA_PAINTINGS").map(std::path::PathBuf::from).unwrap_or_else(aqua_sync::default_root);
        let mut studio = Studio::new(engine, Store::new(root), settings, sync);
        studio.set_dip(dip, false);
        studio.set_window(size.width, size.height, window.scale_factor() as f32);
        if std::env::args().any(|a| a == "--demo") {
            demo(&mut studio);
        }
        window.request_redraw();
        let args: Vec<String> = std::env::args().collect();
        let snapshot = args.iter().position(|a| a == "--snapshot").and_then(|i| args.get(i + 1)).map(|p| (p.into(), 0));
        self.gfx = Some(Gfx { window, egui, egui_state, renderer, studio, snapshot });
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, _: ()) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(g) = self.gfx.as_mut() else { return };
        let resp = g.egui_state.on_window_event(&g.window, &event);
        if resp.repaint {
            g.window.request_redraw();
        }
        let st = &mut g.studio;
        let on_canvas = st.screen == Screen::Canvas && matches!(st.dialog, studio::Dialog::None);
        // A press over the controls belongs to them; a stroke already going keeps going.
        let over_ui = g.egui.is_pointer_over_egui() || g.egui.egui_is_using_pointer();
        let p = &mut self.pointer;
        match event {
            WindowEvent::CloseRequested => {
                if st.screen == Screen::Canvas {
                    st.leave();
                }
                el.exit();
            }
            WindowEvent::Resized(s) => {
                st.set_window(s.width, s.height, g.window.scale_factor() as f32);
                g.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let s = g.window.inner_size();
                st.set_window(s.width, s.height, scale_factor as f32);
            }
            WindowEvent::RedrawRequested => self.next_wake = redraw(g),
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                let (dx, dy) = (x - p.pos.0, y - p.pos.1);
                p.pos = (x, y);
                if p.drawing && p.touch.is_none() {
                    let pt = st.point(x, y, MOUSE_PRESSURE);
                    st.stroke(1, pt);
                    g.window.request_redraw();
                } else if p.panning {
                    st.pan(dx, dy);
                    g.window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let space = g.egui.input(|i| i.key_down(egui::Key::Space));
                match (state, button) {
                    (ElementState::Pressed, MouseButton::Left) if on_canvas && !over_ui && !p.drawing && !p.panning => {
                        st.panel = studio::Panel::None;
                        st.more_open = false;
                        if space {
                            p.panning = true;
                        } else if st.tool.is_none() {
                            st.sample(p.pos.0, p.pos.1);
                        } else {
                            p.drawing = true;
                            let pt = st.point(p.pos.0, p.pos.1, MOUSE_PRESSURE);
                            st.stroke(0, pt);
                        }
                    }
                    (ElementState::Pressed, MouseButton::Middle | MouseButton::Right) if on_canvas && !over_ui => p.panning = true,
                    (ElementState::Released, MouseButton::Left) if p.drawing && p.touch.is_none() => {
                        p.drawing = false;
                        let pt = st.point(p.pos.0, p.pos.1, MOUSE_PRESSURE);
                        st.stroke(2, pt);
                    }
                    (ElementState::Released, _) => p.panning = false,
                    _ => {}
                }
                g.window.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } if on_canvas && !over_ui => {
                let cmd = g.egui.input(|i| i.modifiers.command);
                match delta {
                    // A wheel zooms about the pointer.
                    MouseScrollDelta::LineDelta(_, y) => st.zoom_at(1.15f32.powf(y), p.pos.0, p.pos.1),
                    // A trackpad pans with two fingers, and zooms with the command key held.
                    MouseScrollDelta::PixelDelta(d) if cmd => st.zoom_at((d.y as f32 * 0.005).exp(), p.pos.0, p.pos.1),
                    MouseScrollDelta::PixelDelta(d) => st.pan(d.x as f32, d.y as f32),
                }
                g.window.request_redraw();
            }
            WindowEvent::PinchGesture { delta, .. } if on_canvas => {
                st.zoom_at(1.0 + delta as f32, p.pos.0, p.pos.1);
                g.window.request_redraw();
            }
            // Touch screens, and pens that come through as touch, with their pressure.
            WindowEvent::Touch(t) if on_canvas => {
                let (x, y) = (t.location.x as f32, t.location.y as f32);
                let pressure = t.force.map_or(MOUSE_PRESSURE, |f| f.normalized() as f32);
                match t.phase {
                    TouchPhase::Started if p.touch.is_none() && !over_ui && st.tool.is_some() => {
                        p.touch = Some(t.id);
                        p.drawing = true;
                        let pt = st.point(x, y, pressure);
                        st.stroke(0, pt);
                    }
                    TouchPhase::Started if p.touch.is_none() && !over_ui => st.sample(x, y),
                    TouchPhase::Moved if p.touch == Some(t.id) => {
                        let pt = st.point(x, y, pressure);
                        st.stroke(1, pt);
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled if p.touch == Some(t.id) => {
                        p.touch = None;
                        p.drawing = false;
                        let pt = st.point(x, y, pressure);
                        st.stroke(if t.phase == TouchPhase::Ended { 2 } else { 3 }, pt);
                    }
                    _ => {}
                }
                g.window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(g) = self.gfx.as_mut() else { return };
        let now = Instant::now();
        let tick = now + IDLE_TICK;
        let wake = self.next_wake.map_or(tick, |w| w.min(tick));
        if wake <= now {
            g.window.request_redraw();
            el.set_control_flow(ControlFlow::Wait);
        } else {
            el.set_control_flow(ControlFlow::WaitUntil(wake));
        }
    }

    fn new_events(&mut self, _el: &ActiveEventLoop, cause: winit::event::StartCause) {
        if let (winit::event::StartCause::ResumeTimeReached { .. }, Some(g)) = (cause, &self.gfx) {
            g.window.request_redraw();
        }
    }
}

/// The app's icon, for window managers that take it from the window (X11).
fn icon() -> Option<winit::window::Icon> {
    let (w, h, rgba) = store::decode_png(include_bytes!("../package/aqua.png"))?;
    winit::window::Icon::from_rgba(rgba, w as u32, h as u32).ok()
}

/// Runs the UI, steps the paint and presents both. Returns when the next frame is due.
fn redraw(g: &mut Gfx) -> Option<Instant> {
    let st = &mut g.studio;
    st.autosave();
    let raw = g.egui_state.take_egui_input(&g.window);
    let out = g.egui.run_ui(raw, |ui| ui::draw(ui, st));
    g.egui_state.handle_platform_output(&g.window, out.platform_output);
    let ppp = out.pixels_per_point;
    let jobs = g.egui.tessellate(out.shapes, ppp);
    let gpu = st.engine.gpu.clone();
    for (id, deltas) in &out.textures_delta.set {
        for delta in deltas {
            g.renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
    }
    let size = g.window.inner_size();
    let screen = egui_wgpu::ScreenDescriptor { size_in_pixels: [size.width.max(1), size.height.max(1)], pixels_per_point: ppp };
    st.engine.advance();
    let renderer = &mut g.renderer;
    st.engine.present_with(|enc, target| draw_egui(renderer, &gpu, enc, target, &jobs, &screen));
    if let Some((path, frames)) = g.snapshot.as_mut() {
        *frames += 1;
        if *frames >= 90 {
            snapshot(&mut st.engine, renderer, &jobs, &screen, path);
            std::process::exit(0);
        }
        g.window.request_redraw();
    }
    for id in &out.textures_delta.free {
        g.renderer.free_texture(id);
    }
    let now = Instant::now();
    let mut next: Option<Instant> = None;
    if st.engine.end_frame() {
        next = Some(now + st.engine.frame_delay());
    }
    let egui_delay = out.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
    if egui_delay < Duration::from_secs(3600) {
        let t = now + egui_delay;
        next = Some(next.map_or(t, |n| n.min(t)));
    }
    if next.is_some_and(|n| n <= now) {
        g.window.request_redraw();
    }
    next
}

/// `aqua --demo`: a new painting with a few strokes on it, to see that it all works.
fn demo(st: &mut Studio) {
    // `--plain`: smooth hot press that looks like plain white paper.
    let plain = std::env::args().any(|a| a == "--plain");
    st.create(1600, 1100, plain as u32, plain as u8, plain);
    let (w, h) = st.window;
    for k in 0..3 {
        let y = h * (0.35 + 0.15 * k as f32);
        for i in 0..=40 {
            let x = w * (0.25 + 0.5 * i as f32 / 40.0);
            let p = st.point(x, y + (i as f32 * 0.4).sin() * 30.0, 0.7);
            st.stroke(if i == 0 { 0 } else if i == 40 { 2 } else { 1 }, p);
        }
        let ink = k == 2;
        st.select_tool(Some(if ink { aqua_engine::brush::Tool::Ink } else { aqua_engine::brush::Tool::Watercolor }));
    }
}

fn draw_egui(renderer: &mut egui_wgpu::Renderer, gpu: &Gpu, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, jobs: &[egui::ClippedPrimitive], screen: &egui_wgpu::ScreenDescriptor) {
    // Paint callbacks would be submitted first; egui here has none.
    let _ = renderer.update_buffers(&gpu.device, &gpu.queue, enc, jobs, screen);
    let mut pass = enc
        .begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("egui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        })
        .forget_lifetime();
    renderer.render(&mut pass, jobs, screen);
}

/// `--snapshot out.png`: the window's picture, painting and controls, drawn offscreen.
fn snapshot(engine: &mut Engine, renderer: &mut egui_wgpu::Renderer, jobs: &[egui::ClippedPrimitive], screen: &egui_wgpu::ScreenDescriptor, path: &std::path::Path) {
    let gpu = engine.gpu.clone();
    let Some(format) = engine.surface_format() else { return };
    let [w, h] = screen.size_in_pixels;
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    engine.draw_to(&view, |enc, target| draw_egui(renderer, &gpu, enc, target, jobs, screen));
    let mut px = aqua_engine::gpu::read_texture(&gpu, &tex);
    if matches!(format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
        for c in px.as_chunks_mut::<4>().0 {
            c.swap(0, 2);
        }
    }
    match aqua_engine::storage::encode_png(&(w, h, px)).map(|png| std::fs::write(path, png)) {
        Ok(Ok(())) => println!("aqua: wrote {}", path.display()),
        e => eprintln!("aqua: snapshot failed: {e:?}"),
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let el = EventLoop::<()>::with_user_event().build().expect("event loop");
    let mut app = App { proxy: el.create_proxy(), gfx: None, pointer: Pointer::default(), next_wake: None };
    el.run_app(&mut app).expect("event loop");
}
