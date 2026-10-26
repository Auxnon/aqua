//! Aqua: a watercolour and ink painting engine.
//!
//! The simulation runs on the GPU through wgpu (Vulkan on Android, GLES as a
//! fallback). See `canvas.rs` for the model and `shaders/` for the physics.

pub mod brush;
pub mod canvas;
pub mod engine;
pub mod gpu;
pub mod palette;
pub mod pigment;
pub mod pipelines;
pub mod storage;
pub mod undo;

#[cfg(target_os = "android")]
mod android;
