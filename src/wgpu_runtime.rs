use std::{borrow::Cow, ops::Range, rc::Rc, sync::Arc};

use gpui::{AssetSource, WindowBounds, WindowOptions};
use slotmap::{Key as _, KeyData, SlotMap, new_key_type};

use crate::{EmbeddedDisplay, EmbeddedPlatform, ExternalGpu, WgpuAtlas};

new_key_type! { struct RuntimeWindowKey; }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WgpuWindow {
    runtime_id: u64,
    key: u64,
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
    dispatcher: Arc<gpui::ThreadedDispatcher>,
    platform: Rc<EmbeddedPlatform>,
    app: gpui::ApplicationHandle,
    windows: SlotMap<RuntimeWindowKey, gpui::AnyWindowHandle>,
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
        let dispatcher = Arc::new(gpui::ThreadedDispatcher::new());
        let atlas: Arc<dyn gpui::PlatformAtlas> = Arc::new(WgpuAtlas::new(
            gpu.device.clone(),
            gpu.queue.clone(),
            target_format,
        ));
        let platform = EmbeddedPlatform::new(
            dispatcher.clone(),
            text_system,
            atlas,
            Rc::new(EmbeddedDisplay),
        );
        let app = gpui::Application::new_inaccessible(platform.clone())
            .with_assets(SharedAssets(asset_source))
            .with_quit_mode(gpui::QuitMode::Explicit)
            .run_embedded(|_| {});
        Ok(Self {
            runtime_id: NEXT_RUNTIME_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            _gpu: gpu,
            _target_format: target_format,
            dispatcher,
            platform,
            app,
            windows: SlotMap::with_key(),
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

    pub fn open_window<V: gpui::Render + 'static>(
        &mut self,
        initial_size: gpui::Size<gpui::Pixels>,
        build_root: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> gpui::Entity<V>,
    ) -> anyhow::Result<(WgpuWindow, gpui::Entity<V>)> {
        let width = initial_size.width.as_f32();
        let height = initial_size.height.as_f32();
        anyhow::ensure!(
            width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0,
            "logical window dimensions must be positive and finite"
        );
        let mut root = None;
        let handle = self.app.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::new(
                        gpui::Point::default(),
                        initial_size,
                    ))),
                    ..Default::default()
                },
                |window, cx| {
                    let entity = build_root(window, cx);
                    root = Some(entity.clone());
                    entity
                },
            )
        })?;
        let key = self.windows.insert(handle.into());
        Ok((
            WgpuWindow {
                runtime_id: self.runtime_id,
                key: key.data().as_ffi(),
            },
            root.expect("GPUI invokes the root builder while opening the window"),
        ))
    }

    pub fn render_window(
        &mut self,
        window: WgpuWindow,
        _target: &wgpu::TextureView,
        physical_size: wgpu::Extent3d,
        scale_factor: f32,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            physical_size.width > 0
                && physical_size.height > 0
                && physical_size.depth_or_array_layers == 1,
            "target dimensions must be non-zero with exactly one layer"
        );
        anyhow::ensure!(
            scale_factor.is_finite() && scale_factor > 0.0,
            "scale factor must be positive and finite"
        );
        let handle = self.window_handle(window)?;
        self.platform.resize(
            handle,
            gpui::size(
                gpui::px(physical_size.width as f32 / scale_factor),
                gpui::px(physical_size.height as f32 / scale_factor),
            ),
            scale_factor,
        )?;
        self.platform.request_frame(
            handle,
            gpui::RequestFrameOptions {
                require_presentation: true,
                force_render: true,
            },
        )?;
        anyhow::bail!("embedded rendering is not connected yet")
    }

    pub fn close_window(&mut self, window: WgpuWindow) -> anyhow::Result<()> {
        let key = self.window_key(window)?;
        self.platform.force_close(self.windows[key])?;
        self.windows.remove(key);
        self.pump();
        Ok(())
    }

    pub fn request_close_window(&mut self, window: WgpuWindow) -> anyhow::Result<CloseOutcome> {
        let key = self.window_key(window)?;
        let handle = self.windows[key];
        self.platform.request_close(handle)?;
        self.pump();
        if self.platform.window(handle).is_some() {
            Ok(CloseOutcome::KeptOpen)
        } else {
            self.windows.remove(key);
            Ok(CloseOutcome::Closed)
        }
    }

    pub fn window_state(&self, window: WgpuWindow) -> anyhow::Result<WgpuWindowState> {
        let embedded = self
            .platform
            .window(self.window_handle(window)?)
            .ok_or_else(|| anyhow::anyhow!("window is closed"))?;
        Ok(WgpuWindowState {
            cursor_style: self.platform.cursor_style(),
            cursor_visible: self.platform.cursor_visible(),
            text_input_active: embedded.text_input_active(),
            ime_area: embedded.ime_area(),
        })
    }

    pub fn pump(&mut self) {
        self.dispatcher.run_ready_main_tasks();
    }

    fn window_key(&self, window: WgpuWindow) -> anyhow::Result<RuntimeWindowKey> {
        anyhow::ensure!(
            window.runtime_id == self.runtime_id,
            "window belongs to another WgpuRuntime"
        );
        let key = RuntimeWindowKey::from(KeyData::from_ffi(window.key));
        anyhow::ensure!(self.windows.contains_key(key), "window is closed");
        Ok(key)
    }

    fn window_handle(&self, window: WgpuWindow) -> anyhow::Result<gpui::AnyWindowHandle> {
        Ok(self.windows[self.window_key(window)?])
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

struct SharedAssets(Arc<dyn AssetSource>);

impl AssetSource for SharedAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        self.0.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<gpui::SharedString>> {
        self.0.list(path)
    }
}
