use std::{
    cell::{Cell, RefCell},
    ops::Range,
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::Arc,
};

use anyhow::Result;
use futures::channel::oneshot;
use gpui::{
    Action, AnyWindowHandle, BackgroundExecutor, Bounds, Capslock, ClipboardItem, CursorStyle,
    DispatchEventResult, DummyKeyboardMapper, ForegroundExecutor, GpuSpecs, Keymap, Menu, MenuItem,
    Modifiers, NativeMenuNotSupportedError, PathPromptOptions, Pixels, Platform, PlatformAtlas,
    PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformKeyboardLayout,
    PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point, PromptButton, PromptLevel,
    RequestFrameOptions, Scene, Size, Task, ThermalState, ThreadedDispatcher, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowParams, point, px, size,
};
use raw_window_handle::{HandleError, HasDisplayHandle, HasWindowHandle};

type InputCallback = Box<dyn FnMut(PlatformInput) -> DispatchEventResult>;
type ResizeCallback = Box<dyn FnMut(Size<Pixels>, f32)>;
type HitTestCallback = Box<dyn FnMut(Point<Pixels>) -> Option<WindowControlArea>>;
type RenderCallback = Box<dyn FnMut(&Scene)>;
type LuminanceCallback = Box<dyn FnMut(u32) -> Option<f32>>;

#[derive(Debug)]
pub(crate) struct EmbeddedDisplay;

impl PlatformDisplay for EmbeddedDisplay {
    fn id(&self) -> gpui::DisplayId {
        gpui::DisplayId::new(1)
    }

    fn uuid(&self) -> Result<uuid::Uuid> {
        Ok(uuid::Uuid::nil())
    }

    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0)))
    }
}

pub(crate) struct EmbeddedPlatform {
    background: BackgroundExecutor,
    foreground: ForegroundExecutor,
    text_system: Arc<dyn PlatformTextSystem>,
    atlas: Arc<dyn PlatformAtlas>,
    display: Rc<dyn PlatformDisplay>,
    windows: RefCell<std::collections::HashMap<gpui::WindowId, EmbeddedWindow>>,
    active: Cell<Option<AnyWindowHandle>>,
    cursor: Cell<CursorStyle>,
    cursor_visible: Cell<bool>,
    clipboard: RefCell<Option<ClipboardItem>>,
    system_wake: RefCell<Option<Box<dyn FnMut()>>>,
    default_scale_factor: f32,
    appearance: Cell<WindowAppearance>,
    weak: Weak<Self>,
}

struct EmbeddedWindowState {
    handle: AnyWindowHandle,
    bounds: Bounds<Pixels>,
    scale_factor: f32,
    active: bool,
    hovered: bool,
    fullscreen: bool,
    mouse_position: Point<Pixels>,
    modifiers: Modifiers,
    capslock: Capslock,
    atlas: Arc<dyn PlatformAtlas>,
    platform: Weak<EmbeddedPlatform>,
    input_handler: Option<PlatformInputHandler>,
    request_frame: Option<Box<dyn FnMut(RequestFrameOptions)>>,
    input: Option<InputCallback>,
    active_changed: Option<Box<dyn FnMut(bool)>>,
    hovered_changed: Option<Box<dyn FnMut(bool)>>,
    resized: Option<ResizeCallback>,
    moved: Option<Box<dyn FnMut()>>,
    should_close: Option<Box<dyn FnMut() -> bool>>,
    close: Option<Box<dyn FnOnce()>>,
    hit_test: Option<HitTestCallback>,
    appearance_changed: Option<Box<dyn FnMut()>>,
    appearance: WindowAppearance,
    render: Option<RenderCallback>,
    backdrop_luminance: Option<LuminanceCallback>,
    ime_area: Option<Bounds<Pixels>>,
}

#[derive(Clone)]
pub(crate) struct EmbeddedWindow(Rc<RefCell<EmbeddedWindowState>>);

