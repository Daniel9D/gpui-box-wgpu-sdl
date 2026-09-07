use std::{borrow::Cow, cell::RefCell, ops::Range, rc::Rc, sync::Arc};

use gpui::AppContext as _;
use gpui::{AssetSource, WindowBounds, WindowOptions};
use slotmap::{Key as _, KeyData, SlotMap, new_key_type};

use crate::{
    EmbeddedDisplay, EmbeddedPlatform, ExternalGpu, WgpuAtlas, WgpuContext, WgpuHeadlessRenderer,
};

new_key_type! { struct RuntimeWindowKey; }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WgpuWindow {
    runtime_id: u64,
    key: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WgpuWindowState {
    pub cursor_style: gpui::CursorStyle,
    pub cursor_visible: bool,
    pub text_input_active: bool,
    pub ime_area: Option<gpui::Bounds<gpui::Pixels>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WgpuWindowError {
    ForeignRuntime,
    Closed,
    EntityAlreadyAttached,
    WrongEntity,
}

impl std::fmt::Display for WgpuWindowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignRuntime => formatter.write_str("window belongs to another WgpuRuntime"),
            Self::Closed => formatter.write_str("window is closed"),
            Self::EntityAlreadyAttached => formatter.write_str("entity is already attached"),
            Self::WrongEntity => formatter.write_str("entity is not attached to this window"),
        }
    }
}

impl std::error::Error for WgpuWindowError {}

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
    context: WgpuContext,
    atlas: Arc<WgpuAtlas>,
    target_format: wgpu::TextureFormat,
    dispatcher: Arc<gpui::ThreadedDispatcher>,
    platform: Rc<EmbeddedPlatform>,
    app: gpui::ApplicationHandle,
    windows: SlotMap<RuntimeWindowKey, RuntimeWindow>,
    attached_entities: std::collections::HashSet<gpui::EntityId>,
    execution_mode: WgpuExecutionMode,
    renderer_shared: Option<crate::wgpu_renderer::shared::RendererSharedHandle>,
}

struct RuntimeWindow {
    handle: gpui::AnyWindowHandle,
    render: Rc<RefCell<WindowRenderState>>,
    redraw_pending: bool,
    root_entity_id: gpui::EntityId,
}

struct WindowRenderTarget {
    view: wgpu::TextureView,
    size: gpui::Size<gpui::DevicePixels>,
}

struct WindowRenderState {
    renderer: WgpuHeadlessRenderer,
    target: Option<WindowRenderTarget>,
    error: Option<anyhow::Error>,
}

struct WindowRenderTargetGuard(Rc<RefCell<WindowRenderState>>);

impl Drop for WindowRenderTargetGuard {
    fn drop(&mut self) {
        if let Ok(mut render) = self.0.try_borrow_mut() {
            render.target = None;
        }
    }
}

pub struct WgpuRuntimeBuilder {
    gpu: ExternalGpu,
    target_format: wgpu::TextureFormat,
    text_system: Arc<dyn gpui::PlatformTextSystem>,
    asset_source: Arc<dyn gpui::AssetSource>,
    execution_mode: WgpuExecutionMode,
    default_scale_factor: f32,
    default_appearance: gpui::WindowAppearance,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WgpuExecutionMode {
    #[default]
    Realtime,
    Deterministic,
}

impl WgpuRuntime {
    pub fn new(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
    ) -> anyhow::Result<Self> {
        Self::builder(gpu, target_format, text_system, asset_source).build()
    }

