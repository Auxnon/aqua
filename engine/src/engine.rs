//! Platform-independent engine: owns the GPU, the open painting and the
//! on-screen presentation. Android drives it from its own thread (see
//! `android.rs`); tests drive it headless.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};

use crate::brush::{BrushSettings, InputPoint};
use crate::canvas::{Canvas, PaperSpec, STEPS_PER_SECOND, tv};
use crate::gpu::Gpu;
use crate::pipelines::{Pipelines, present_pipeline};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ViewU {
    row0: [f32; 4],
    row1: [f32; 4],
    canvas: [f32; 2],
    scale: f32,
    encode_srgb: f32,
    background: [f32; 4],
}

pub struct Presenter {
    pub surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    encode_srgb: bool,
    sampler: wgpu::Sampler,
    view_buf: wgpu::Buffer,
}

/// canvas_from_screen affine transform plus the zoom factor.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub m: [f32; 6],
    pub scale: f32,
}

impl Default for View {
    fn default() -> Self {
        Self { m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0], scale: 1.0 }
    }
}

/// While paint is only drying (no touch, no view change), frames are spaced this
/// far apart. The simulation keeps its pace by taking more steps per frame.
/// Matches the Android window and Compose background exactly, so nothing blinks
/// when the canvas surface comes and goes.
const BACKGROUND_SRGB: [f32; 4] = [9.0 / 255.0, 9.0 / 255.0, 11.0 / 255.0, 1.0];

const DRYING_FRAME: Duration = Duration::from_millis(33);
/// How long after the last touch frames stay at the display's full rate.
const INPUT_GRACE: Duration = Duration::from_millis(400);

pub struct Engine {
    pub gpu: Arc<Gpu>,
    pub pipes: Arc<Pipelines>,
    pub canvas: Option<Canvas>,
    pub presenter: Option<Presenter>,
    pub view: View,
    pub brush: BrushSettings,
    last_frame: Option<Instant>,
    step_debt: f32,
    last_input: Option<Instant>,
    /// Set by anything outside the frame loop that changes what is on screen.
    pub needs_present: bool,
}

impl Engine {
    pub fn new(gpu: Arc<Gpu>) -> Self {
        let pipes = Arc::new(Pipelines::new(&gpu.device));
        Self {
            gpu,
            pipes,
            canvas: None,
            presenter: None,
            view: View::default(),
            brush: BrushSettings::default(),
            last_frame: None,
            step_debt: 0.0,
            last_input: None,
            needs_present: true,
        }
    }

    pub fn new_canvas(&mut self, w: u32, h: u32, paper_kind: u32, seed: u32) {
        let max = self.gpu.device.limits().max_texture_dimension_2d;
        let (w, h) = (w.clamp(64, max), h.clamp(64, max));
        self.canvas = Some(Canvas::new(self.gpu.clone(), self.pipes.clone(), w, h, PaperSpec::preset(paper_kind, seed)));
        self.needs_present = true;
    }

    pub fn attach_surface(&mut self, surface: wgpu::Surface<'static>, w: u32, h: u32) {
        let caps = surface.get_capabilities(&self.gpu.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .unwrap_or(wgpu::TextureFormat::Rgba8Unorm);
        let alpha = caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w.max(1),
            height: h.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: alpha,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&self.gpu.device, &config);
        let d = &self.gpu.device;
        self.presenter = Some(Presenter {
            pipeline: present_pipeline(d, format),
            encode_srgb: !format.is_srgb(),
            sampler: d.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            view_buf: d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("view"),
                size: size_of::<ViewU>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            surface,
            config,
        });
        self.needs_present = true;
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if let Some(p) = self.presenter.as_mut() {
            p.config.width = w.max(1);
            p.config.height = h.max(1);
            p.surface.configure(&self.gpu.device, &p.config);
        }
        self.needs_present = true;
    }

    pub fn detach_surface(&mut self) {
        self.presenter = None;
        self.last_frame = None;
    }

    pub fn set_view(&mut self, v: View) {
        self.view = v;
        self.last_input = Some(Instant::now());
        self.needs_present = true;
    }

    pub fn stroke(&mut self, phase: i32, pts: &[InputPoint]) {
        let Some(c) = self.canvas.as_mut() else { return };
        match phase {
            3 => c.cancel_stroke(),
            _ => {
                for (i, p) in pts.iter().enumerate() {
                    if phase == 0 && i == 0 {
                        c.begin_stroke(self.brush, *p);
                    } else {
                        c.stroke_to(*p);
                    }
                }
                if phase == 2 {
                    c.end_stroke(pts.last().copied());
                }
            }
        }
        self.last_input = Some(Instant::now());
        self.needs_present = true;
    }

