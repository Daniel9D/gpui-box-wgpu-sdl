use std::sync::Arc;

use crate::{WgpuExecutionMode, WgpuRuntime, WgpuWindow};

pub struct ExternalGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

/// Backward-compatible single-window facade over [`WgpuRuntime`].
pub struct WgpuHost {
    runtime: WgpuRuntime,
    window: WgpuWindow,
}

impl WgpuHost {
    pub fn new<V: gpui::Render + 'static>(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        initial_size: gpui::Size<gpui::Pixels>,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
        build_root: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> gpui::Entity<V>,
    ) -> anyhow::Result<Self> {
        let mut runtime = WgpuRuntime::builder(gpu, target_format, text_system, asset_source)
            .execution_mode(WgpuExecutionMode::Deterministic)
            .build()?;
        let (window, _) = runtime.open_window(initial_size, build_root)?;
        Ok(Self { runtime, window })
    }

    /// Updates the hosted window and application state.
    pub fn update<R>(
        &mut self,
        update: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R,
    ) -> anyhow::Result<R> {
        self.runtime.update_window(self.window, update)
    }

    /// Pumps queued work after advancing real time by the engine delta.
    ///
    /// This compatibility method waits for `elapsed`; new integrations should
    /// use [`WgpuRuntime::pump`] from their normal realtime event loop.
    pub fn tick(&mut self, elapsed: std::time::Duration) {
        self.runtime.pump_until_idle();
        std::thread::sleep(elapsed);
        self.runtime.pump_until_idle();
    }

    pub fn dispatch(
        &mut self,
        input: gpui::PlatformInput,
    ) -> anyhow::Result<gpui::DispatchEventResult> {
        self.runtime.dispatch(self.window, input)
    }

    pub fn dispatch_text(&mut self, text: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!text.is_empty(), "committed text must not be empty");
        let keystroke = gpui::Keystroke {
            modifiers: gpui::Modifiers::default(),
            key: String::new(),
            key_char: Some(text.to_owned()),
        };
        self.runtime.update_window(self.window, |window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        })?;
        self.runtime.pump();
        Ok(())
    }

    /// Imports UTF-8 text from the embedding platform's clipboard.
    pub fn set_clipboard_text(&mut self, text: String) {
        self.runtime.set_clipboard_text(text);
    }

    /// Returns UTF-8 text last written to GPUI's clipboard.
    pub fn clipboard_text(&mut self) -> Option<String> {
        self.runtime.clipboard_text()
    }

    /// Returns the cursor style requested during the latest paint.
    pub fn cursor_style(&self) -> gpui::CursorStyle {
        self.runtime
            .window_state(self.window)
            .map_or(gpui::CursorStyle::Arrow, |state| state.cursor_style)
    }

    /// Returns whether the embedding platform should display the cursor.
    pub fn is_cursor_visible(&self) -> bool {
        self.runtime
            .window_state(self.window)
            .is_ok_and(|state| state.cursor_visible)
    }

    pub fn render_to_view(
        &mut self,
        target: &wgpu::TextureView,
        physical_size: wgpu::Extent3d,
        scale_factor: f32,
    ) -> anyhow::Result<()> {
        self.runtime
            .render_window(self.window, target, physical_size, scale_factor)
    }

    /// Returns renderer cache diagnostics for validation builds.
    #[cfg(feature = "test-support")]
    pub fn render_cache_stats(&self) -> crate::WgpuRenderCacheStats {
        self.runtime
            .render_cache_stats(self.window)
            .expect("the compatibility window remains open for the host lifetime")
    }
}