impl EmbeddedPlatform {
    #[cfg(test)]
    pub(crate) fn new(
        dispatcher: Arc<ThreadedDispatcher>,
        text_system: Arc<dyn PlatformTextSystem>,
        atlas: Arc<dyn PlatformAtlas>,
        display: Rc<dyn PlatformDisplay>,
    ) -> Rc<Self> {
        Self::new_with_defaults(
            dispatcher,
            text_system,
            atlas,
            display,
            1.0,
            WindowAppearance::Light,
        )
    }

    pub(crate) fn new_with_defaults(
        dispatcher: Arc<ThreadedDispatcher>,
        text_system: Arc<dyn PlatformTextSystem>,
        atlas: Arc<dyn PlatformAtlas>,
        display: Rc<dyn PlatformDisplay>,
        default_scale_factor: f32,
        appearance: WindowAppearance,
    ) -> Rc<Self> {
        Rc::new_cyclic(|weak| Self {
            background: BackgroundExecutor::new(dispatcher.clone()),
            foreground: ForegroundExecutor::new(dispatcher.clone()),
            text_system,
            atlas,
            display,
            windows: RefCell::new(Default::default()),
            active: Cell::new(None),
            cursor: Cell::new(CursorStyle::Arrow),
            cursor_visible: Cell::new(true),
            clipboard: RefCell::new(None),
            system_wake: RefCell::new(None),
            default_scale_factor,
            appearance: Cell::new(appearance),
            weak: weak.clone(),
        })
    }

    pub(crate) fn window(&self, handle: AnyWindowHandle) -> Option<EmbeddedWindow> {
        self.windows.borrow().get(&handle.window_id()).cloned()
    }

    pub(crate) fn resize(
        &self,
        handle: AnyWindowHandle,
        size: Size<Pixels>,
        scale_factor: f32,
    ) -> Result<()> {
        anyhow::ensure!(
            scale_factor.is_finite() && scale_factor > 0.0,
            "invalid scale factor"
        );
        let window = self
            .window(handle)
            .ok_or_else(|| anyhow::anyhow!("window is closed"))?;
        let callback = {
            let mut state = window.0.borrow_mut();
            state.bounds.size = size;
            state.scale_factor = scale_factor;
            state.resized.take()
        };
        if let Some(mut callback) = callback {
            callback(size, scale_factor);
            if let Ok(mut state) = window.0.try_borrow_mut() {
                state.resized = Some(callback);
            }
        }
        Ok(())
    }

    pub(crate) fn set_active(&self, handle: AnyWindowHandle, active: bool) -> Result<()> {
        let window = self
            .window(handle)
            .ok_or_else(|| anyhow::anyhow!("window is closed"))?;
        if active {
            if let Some(previous) = self.active.replace(Some(handle))
                && previous != handle
                && let Some(previous) = self.window(previous)
            {
                previous.set_active(false);
            }
        } else if self.active.get() == Some(handle) {
            self.active.set(None);
        }
        window.set_active(active);
        Ok(())
    }

    pub(crate) fn request_close(&self, handle: AnyWindowHandle) -> Result<()> {
        self.window(handle)
            .ok_or_else(|| anyhow::anyhow!("window is closed"))?
            .request_close();
        Ok(())
    }

    pub(crate) fn request_frame(
        &self,
        handle: AnyWindowHandle,
        options: RequestFrameOptions,
    ) -> Result<()> {
        let window = self
            .window(handle)
            .ok_or_else(|| anyhow::anyhow!("window is closed"))?;
        let callback = window.0.borrow_mut().request_frame.take();
        if let Some(mut callback) = callback {
            callback(options);
            if let Ok(mut state) = window.0.try_borrow_mut() {
                state.request_frame = Some(callback);
            }
        }
        Ok(())
    }

    pub(crate) fn force_close(&self, handle: AnyWindowHandle) -> Result<()> {
        self.window(handle)
            .ok_or_else(|| anyhow::anyhow!("window is closed"))?
            .finish_close();
        Ok(())
    }