    /// True if `frame` should be called again soon.
    pub fn wants_frame(&self) -> bool {
        self.needs_present || self.canvas.as_ref().is_some_and(|c| c.needs_update())
    }

    /// True while the user is interacting, or something outside the simulation
    /// changed the picture: frames then follow the display rate.
    fn interactive(&self) -> bool {
        self.needs_present
            || self.canvas.as_ref().is_some_and(|c| c.stroking())
            || self.last_input.is_some_and(|t| t.elapsed() < INPUT_GRACE)
    }

    /// How long to wait before the next `frame`. Zero while interactive; while paint
    /// is only drying, frames are spaced `DRYING_FRAME` apart to save power.
    pub fn frame_delay(&self) -> Duration {
        match self.last_frame {
            Some(t) if !self.interactive() => DRYING_FRAME.saturating_sub(t.elapsed()),
            _ => Duration::ZERO,
        }
    }

    /// Steps the simulation by the elapsed wall time and presents. Returns `wants_frame()`.
    pub fn frame(&mut self) -> bool {
        let now = Instant::now();
        let dt = self.last_frame.map_or(1.0 / 60.0, |t| (now - t).as_secs_f32()).min(0.1);
        self.last_frame = Some(now);
        // At most two steps a frame (four at the drying frame rate): on a slow GPU the
        // paint dries a little slower instead of the frame rate spiralling down.
        let max_steps = if self.interactive() { 2 } else { 4 };
        let mut changed = self.needs_present;
        if let Some(c) = self.canvas.as_mut() {
            c.dwell(dt);
            self.step_debt += dt * STEPS_PER_SECOND;
            let steps = (self.step_debt.floor() as u32).min(max_steps);
            self.step_debt = (self.step_debt - steps as f32).min(1.0);
            if c.needs_update() || steps > 0 && c.any_wet() {
                c.update(steps);
                changed = true;
            }
        }
        if changed {
            self.present();
        }
        let more = self.wants_frame();
        if !more {
            self.last_frame = None;
        }
        more
    }

    fn present(&mut self) {
        let Some(p) = self.presenter.as_mut() else { return };
        let frame = match p.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                p.surface.configure(&self.gpu.device, &p.config);
                return;
            }
            _ => return,
        };
        self.needs_present = false;
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        // The app's background (#09090B), in linear light: the shader works in
        // linear and encodes to sRGB on the way out, like an sRGB surface does.
        let bg_color = BACKGROUND_SRGB.map(crate::pigment::srgb_to_linear);
        let c = if p.encode_srgb { BACKGROUND_SRGB } else { bg_color };
        let clear = wgpu::Color { r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: 1.0 };
        let atts = color_attachment(&target, clear);
        match self.canvas.as_ref() {
            Some(c) => {
                let m = self.view.m;
                let u = ViewU {
                    row0: [m[0], m[1], m[2], 0.0],
                    row1: [m[3], m[4], m[5], 0.0],
                    canvas: [c.w as f32, c.h as f32],
                    scale: self.view.scale,
                    encode_srgb: p.encode_srgb as u32 as f32,
                    background: bg_color,
                };
                self.gpu.queue.write_buffer(&p.view_buf, 0, bytemuck::bytes_of(&u));
                let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &p.pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: p.view_buf.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: tv(&c.composite) },
                        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&p.sampler) },
                    ],
                });
                let mut pass = enc.begin_render_pass(&pass_desc(&atts));
                pass.set_pipeline(&p.pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.draw(0..3, 0..1);
            }
            None => {
                let _ = enc.begin_render_pass(&pass_desc(&atts));
            }
        }
        self.gpu.queue.submit([enc.finish()]);
        self.gpu.queue.present(frame);
    }
}

fn color_attachment(target: &wgpu::TextureView, clear: wgpu::Color) -> [Option<wgpu::RenderPassColorAttachment<'_>>; 1] {
    [Some(wgpu::RenderPassColorAttachment {
        view: target,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
    })]
}

fn pass_desc<'a>(atts: &'a [Option<wgpu::RenderPassColorAttachment<'a>>]) -> wgpu::RenderPassDescriptor<'a> {
    wgpu::RenderPassDescriptor { label: Some("present"), color_attachments: atts, depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }
}
