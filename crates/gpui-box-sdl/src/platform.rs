use std::{ffi::CStr, ptr};

use gpui::CursorStyle;
use sdl3_sys::everything as sdl;

/// Reads UTF-8 text from SDL's system clipboard.
pub fn clipboard_text() -> anyhow::Result<String> {
    let raw = unsafe { sdl::SDL_GetClipboardText() };
    anyhow::ensure!(
        !raw.is_null(),
        "SDL_GetClipboardText failed: {}",
        sdl_error()
    );
    let bytes = unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec();
    unsafe { sdl::SDL_free(raw.cast()) };
    String::from_utf8(bytes).map_err(|error| anyhow::anyhow!("SDL clipboard is not UTF-8: {error}"))
}

/// Writes UTF-8 text to SDL's system clipboard.
pub fn set_clipboard_text(text: &str) -> anyhow::Result<()> {
    let text = std::ffi::CString::new(text)
        .map_err(|_| anyhow::anyhow!("clipboard text contains an interior NUL"))?;
    anyhow::ensure!(
        unsafe { sdl::SDL_SetClipboardText(text.as_ptr()) },
        "SDL_SetClipboardText failed: {}",
        sdl_error()
    );
    Ok(())
}

/// Maps a GPUI cursor request to the closest SDL system cursor.
pub fn system_cursor(style: CursorStyle) -> sdl::SDL_SystemCursor {
    match style {
        CursorStyle::Arrow => sdl::SDL_SYSTEM_CURSOR_DEFAULT,
        CursorStyle::IBeam | CursorStyle::IBeamCursorForVerticalLayout => {
            sdl::SDL_SYSTEM_CURSOR_TEXT
        }
        CursorStyle::Crosshair => sdl::SDL_SYSTEM_CURSOR_CROSSHAIR,
        CursorStyle::ClosedHand | CursorStyle::OpenHand => sdl::SDL_SYSTEM_CURSOR_MOVE,
        CursorStyle::PointingHand
        | CursorStyle::DragLink
        | CursorStyle::DragCopy
        | CursorStyle::ContextualMenu => sdl::SDL_SYSTEM_CURSOR_POINTER,
        CursorStyle::ResizeLeft => sdl::SDL_SYSTEM_CURSOR_W_RESIZE,
        CursorStyle::ResizeRight => sdl::SDL_SYSTEM_CURSOR_E_RESIZE,
        CursorStyle::ResizeLeftRight | CursorStyle::ResizeColumn => {
            sdl::SDL_SYSTEM_CURSOR_EW_RESIZE
        }
        CursorStyle::ResizeUp => sdl::SDL_SYSTEM_CURSOR_N_RESIZE,
        CursorStyle::ResizeDown => sdl::SDL_SYSTEM_CURSOR_S_RESIZE,
        CursorStyle::ResizeUpDown | CursorStyle::ResizeRow => sdl::SDL_SYSTEM_CURSOR_NS_RESIZE,
        CursorStyle::ResizeUpLeftDownRight => sdl::SDL_SYSTEM_CURSOR_NWSE_RESIZE,
        CursorStyle::ResizeUpRightDownLeft => sdl::SDL_SYSTEM_CURSOR_NESW_RESIZE,
        CursorStyle::OperationNotAllowed => sdl::SDL_SYSTEM_CURSOR_NOT_ALLOWED,
    }
}

/// Owns the SDL system cursor currently installed for GPUI.
pub struct SdlCursor {
    raw: *mut sdl::SDL_Cursor,
    kind: Option<sdl::SDL_SystemCursor>,
}

impl SdlCursor {
    pub fn new() -> Self {
        Self {
            raw: ptr::null_mut(),
            kind: None,
        }
    }

    /// Applies GPUI's requested cursor shape and visibility on the SDL main thread.
    pub fn apply(&mut self, style: CursorStyle, visible: bool) -> anyhow::Result<()> {
        if !visible {
            anyhow::ensure!(
                unsafe { sdl::SDL_HideCursor() },
                "SDL_HideCursor failed: {}",
                sdl_error()
            );
            return Ok(());
        }
        anyhow::ensure!(
            unsafe { sdl::SDL_ShowCursor() },
            "SDL_ShowCursor failed: {}",
            sdl_error()
        );

        let kind = system_cursor(style);
        if self.kind == Some(kind) {
            return Ok(());
        }
        let raw = unsafe { sdl::SDL_CreateSystemCursor(kind) };
        anyhow::ensure!(
            !raw.is_null(),
            "SDL_CreateSystemCursor failed: {}",
            sdl_error()
        );
        if !unsafe { sdl::SDL_SetCursor(raw) } {
            let error = sdl_error();
            unsafe { sdl::SDL_DestroyCursor(raw) };
            anyhow::bail!("SDL_SetCursor failed: {error}");
        }
        if !self.raw.is_null() {
            unsafe { sdl::SDL_DestroyCursor(self.raw) };
        }
        self.raw = raw;
        self.kind = Some(kind);
        Ok(())
    }
}