    fn new_configured(builder: WgpuRuntimeBuilder) -> anyhow::Result<Self> {
        anyhow::ensure!(
            builder.default_scale_factor.is_finite() && builder.default_scale_factor > 0.0,
            "default scale factor must be positive and finite"
        );
        static NEXT_RUNTIME_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let dispatcher = Arc::new(gpui::ThreadedDispatcher::new());
        let context = WgpuContext::from_external(
            builder.gpu.instance,
            builder.gpu.adapter,
            builder.gpu.device,
            builder.gpu.queue,
        )?;
        let atlas = Arc::new(WgpuAtlas::from_context(&context));
        let platform = EmbeddedPlatform::new_with_defaults(
            dispatcher.clone(),
            builder.text_system,
            atlas.clone(),
            Rc::new(EmbeddedDisplay),
            builder.default_scale_factor,
            builder.default_appearance,
        );
        let app = gpui::Application::new_inaccessible(platform.clone())
            .with_assets(SharedAssets(builder.asset_source))
            .with_quit_mode(gpui::QuitMode::Explicit)
            .run_embedded(|_| {});
        Ok(Self {
            runtime_id: NEXT_RUNTIME_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            context,
            atlas,
            target_format: builder.target_format,
            dispatcher,
            platform,
            app,
            windows: SlotMap::with_key(),
            attached_entities: std::collections::HashSet::new(),
            execution_mode: builder.execution_mode,
            renderer_shared: None,
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
            execution_mode: WgpuExecutionMode::Realtime,
            default_scale_factor: 1.0,
            default_appearance: gpui::WindowAppearance::Light,
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
        let handle = handle.into();
        let renderer = WgpuHeadlessRenderer::from_context(
            &self.context,
            self.atlas.clone(),
            self.target_format,
            self.execution_mode == WgpuExecutionMode::Deterministic,
            self.renderer_shared.clone(),
        )?;
        if self.renderer_shared.is_none() {
            self.renderer_shared = Some(renderer.shared_resources());
        }
        let render = Rc::new(RefCell::new(WindowRenderState {
            renderer,
            target: None,
            error: None,
        }));
        let render_callback = render.clone();
        self.platform
            .window(handle)
            .expect("the embedded platform owns every open GPUI window")
            .set_render_callback(Box::new(move |scene| {
                let mut state = render_callback.borrow_mut();
                let Some(target) = state.target.as_ref() else {
                    return;
                };
                let view = target.view.clone();
                let size = target.size;
                if let Err(error) = state.renderer.render_scene_to_view(scene, size, &view) {
                    state.error = Some(error);
                }
            }));
        let luminance = render.clone();
        self.platform
            .window(handle)
            .expect("the embedded platform owns every open GPUI window")
            .set_luminance_callback(Box::new(move |slot| {
                luminance
                    .try_borrow_mut()
                    .ok()
                    .and_then(|mut state| state.renderer.backdrop_luminance(slot))
            }));
        let root = root.expect("GPUI invokes the root builder while opening the window");
        let root_entity_id = root.entity_id();
        let inserted = self.attached_entities.insert(root_entity_id);
        debug_assert!(
            inserted,
            "a newly built root entity cannot already be attached"
        );
        let key = self.windows.insert(RuntimeWindow {
            handle,
            render,
            redraw_pending: true,
            root_entity_id,
        });
        Ok((
            WgpuWindow {
                runtime_id: self.runtime_id,
                key: key.data().as_ffi(),
            },
            root,
        ))
    }

    pub fn open_window_with_entity<V: gpui::Render + 'static>(
        &mut self,
        initial_size: gpui::Size<gpui::Pixels>,
        entity: gpui::Entity<V>,
    ) -> anyhow::Result<WgpuWindow> {
        if self.attached_entities.contains(&entity.entity_id()) {
            return Err(WgpuWindowError::EntityAlreadyAttached.into());
        }
        let (window, _) = self.open_window(initial_size, move |_, _| entity)?;
        Ok(window)
    }

    pub fn detach_window<V: gpui::Render + 'static>(
        &mut self,
        window: WgpuWindow,
        entity: &gpui::Entity<V>,
    ) -> anyhow::Result<gpui::Entity<V>> {
        let key = self.window_key(window)?;
        if self.windows[key].root_entity_id != entity.entity_id() {
            return Err(WgpuWindowError::WrongEntity.into());
        }
        let entity = entity.clone();
        self.close_window(window)?;
        Ok(entity)
    }