    pub(crate) fn dispatch_input(
        &self,
        handle: AnyWindowHandle,
        input: PlatformInput,
    ) -> Result<DispatchEventResult> {
        let window = self
            .window(handle)
            .ok_or_else(|| anyhow::anyhow!("window is closed"))?;
        let hovered = match &input {
            PlatformInput::MouseMove(_) => Some(true),
            PlatformInput::MouseExited(_) => Some(false),
            _ => None,
        };
        {
            let mut state = window.0.borrow_mut();
            match &input {
                PlatformInput::MouseMove(event) => {
                    state.mouse_position = event.position;
                    state.modifiers = event.modifiers;
                    self.cursor_visible.set(true);
                }
                PlatformInput::ModifiersChanged(event) => {
                    state.modifiers = event.modifiers;
                    state.capslock = event.capslock;
                }
                _ => {}
            }
        }
        if let Some(hovered) = hovered {
            window.set_hovered(hovered);
        }
        let callback = window.0.borrow_mut().input.take();
        let result = callback.map(|mut callback| {
            let result = callback(input);
            if let Ok(mut state) = window.0.try_borrow_mut() {
                state.input = Some(callback);
            }
            result
        });
        Ok(result.unwrap_or_default())
    }

    pub(crate) fn set_hovered(&self, handle: AnyWindowHandle, hovered: bool) -> Result<()> {
        self.window(handle)
            .ok_or_else(|| anyhow::anyhow!("window is closed"))?
            .set_hovered(hovered);
        Ok(())
    }

    pub(crate) fn wake(&self) {
        let callback = self.system_wake.borrow_mut().take();
        if let Some(mut callback) = callback {
            callback();
            self.system_wake.replace(Some(callback));
        }
    }

    pub(crate) fn set_clipboard_text(&self, text: String) {
        self.clipboard
            .replace(Some(ClipboardItem::new_string(text)));
    }

    pub(crate) fn clipboard_text(&self) -> Option<String> {
        self.clipboard
            .borrow()
            .as_ref()
            .and_then(ClipboardItem::text)
    }

    pub(crate) fn cursor_style(&self) -> CursorStyle {
        self.cursor.get()
    }

    pub(crate) fn cursor_visible(&self) -> bool {
        self.cursor_visible.get()
    }
}

impl EmbeddedWindow {
    pub(crate) fn set_render_callback(&self, callback: RenderCallback) {
        self.0.borrow_mut().render = Some(callback);
    }

    pub(crate) fn set_luminance_callback(&self, callback: LuminanceCallback) {
        self.0.borrow_mut().backdrop_luminance = Some(callback);
    }

    fn set_active(&self, active: bool) {
        let callback = {
            let mut state = self.0.borrow_mut();
            if state.active == active {
                return;
            }
            state.active = active;
            state.active_changed.take()
        };
        if let Some(mut callback) = callback {
            callback(active);
            if let Ok(mut state) = self.0.try_borrow_mut() {
                state.active_changed = Some(callback);
            }
        }
    }

    fn set_hovered(&self, hovered: bool) {
        let callback = {
            let mut state = self.0.borrow_mut();
            if state.hovered == hovered {
                return;
            }
            state.hovered = hovered;
            state.hovered_changed.take()
        };
        if let Some(mut callback) = callback {
            callback(hovered);
            if let Ok(mut state) = self.0.try_borrow_mut() {
                state.hovered_changed = Some(callback);
            }
        }
    }

    pub(crate) fn dispatch_text(&self, text: &str) {
        self.with_input_handler(|handler| handler.replace_text_in_range(None, text));
    }

    pub(crate) fn dispatch_text_editing(&self, text: &str, selection_utf16: Option<Range<usize>>) {
        self.with_input_handler(|handler| {
            handler.replace_and_mark_text_in_range(None, text, selection_utf16)
        });
    }

    fn with_input_handler(&self, update: impl FnOnce(&mut PlatformInputHandler)) {
        let handler = self.0.borrow_mut().input_handler.take();
        if let Some(mut handler) = handler {
            update(&mut handler);
            let ime_area = handler.ime_candidate_bounds();
            if let Ok(mut state) = self.0.try_borrow_mut() {
                state.ime_area = ime_area;
                if state.input_handler.is_none() {
                    state.input_handler = Some(handler);
                }
            }
        }
    }

