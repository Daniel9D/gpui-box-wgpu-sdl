/// Immutable pipelines shared by every pass of compatible renderer instances.
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

#[derive(Clone)]
pub(super) struct WgpuBindGroupLayouts {
    pub globals: wgpu::BindGroupLayout,
    pub instances: wgpu::BindGroupLayout,
    pub texture: wgpu::BindGroupLayout,
    pub surfaces: wgpu::BindGroupLayout,
    pub backdrop_blur_weights: wgpu::BindGroupLayout,
    pub backdrop: wgpu::BindGroupLayout,
}

/// Device-scoped immutable resources that compatible windows can reuse.
pub(crate) struct RendererShared {
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
    pub(super) pipelines: WgpuPipelines,
    pub(super) bind_group_layouts: WgpuBindGroupLayouts,
    pub(super) atlas_sampler: wgpu::Sampler,
    pub(super) atlas: Arc<WgpuAtlas>,
    pub(super) surface_format: wgpu::TextureFormat,
    pub(super) alpha_mode: wgpu::CompositeAlphaMode,
    pub(super) path_sample_count: u32,
    pub(super) dual_source_blending: bool,
    pub(super) uses_webgl_instance_data: bool,
    pub(super) atlas_bind_groups: RefCell<HashMap<AtlasTextureId, (u64, wgpu::BindGroup)>>,
    #[cfg(feature = "test-support")]
    pub(super) atlas_bind_group_creations: Cell<u64>,
}

pub(crate) type RendererSharedHandle = Rc<RendererShared>;

/// Mutable resources isolated to one window/target. Dereferencing exposes the
/// compatible immutable GPU resources without duplicating them per window.
pub(super) struct RendererResources {
    pub shared: RendererSharedHandle,
    pub surface: Option<wgpu::Surface<'static>>,
    pub globals_buffer: wgpu::Buffer,
    pub globals_bind_group: wgpu::BindGroup,
    pub path_globals_bind_group: wgpu::BindGroup,
    pub instance_data: InstanceData,
    pub window: WindowRendererState,
    pub external_image_bind_groups: RefCell<HashMap<ExternalImageId, wgpu::BindGroup>>,
    #[cfg(feature = "test-support")]
    pub external_image_bind_group_creations: Cell<u64>,
}

impl RendererResources {
    pub fn invalidate_intermediate_textures(&mut self) {
        self.window.invalidate_intermediate_textures();
    }
}

impl std::ops::Deref for RendererResources {
    type Target = RendererShared;

    fn deref(&self) -> &Self::Target {
        &self.shared
    }
}

use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

#[cfg(feature = "test-support")]
use std::cell::Cell;

use gpui::{AtlasTextureId, ExternalImageId};

use super::{InstanceData, WgpuAtlas, WindowRendererState};
