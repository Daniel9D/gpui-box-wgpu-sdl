use std::{ops::Range, sync::Arc};

use crate::ExternalGpu;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WgpuWindow {
    runtime_id: u64,
    index: u32,
    generation: u32,
}

pub struct WgpuWindowState {
    pub cursor_style: gpui::CursorStyle,
    pub cursor_visible: bool,
    pub text_input_active: bool,
    pub ime_area: Option<gpui::Bounds<gpui::Pixels>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseOutcome {
    KeptOpen,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextPreedit {
    pub text: String,
    pub selection_utf16: Option<Range<usize>>,
}

pub struct WgpuRuntime {
    runtime_id: u64,
    _gpu: ExternalGpu,
    _target_format: wgpu::TextureFormat,
    _text_system: Arc<dyn gpui::PlatformTextSystem>,
    _asset_source: Arc<dyn gpui::AssetSource>,
}

pub struct WgpuRuntimeBuilder {
    gpu: ExternalGpu,
    target_format: wgpu::TextureFormat,
    text_system: Arc<dyn gpui::PlatformTextSystem>,
    asset_source: Arc<dyn gpui::AssetSource>,
}

impl WgpuRuntime {
    pub fn new(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
    ) -> anyhow::Result<Self> {
        static NEXT_RUNTIME_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Ok(Self {
            runtime_id: NEXT_RUNTIME_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            _gpu: gpu,
            _target_format: target_format,
            _text_system: text_system,
            _asset_source: asset_source,
        })
    }

    pub fn builder(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
    ) -> WgpuRuntimeBuilder {
        WgpuRuntimeBuilder {
            gpu,
            target_format,
            text_system,
            asset_source,
        }
    }

    pub fn render_window(
        &mut self,
        window: WgpuWindow,
        _target: &wgpu::TextureView,
        _physical_size: wgpu::Extent3d,
        _scale_factor: f32,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            window.runtime_id == self.runtime_id,
            "window belongs to another WgpuRuntime"
        );
        anyhow::bail!("window is not open")
    }
}

impl WgpuRuntimeBuilder {
    pub fn build(self) -> anyhow::Result<WgpuRuntime> {
        WgpuRuntime::new(
            self.gpu,
            self.target_format,
            self.text_system,
            self.asset_source,
        )
    }
}
