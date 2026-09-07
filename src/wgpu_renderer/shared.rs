/// Immutable pipelines shared by every pass owned by one renderer instance.
pub(super) struct WgpuPipelines {
    pub quads: wgpu::RenderPipeline,
    pub shadows: wgpu::RenderPipeline,
    pub path_rasterization: wgpu::RenderPipeline,
    pub paths: wgpu::RenderPipeline,
    pub underlines: wgpu::RenderPipeline,
    pub mono_sprites: wgpu::RenderPipeline,
    pub subpixel_sprites: Option<wgpu::RenderPipeline>,
    pub poly_sprites_normal: wgpu::RenderPipeline,
    pub poly_sprites_additive: wgpu::RenderPipeline,
    pub poly_sprites_screen: wgpu::RenderPipeline,
    #[allow(dead_code)]
    pub surfaces: wgpu::RenderPipeline,
    pub backdrop_blur_weights: wgpu::RenderPipeline,
    pub backdrop_blur: wgpu::RenderPipeline,
    pub backdrop_composite: wgpu::RenderPipeline,
    pub backdrop_copy: wgpu::RenderPipeline,
}

pub(super) struct WgpuBindGroupLayouts {
    pub globals: wgpu::BindGroupLayout,
    pub instances: wgpu::BindGroupLayout,
    pub texture: wgpu::BindGroupLayout,
    pub surfaces: wgpu::BindGroupLayout,
    pub backdrop_blur_weights: wgpu::BindGroupLayout,
    pub backdrop: wgpu::BindGroupLayout,
}

/// GPU-global resources and caches shared by all passes of one renderer. The
/// target-sized allocation set is isolated in `window` so resize/recovery can
/// invalidate it without rebuilding immutable pipelines and layouts.
pub(super) struct RendererShared {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface: Option<wgpu::Surface<'static>>,
    pub pipelines: WgpuPipelines,
    pub bind_group_layouts: WgpuBindGroupLayouts,
    pub atlas_sampler: wgpu::Sampler,
    pub globals_buffer: wgpu::Buffer,
    pub globals_bind_group: wgpu::BindGroup,
    pub path_globals_bind_group: wgpu::BindGroup,
    pub instance_data: InstanceData,
    pub window: WindowRendererState,
    pub external_image_bind_groups: RefCell<HashMap<ExternalImageId, wgpu::BindGroup>>,
    pub atlas_bind_groups: RefCell<HashMap<(u64, AtlasTextureId), wgpu::BindGroup>>,
    #[cfg(feature = "test-support")]
    pub external_image_bind_group_creations: Cell<u64>,
    #[cfg(feature = "test-support")]
    pub atlas_bind_group_creations: Cell<u64>,
}

impl RendererShared {
    pub fn invalidate_intermediate_textures(&mut self) {
        self.window.invalidate_intermediate_textures();
    }
}
use std::{cell::RefCell, collections::HashMap, sync::Arc};

#[cfg(feature = "test-support")]
use std::cell::Cell;

use gpui::{AtlasTextureId, ExternalImageId};

use super::{InstanceData, WindowRendererState};