    pub fn render_window(
        &mut self,
        window: WgpuWindow,
        target: &wgpu::TextureView,
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
        let key = self.window_key(window)?;
        let handle = self.windows[key].handle;
        let width = i32::try_from(physical_size.width)
            .map_err(|_| anyhow::anyhow!("target width exceeds GPUI limits"))?;
        let height = i32::try_from(physical_size.height)
            .map_err(|_| anyhow::anyhow!("target height exceeds GPUI limits"))?;
        self.platform.resize(
            handle,
            gpui::size(
                gpui::px(physical_size.width as f32 / scale_factor),
                gpui::px(physical_size.height as f32 / scale_factor),
            ),
            scale_factor,
        )?;
        let render_state = self.windows[key].render.clone();
        {
            let mut render = render_state.borrow_mut();
            anyhow::ensure!(
                render.target.is_none(),
                "window render is already in progress"
            );
            render.error = None;
            render.target = Some(WindowRenderTarget {
                view: target.clone(),
                size: gpui::size(gpui::DevicePixels(width), gpui::DevicePixels(height)),
            });
        }
        let target_guard = WindowRenderTargetGuard(render_state.clone());
        self.platform.request_frame(
            handle,
            gpui::RequestFrameOptions {
                require_presentation: true,
                force_render: true,
            },
        )?;
        self.pump();
        drop(target_guard);
        let mut render = render_state.borrow_mut();
        if let Some(error) = render.error.take() {
            return Err(error);
        }
        drop(render);
        self.windows[key].redraw_pending = false;
        Ok(())
    }

    pub fn update_window<R>(
        &mut self,
        window: WgpuWindow,
        update: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R,
    ) -> anyhow::Result<R> {
        let handle = self.window_handle(window)?;
        self.app
            .update(|cx| cx.update_window(handle, |_, window, cx| update(window, cx)))
    }

    pub fn resize_window(
        &mut self,
        window: WgpuWindow,
        logical_size: gpui::Size<gpui::Pixels>,
        scale_factor: f32,
    ) -> anyhow::Result<()> {
        let width = logical_size.width.as_f32();
        let height = logical_size.height.as_f32();
        anyhow::ensure!(
            width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0,
            "logical window dimensions must be positive and finite"
        );
        let key = self.window_key(window)?;
        self.platform
            .resize(self.windows[key].handle, logical_size, scale_factor)?;
        self.windows[key].redraw_pending = true;
        Ok(())
    }

    pub fn dispatch(
        &mut self,
        window: WgpuWindow,
        input: gpui::PlatformInput,
    ) -> anyhow::Result<gpui::DispatchEventResult> {
        let key = self.window_key(window)?;
        let result = self
            .platform
            .dispatch_input(self.windows[key].handle, input)?;
        self.windows[key].redraw_pending = true;
        self.pump();
        Ok(result)
    }

    pub fn dispatch_text(&mut self, window: WgpuWindow, text: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!text.is_empty(), "committed text must not be empty");
        let key = self.window_key(window)?;
        self.platform
            .window(self.windows[key].handle)
            .ok_or(WgpuWindowError::Closed)?
            .dispatch_text(text);
        self.windows[key].redraw_pending = true;
        self.pump();
        Ok(())
    }

    pub fn dispatch_text_editing(
        &mut self,
        window: WgpuWindow,
        editing: TextPreedit,
    ) -> anyhow::Result<()> {
        let key = self.window_key(window)?;
        self.platform
            .window(self.windows[key].handle)
            .ok_or(WgpuWindowError::Closed)?
            .dispatch_text_editing(&editing.text, editing.selection_utf16);
        self.windows[key].redraw_pending = true;
        self.pump();
        Ok(())
    }

    pub fn set_window_focus(&mut self, window: WgpuWindow, focused: bool) -> anyhow::Result<()> {
        let key = self.window_key(window)?;
        self.platform
            .set_active(self.windows[key].handle, focused)?;
        self.windows[key].redraw_pending = true;
        self.pump();
        Ok(())
    }

    pub fn set_window_hovered(&mut self, window: WgpuWindow, hovered: bool) -> anyhow::Result<()> {
        let key = self.window_key(window)?;
        self.platform
            .set_hovered(self.windows[key].handle, hovered)?;
        self.windows[key].redraw_pending = true;
        self.pump();
        Ok(())
    }

    pub fn wake(&mut self) {
        self.platform.wake();
        self.pump();
        for window in self.windows.values_mut() {
            window.redraw_pending = true;
        }
    }

    pub fn take_redraw_requests(&mut self) -> Vec<WgpuWindow> {
        self.windows
            .iter_mut()
            .filter_map(|(key, entry)| {
                std::mem::take(&mut entry.redraw_pending).then_some(WgpuWindow {
                    runtime_id: self.runtime_id,
                    key: key.data().as_ffi(),
                })
            })
            .collect()
    }

    pub fn set_clipboard_text(&self, text: String) {
        self.platform.set_clipboard_text(text);
    }