    pub(crate) fn text_input_active(&self) -> bool {
        self.0.borrow().input_handler.is_some()
    }

    pub(crate) fn ime_area(&self) -> Option<Bounds<Pixels>> {
        self.0.borrow().ime_area
    }

    fn finish_close(&self) {
        let (handle, platform, close) = {
            let mut state = self.0.borrow_mut();
            (state.handle, state.platform.clone(), state.close.take())
        };
        if let Some(close) = close {
            close();
        }
        if let Some(platform) = platform.upgrade() {
            platform.windows.borrow_mut().remove(&handle.window_id());
            if platform.active.get() == Some(handle) {
                platform.active.set(None);
            }
        }
    }
}

impl HasWindowHandle for EmbeddedWindow {
    fn window_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::WindowHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

impl HasDisplayHandle for EmbeddedWindow {
    fn display_handle(
        &self,
    ) -> std::result::Result<raw_window_handle::DisplayHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

struct EmbeddedKeyboardLayout;

impl PlatformKeyboardLayout for EmbeddedKeyboardLayout {
    fn id(&self) -> &str {
        "embedded"
    }
    fn name(&self) -> &str {
        "Embedded"
    }
}

impl Platform for EmbeddedPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        self.background.clone()
    }
    fn foreground_executor(&self) -> ForegroundExecutor {
        self.foreground.clone()
    }
    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.text_system.clone()
    }
    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(EmbeddedKeyboardLayout)
    }
    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(DummyKeyboardMapper)
    }
    fn on_keyboard_layout_change(&self, _: Box<dyn FnMut()>) {}
    fn on_thermal_state_change(&self, _: Box<dyn FnMut()>) {}
    fn thermal_state(&self) -> ThermalState {
        ThermalState::Nominal
    }
    fn run(&self, launch: Box<dyn FnOnce()>) {
        launch()
    }
    fn quit(&self) {}
    fn restart(&self, _: Option<PathBuf>) {}
    fn activate(&self, _: bool) {}
    fn hide(&self) {}
    fn hide_other_apps(&self) {}
    fn unhide_other_apps(&self) {}
    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        vec![self.display.clone()]
    }
    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(self.display.clone())
    }
    fn active_window(&self) -> Option<AnyWindowHandle> {
        self.active.get()
    }
    fn open_window(
        &self,
        handle: AnyWindowHandle,
        params: WindowParams,
    ) -> Result<Box<dyn PlatformWindow>> {
        let window = EmbeddedWindow(Rc::new(RefCell::new(EmbeddedWindowState {
            handle,
            bounds: params.bounds,
            scale_factor: self.default_scale_factor,
            active: false,
            hovered: false,
            fullscreen: false,
            mouse_position: Point::default(),
            modifiers: Modifiers::default(),
            capslock: Capslock::default(),
            atlas: self.atlas.clone(),
            platform: self.weak.clone(),
            input_handler: None,
            request_frame: None,
            input: None,
            active_changed: None,
            hovered_changed: None,
            resized: None,
            moved: None,
            should_close: None,
            close: None,
            hit_test: None,
            appearance_changed: None,
            appearance: self.appearance.get(),
            render: None,
            backdrop_luminance: None,
            ime_area: None,
        })));
        self.windows
            .borrow_mut()
            .insert(handle.window_id(), window.clone());
        Ok(Box::new(window))
    }
    fn window_appearance(&self) -> WindowAppearance {
        self.appearance.get()
    }
    fn set_window_appearance(&self, appearance: Option<WindowAppearance>) {
        let appearance = appearance.unwrap_or_default();
        self.appearance.set(appearance);
        let windows: Vec<_> = self.windows.borrow().values().cloned().collect();
        for window in windows {
            let callback = {
                let mut state = window.0.borrow_mut();
                state.appearance = appearance;
                state.appearance_changed.take()
            };
            if let Some(mut callback) = callback {
                callback();
                if let Ok(mut state) = window.0.try_borrow_mut() {
                    state.appearance_changed = Some(callback);
                }
            }
        }
    }
    fn open_url(&self, _: &str) {}
    fn on_open_urls(&self, _: Box<dyn FnMut(Vec<String>)>) {}
    fn prompt_for_paths(
        &self,
        _: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(None));
        rx
    }
    fn prompt_for_new_path(
        &self,
        _: &Path,
        _: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        let (tx, rx) = oneshot::channel();
        let _ = tx.send(Ok(None));
        rx
    }
    fn can_select_mixed_files_and_dirs(&self) -> bool {
        true
    }
    fn reveal_path(&self, _: &Path) {}
    fn on_quit(&self, _: Box<dyn FnMut()>) {}
    fn on_reopen(&self, _: Box<dyn FnMut()>) {}
    fn on_system_wake(&self, callback: Box<dyn FnMut()>) {
        self.system_wake.replace(Some(callback));
    }
    fn set_menus(&self, _: Vec<Menu>, _: &Keymap) {}
    fn set_dock_menu(&self, _: Vec<MenuItem>, _: &Keymap) {}
    fn on_app_menu_action(&self, _: Box<dyn FnMut(&dyn Action)>) {}
    fn on_will_open_app_menu(&self, _: Box<dyn FnMut()>) {}
    fn on_validate_app_menu_command(&self, _: Box<dyn FnMut(&dyn Action) -> bool>) {}
    fn app_path(&self) -> Result<PathBuf> {
        std::env::current_exe().map_err(Into::into)
    }
    fn path_for_auxiliary_executable(&self, name: &str) -> Result<PathBuf> {
        Ok(self.app_path()?.with_file_name(name))
    }
    fn set_cursor_style(&self, style: CursorStyle) {
        self.cursor.set(style)
    }
    fn hide_cursor_until_mouse_moves(&self) {
        self.cursor_visible.set(false)
    }
    fn is_cursor_visible(&self) -> bool {
        self.cursor_visible.get()
    }
    fn should_auto_hide_scrollbars(&self) -> bool {
        false
    }
    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        self.clipboard.borrow().clone()
    }
    fn write_to_clipboard(&self, item: ClipboardItem) {
        self.clipboard.replace(Some(item));
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn read_from_primary(&self) -> Option<ClipboardItem> {
        self.read_from_clipboard()
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn write_to_primary(&self, item: ClipboardItem) {
        self.write_to_clipboard(item)
    }
    fn write_credentials(&self, _: &str, _: &str, _: &[u8]) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }
    fn read_credentials(&self, _: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        Task::ready(Ok(None))
    }
    fn delete_credentials(&self, _: &str) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }
    fn register_url_scheme(&self, _: &str) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }
    fn open_with_system(&self, _: &Path) {}
}