impl Default for SdlCursor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SdlCursor {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { sdl::SDL_DestroyCursor(self.raw) };
        }
    }
}

/// Synchronizes SDL's UTF-8 clipboard and cursor with a [`gpui_wgpu::WgpuHost`].
#[derive(Default)]
pub struct SdlPlatformBridge {
    #[cfg(feature = "wgpu-runtime")]
    cursor: SdlCursor,
    #[cfg(feature = "wgpu-runtime")]
    last_clipboard: Option<String>,
}

impl SdlPlatformBridge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Imports the current SDL clipboard before dispatching paste input to GPUI.
    #[cfg(feature = "wgpu-runtime")]
    #[deprecated(note = "use pull_runtime_clipboard with WgpuRuntime")]
    pub fn pull_clipboard(&mut self, host: &mut gpui_wgpu::WgpuHost) -> anyhow::Result<()> {
        let text = clipboard_text()?;
        host.set_clipboard_text(text.clone());
        self.last_clipboard = Some(text);
        Ok(())
    }

    /// Exports changed GPUI clipboard text after dispatching input or actions.
    #[cfg(feature = "wgpu-runtime")]
    #[deprecated(note = "use push_runtime_clipboard with WgpuRuntime")]
    pub fn push_clipboard(&mut self, host: &mut gpui_wgpu::WgpuHost) -> anyhow::Result<()> {
        let Some(text) = host.clipboard_text() else {
            return Ok(());
        };
        if self.last_clipboard.as_ref() != Some(&text) {
            set_clipboard_text(&text)?;
            self.last_clipboard = Some(text);
        }
        Ok(())
    }

    /// Applies the host cursor state after input dispatch or a rendered frame.
    #[cfg(feature = "wgpu-runtime")]
    #[deprecated(note = "use sync_runtime_cursor with WgpuRuntime")]
    pub fn sync_cursor(&mut self, host: &gpui_wgpu::WgpuHost) -> anyhow::Result<()> {
        self.cursor
            .apply(host.cursor_style(), host.is_cursor_visible())
    }

    /// Imports SDL's UTF-8 clipboard into the shared embedded runtime.
    #[cfg(feature = "wgpu-runtime")]
    pub fn pull_runtime_clipboard(
        &mut self,
        runtime: &gpui_wgpu::WgpuRuntime,
    ) -> anyhow::Result<()> {
        let text = clipboard_text()?;
        runtime.set_clipboard_text(text.clone());
        self.last_clipboard = Some(text);
        Ok(())
    }

    /// Exports changed UTF-8 text from the embedded runtime to SDL.
    #[cfg(feature = "wgpu-runtime")]
    pub fn push_runtime_clipboard(
        &mut self,
        runtime: &gpui_wgpu::WgpuRuntime,
    ) -> anyhow::Result<()> {
        let Some(text) = runtime.clipboard_text() else {
            return Ok(());
        };
        if self.last_clipboard.as_ref() != Some(&text) {
            set_clipboard_text(&text)?;
            self.last_clipboard = Some(text);
        }
        Ok(())
    }

    /// Applies the cursor requested by one runtime window on SDL's main thread.
    #[cfg(feature = "wgpu-runtime")]
    pub fn sync_runtime_cursor(
        &mut self,
        runtime: &gpui_wgpu::WgpuRuntime,
        window: gpui_wgpu::WgpuWindow,
    ) -> anyhow::Result<()> {
        let state = runtime.window_state(window)?;
        self.cursor.apply(state.cursor_style, state.cursor_visible)
    }

    /// Dispatches one window-scoped SDL adapter event to an embedded GPUI window.
    #[cfg(feature = "wgpu-runtime")]
    pub fn dispatch_runtime_event(
        &mut self,
        runtime: &mut gpui_wgpu::WgpuRuntime,
        window: gpui_wgpu::WgpuWindow,
        event: crate::SdlHostEvent,
        scale_factor: f32,
    ) -> anyhow::Result<()> {
        match event {
            crate::SdlHostEvent::Input(input) => {
                runtime.dispatch(window, input)?;
            }
            crate::SdlHostEvent::TextInput(text) => runtime.dispatch_text(window, &text)?,
            crate::SdlHostEvent::TextEditing(editing) => {
                let selection_utf16 = editing.selection_utf16();
                runtime.dispatch_text_editing(
                    window,
                    gpui_wgpu::TextPreedit {
                        text: editing.text,
                        selection_utf16,
                    },
                )?;
            }
            crate::SdlHostEvent::WindowResized { width, height } => runtime.resize_window(
                window,
                gpui::size(
                    gpui::px(width as f32 / scale_factor),
                    gpui::px(height as f32 / scale_factor),
                ),
                scale_factor,
            )?,
            crate::SdlHostEvent::FocusChanged(focused) => {
                runtime.set_window_focus(window, focused)?;
            }
            crate::SdlHostEvent::Quit => {}
        }
        Ok(())
    }
}

fn sdl_error() -> String {
    let raw = sdl::SDL_GetError();
    if raw.is_null() {
        "unknown SDL error".to_owned()
    } else {
        unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned()
    }
}
