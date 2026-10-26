//! Device setup and small texture/buffer helpers shared by the engine.

use std::sync::Arc;

pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub fn new_instance() -> wgpu::Instance {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
        wgpu::Instance::new(desc)
    }

    /// Picks an adapter (preferring Vulkan) able to present to `surface`, if given.
    pub fn new(instance: wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Result<Arc<Self>, String> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface,
            apply_limit_buckets: false,
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        log::info!("aqua: using adapter {:?}", adapter.get_info());

        let alimits = adapter.limits();
        let mut limits = wgpu::Limits::downlevel_defaults();
        limits.max_texture_dimension_2d = alimits.max_texture_dimension_2d.min(8192);
        limits.max_storage_textures_per_shader_stage = alimits.max_storage_textures_per_shader_stage.min(8);
        limits.max_color_attachments = alimits.max_color_attachments.min(8);
        limits.max_color_attachment_bytes_per_sample = alimits.max_color_attachment_bytes_per_sample;
        limits.max_compute_workgroup_storage_size = alimits.max_compute_workgroup_storage_size;
        limits.max_storage_buffers_per_shader_stage = alimits.max_storage_buffers_per_shader_stage.min(4);
        limits.max_sampled_textures_per_shader_stage = alimits.max_sampled_textures_per_shader_stage.min(16);

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("aqua"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .map_err(|e| format!("request_device failed: {e}"))?;
        device.on_uncaptured_error(Arc::new(|e| log::error!("aqua wgpu error: {e}")));
        Ok(Arc::new(Self { instance, adapter, device, queue }))
    }

    pub fn headless() -> Result<Arc<Self>, String> {
        Self::new(Self::new_instance(), None)
    }
}

pub const SIM_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// A texture together with its default view.
pub struct Tex {
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl Tex {
    pub fn new(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat, label: &str) -> Self {
        let usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | if format == SIM_FORMAT { wgpu::TextureUsages::RENDER_ATTACHMENT } else { wgpu::TextureUsages::empty() };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        Self { tex, view }
    }

    pub fn sim(device: &wgpu::Device, w: u32, h: u32, label: &str) -> Self {
        Self::new(device, w, h, SIM_FORMAT, label)
    }

    pub fn width(&self) -> u32 {
        self.tex.width()
    }

    pub fn height(&self) -> u32 {
        self.tex.height()
    }

    pub fn bytes(&self) -> u64 {
        let bpp = self.tex.format().block_copy_size(None).unwrap_or(8) as u64;
        self.width() as u64 * self.height() as u64 * bpp
    }
}

/// Integer rectangle in canvas pixels (x0,y0 inclusive; x1,y1 exclusive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    pub const EMPTY: IRect = IRect { x0: i32::MAX, y0: i32::MAX, x1: i32::MIN, y1: i32::MIN };

    pub fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn full(w: u32, h: u32) -> Self {
        Self::new(0, 0, w as i32, h as i32)
    }

    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    pub fn union(&self, o: &IRect) -> IRect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        IRect::new(self.x0.min(o.x0), self.y0.min(o.y0), self.x1.max(o.x1), self.y1.max(o.y1))
    }

    pub fn intersect(&self, o: &IRect) -> IRect {
        IRect::new(self.x0.max(o.x0), self.y0.max(o.y0), self.x1.min(o.x1), self.y1.min(o.y1))
    }

    pub fn pad(&self, m: i32) -> IRect {
        if self.is_empty() {
            return *self;
        }
        IRect::new(self.x0 - m, self.y0 - m, self.x1 + m, self.y1 + m)
    }

    pub fn contains(&self, o: &IRect) -> bool {
        o.is_empty() || (o.x0 >= self.x0 && o.y0 >= self.y0 && o.x1 <= self.x1 && o.y1 <= self.y1)
    }

    pub fn w(&self) -> u32 {
        (self.x1 - self.x0).max(0) as u32
    }

    pub fn h(&self) -> u32 {
        (self.y1 - self.y0).max(0) as u32
    }
}