impl PlatformWindow for EmbeddedWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        self.0.borrow().bounds
    }
    fn is_maximized(&self) -> bool {
        false
    }
    fn window_bounds(&self) -> WindowBounds {
        WindowBounds::Windowed(self.bounds())
    }
    fn content_size(&self) -> Size<Pixels> {
        self.0.borrow().bounds.size
    }
    fn resize(&mut self, size: Size<Pixels>) {
        self.0.borrow_mut().bounds.size = size
    }
    fn scale_factor(&self) -> f32 {
        self.0.borrow().scale_factor
    }
    fn appearance(&self) -> WindowAppearance {
        self.0.borrow().appearance
    }
    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        self.0
            .borrow()
            .platform
            .upgrade()
            .map(|p| p.display.clone())
    }
    fn mouse_position(&self) -> Point<Pixels> {
        self.0.borrow().mouse_position
    }
    fn modifiers(&self) -> Modifiers {
        self.0.borrow().modifiers
    }
    fn capslock(&self) -> Capslock {
        self.0.borrow().capslock
    }
    fn set_input_handler(&mut self, handler: PlatformInputHandler) {
        self.0.borrow_mut().input_handler = Some(handler)
    }
    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.0.borrow_mut().input_handler.take()
    }
    fn prompt(
        &self,
        _: PromptLevel,
        _: &str,
        _: Option<&str>,
        _: &[PromptButton],
    ) -> Option<oneshot::Receiver<usize>> {
        None
    }
    fn activate(&self) {
        if let Some(p) = self.0.borrow().platform.upgrade() {
            let _ = p.set_active(self.0.borrow().handle, true);
        }
    }
    fn is_active(&self) -> bool {
        self.0.borrow().active
    }
    fn is_hovered(&self) -> bool {
        self.0.borrow().hovered
    }
    fn background_appearance(&self) -> WindowBackgroundAppearance {
        WindowBackgroundAppearance::Opaque
    }
    fn set_title(&mut self, _: &str) {}
    fn set_background_appearance(&self, _: WindowBackgroundAppearance) {}
    fn minimize(&self) {}
    fn zoom(&self) {}
    fn request_close(&self) {
        let should_close = {
            let callback = self.0.borrow_mut().should_close.take();
            callback.is_none_or(|mut callback| {
                let result = callback();
                if let Ok(mut state) = self.0.try_borrow_mut() {
                    state.should_close = Some(callback);
                }
                result
            })
        };
        if should_close {
            self.finish_close();
        }
    }
    fn toggle_fullscreen(&self) {
        let fullscreen = self.0.borrow().fullscreen;
        self.0.borrow_mut().fullscreen = !fullscreen;
    }
    fn is_fullscreen(&self) -> bool {
        self.0.borrow().fullscreen
    }
    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.0.borrow_mut().request_frame = Some(callback)
    }
    fn on_input(&self, callback: InputCallback) {
        self.0.borrow_mut().input = Some(callback)
    }
    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.borrow_mut().active_changed = Some(callback)
    }
    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.borrow_mut().hovered_changed = Some(callback)
    }
    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        self.0.borrow_mut().resized = Some(callback)
    }
    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.0.borrow_mut().moved = Some(callback)
    }
    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.0.borrow_mut().should_close = Some(callback)
    }
    fn on_hit_test_window_control(
        &self,
        callback: Box<dyn FnMut(Point<Pixels>) -> Option<WindowControlArea>>,
    ) {
        self.0.borrow_mut().hit_test = Some(callback)
    }
    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.0.borrow_mut().close = Some(callback)
    }
    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        self.0.borrow_mut().appearance_changed = Some(callback)
    }
    fn draw(&self, scene: &Scene) {
        let render = self.0.borrow_mut().render.take();
        if let Some(mut render) = render {
            render(scene);
            if let Ok(mut state) = self.0.try_borrow_mut() {
                state.render = Some(render);
            }
        }
    }
    fn backdrop_luminance(&self, slot: u32) -> Option<f32> {
        let callback = self.0.borrow_mut().backdrop_luminance.take();
        callback.and_then(|mut callback| {
            let value = callback(slot);
            if let Ok(mut state) = self.0.try_borrow_mut() {
                state.backdrop_luminance = Some(callback);
            }
            value
        })
    }
    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.0.borrow().atlas.clone()
    }
    fn is_subpixel_rendering_supported(&self) -> bool {
        false
    }
    #[cfg(target_os = "windows")]
    fn get_raw_handle(&self) -> windows::Win32::Foundation::HWND {
        windows::Win32::Foundation::HWND::default()
    }
    fn show_context_menu(
        &self,
        _: Menu,
        _: Point<Pixels>,
    ) -> std::result::Result<oneshot::Receiver<Option<Box<dyn Action>>>, NativeMenuNotSupportedError>
    {
        Err(NativeMenuNotSupportedError)
    }
    fn gpu_specs(&self) -> Option<GpuSpecs> {
        None
    }
    fn update_ime_position(&self, bounds: Bounds<Pixels>) {
        self.0.borrow_mut().ime_area = Some(bounds)
    }
}
