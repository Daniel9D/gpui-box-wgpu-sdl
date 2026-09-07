#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BackdropTextureRole {
    Scene,
    Sharp,
    Horizontal,
    Vertical,
}

pub(super) struct CachedBackdropBindGroup {
    pub source: BackdropTextureRole,
    pub sharp: BackdropTextureRole,
    pub bind_group: wgpu::BindGroup,
}

/// Render targets whose dimensions and lifetime follow one native window.
pub(super) struct BackdropTextures {
    pub _scene: wgpu::Texture,
    pub scene_view: wgpu::TextureView,
    pub sharp: wgpu::Texture,
    pub sharp_view: wgpu::TextureView,
    pub _horizontal: wgpu::Texture,
    pub horizontal_view: wgpu::TextureView,
    pub _blur_weights: wgpu::Texture,
    pub blur_weights_view: wgpu::TextureView,
    pub vertical: wgpu::Texture,
    pub vertical_view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
}

/// Mutable, target-sized resources isolated to one rendered window.
#[derive(Default)]
pub(super) struct WindowRendererState {
    pub path_intermediate_texture: Option<wgpu::Texture>,
    pub path_intermediate_view: Option<wgpu::TextureView>,
    pub path_msaa_texture: Option<wgpu::Texture>,
    pub path_msaa_view: Option<wgpu::TextureView>,
    pub backdrop_textures: Option<BackdropTextures>,
    pub backdrop_params_buffers: Vec<wgpu::Buffer>,
    pub backdrop_blur_weight_bind_groups: std::cell::RefCell<Vec<Option<wgpu::BindGroup>>>,
    pub backdrop_bind_groups: std::cell::RefCell<Vec<Option<CachedBackdropBindGroup>>>,
}

impl WindowRendererState {
    pub fn invalidate_intermediate_textures(&mut self) {
        self.path_intermediate_texture = None;
        self.path_intermediate_view = None;
        self.path_msaa_texture = None;
        self.path_msaa_view = None;
        self.backdrop_textures = None;
        self.backdrop_blur_weight_bind_groups.borrow_mut().clear();
        self.backdrop_bind_groups.borrow_mut().clear();
    }
}