pub fn copy_rect(enc: &mut wgpu::CommandEncoder, src: &wgpu::Texture, dst: &wgpu::Texture, src_xy: (u32, u32), dst_xy: (u32, u32), w: u32, h: u32) {
    if w == 0 || h == 0 {
        return;
    }
    enc.copy_texture_to_texture(
        wgpu::TexelCopyTextureInfo { texture: src, mip_level: 0, origin: wgpu::Origin3d { x: src_xy.0, y: src_xy.1, z: 0 }, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyTextureInfo { texture: dst, mip_level: 0, origin: wgpu::Origin3d { x: dst_xy.0, y: dst_xy.1, z: 0 }, aspect: wgpu::TextureAspect::All },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
}

pub fn copy_full(enc: &mut wgpu::CommandEncoder, src: &Tex, dst: &Tex) {
    copy_rect(enc, &src.tex, &dst.tex, (0, 0), (0, 0), src.width(), src.height());
}

/// Blocking read of a whole texture into tightly packed bytes.
pub fn read_texture(gpu: &Gpu, tex: &wgpu::Texture) -> Vec<u8> {
    let (w, h) = (tex.width(), tex.height());
    let bpp = tex.format().block_copy_size(None).unwrap_or(4);
    let row = w * bpp;
    let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: padded as u64 * h as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([enc.finish()]);
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().expect("readback map failed");
    let mut out = Vec::with_capacity((row * h) as usize);
    for y in 0..h as usize {
        let start = y * padded as usize;
        out.extend_from_slice(&data[start..start + row as usize]);
    }
    drop(data);
    buf.unmap();
    out
}

/// Blocking read of one texel.
pub fn read_pixel(gpu: &Gpu, tex: &wgpu::Texture, x: u32, y: u32) -> Vec<u8> {
    let bpp = tex.format().block_copy_size(None).unwrap_or(4);
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pixel"),
        size: wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: None, rows_per_image: None } },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([enc.finish()]);
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let out = slice.get_mapped_range().map(|d| d[..bpp as usize].to_vec()).unwrap_or_default();
    buf.unmap();
    out
}

/// Uploads tightly packed bytes into a whole texture.
pub fn write_texture(gpu: &Gpu, tex: &wgpu::Texture, data: &[u8]) {
    let bpp = tex.format().block_copy_size(None).unwrap_or(4);
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(tex.width() * bpp), rows_per_image: Some(tex.height()) },
        wgpu::Extent3d { width: tex.width(), height: tex.height(), depth_or_array_layers: 1 },
    );
}

/// Per-frame uniform arena: values are staged on the CPU, bound by offset, and
/// uploaded with a single write before the frame's commands are submitted.
pub struct UniformRing {
    pub buf: wgpu::Buffer,
    staging: Vec<u8>,
    align: usize,
}

impl UniformRing {
    pub const CAPACITY: u64 = 1 << 20;

    pub fn new(device: &wgpu::Device) -> Self {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform ring"),
            size: Self::CAPACITY,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let align = device.limits().min_uniform_buffer_offset_alignment as usize;
        Self { buf, staging: Vec::with_capacity(1 << 16), align }
    }

    /// Stages `value`; bind it with [`UniformRing::res`].
    pub fn push<T: bytemuck::Pod>(&mut self, value: &T) -> UBind {
        let off = self.staging.len().next_multiple_of(self.align);
        assert!((off + size_of::<T>()) as u64 <= Self::CAPACITY, "uniform ring overflow");
        self.staging.resize(off, 0);
        self.staging.extend_from_slice(bytemuck::bytes_of(value));
        UBind { offset: off as u64, size: size_of::<T>() as u64 }
    }

    pub fn res(&self, b: UBind) -> wgpu::BindingResource<'_> {
        wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.buf, offset: b.offset, size: wgpu::BufferSize::new(b.size) })
    }

    pub fn flush(&mut self, queue: &wgpu::Queue) {
        if !self.staging.is_empty() {
            queue.write_buffer(&self.buf, 0, &self.staging);
            self.staging.clear();
        }
    }
}

#[derive(Clone, Copy)]
pub struct UBind {
    pub offset: u64,
    pub size: u64,
}