    pub fn clipboard_text(&self) -> Option<String> {
        self.platform.clipboard_text()
    }

    pub fn close_window(&mut self, window: WgpuWindow) -> anyhow::Result<()> {
        let key = self.window_key(window)?;
        self.platform.force_close(self.windows[key].handle)?;
        let closed = self
            .windows
            .remove(key)
            .expect("validated runtime window must still exist");
        self.attached_entities.remove(&closed.root_entity_id);
        self.pump();
        Ok(())
    }

    pub fn request_close_window(&mut self, window: WgpuWindow) -> anyhow::Result<CloseOutcome> {
        let key = self.window_key(window)?;
        let handle = self.windows[key].handle;
        self.platform.request_close(handle)?;
        self.pump();
        if self.platform.window(handle).is_some() {
            Ok(CloseOutcome::KeptOpen)
        } else {
            let closed = self
                .windows
                .remove(key)
                .expect("validated runtime window must still exist");
            self.attached_entities.remove(&closed.root_entity_id);
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

    /// Returns renderer cache diagnostics for validation builds.
    #[cfg(feature = "test-support")]
    pub fn render_cache_stats(
        &self,
        window: WgpuWindow,
    ) -> anyhow::Result<crate::WgpuRenderCacheStats> {
        let key = self.window_key(window)?;
        Ok(self.windows[key].render.borrow().renderer.cache_stats())
    }

    /// Confirms that two windows reuse the same immutable GPU resources.
    #[cfg(feature = "test-support")]
    pub fn windows_share_renderer_resources(
        &self,
        first: WgpuWindow,
        second: WgpuWindow,
    ) -> anyhow::Result<bool> {
        let first = self.windows[self.window_key(first)?].render.borrow();
        let second = self.windows[self.window_key(second)?].render.borrow();
        Ok(first.renderer.shares_resources_with(&second.renderer))
    }

    pub fn pump(&mut self) {
        for window in self.windows.values() {
            if let Ok(mut render) = window.render.try_borrow_mut() {
                render.renderer.poll();
            }
        }
        match self.execution_mode {
            WgpuExecutionMode::Realtime => {
                self.dispatcher.run_ready_main_tasks();
            }
            WgpuExecutionMode::Deterministic => self.dispatcher.run_until_idle(),
        }
    }

    pub(crate) fn pump_until_idle(&mut self) {
        self.dispatcher.run_until_idle();
    }

    fn window_key(&self, window: WgpuWindow) -> anyhow::Result<RuntimeWindowKey> {
        if window.runtime_id != self.runtime_id {
            return Err(WgpuWindowError::ForeignRuntime.into());
        }
        let key = RuntimeWindowKey::from(KeyData::from_ffi(window.key));
        if !self.windows.contains_key(key) {
            return Err(WgpuWindowError::Closed.into());
        }
        Ok(key)
    }

    fn window_handle(&self, window: WgpuWindow) -> anyhow::Result<gpui::AnyWindowHandle> {
        Ok(self.windows[self.window_key(window)?].handle)
    }
}

impl Drop for WgpuRuntime {
    fn drop(&mut self) {
        let handles: Vec<_> = self.windows.values().map(|window| window.handle).collect();
        for handle in handles {
            let _ = self.platform.force_close(handle);
        }
        self.windows.clear();
        self.attached_entities.clear();
        self.pump();
    }
}

impl WgpuRuntimeBuilder {
    pub fn execution_mode(mut self, execution_mode: WgpuExecutionMode) -> Self {
        self.execution_mode = execution_mode;
        self
    }

    pub fn default_scale_factor(mut self, scale_factor: f32) -> Self {
        self.default_scale_factor = scale_factor;
        self
    }

    pub fn default_appearance(mut self, appearance: gpui::WindowAppearance) -> Self {
        self.default_appearance = appearance;
        self
    }

    pub fn text_system(mut self, text_system: Arc<dyn gpui::PlatformTextSystem>) -> Self {
        self.text_system = text_system;
        self
    }

    pub fn assets(mut self, asset_source: Arc<dyn gpui::AssetSource>) -> Self {
        self.asset_source = asset_source;
        self
    }

    pub fn build(self) -> anyhow::Result<WgpuRuntime> {
        WgpuRuntime::new_configured(self)
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
