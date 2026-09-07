//! Minimal routing skeleton for an SDL-owned multi-window application.
//!
//! SDL still creates the native windows and swapchains. `WgpuRuntime` owns the
//! GPUI application and maps each SDL `WindowID` to one generational GPUI
//! window handle.

use std::collections::HashMap;

use gpui_sdl::{RoutedSdlHostEvent, SdlPlatformBridge, SdlWindowId, SdlWindowRouter, Viewport};
use gpui_wgpu::{CloseOutcome, WgpuRuntime, WgpuWindow};
use sdl3_sys::everything as sdl;

struct NativeWindow {
    gpui: WgpuWindow,
    scale_factor: f32,
}

/// Register this immediately after SDL creates a native window.
fn register(
    router: &mut SdlWindowRouter,
    windows: &mut HashMap<SdlWindowId, NativeWindow>,
    sdl_id: SdlWindowId,
    gpui: WgpuWindow,
    scale_factor: f32,
) -> anyhow::Result<()> {
    router.register_window(sdl_id, Viewport::default())?;
    windows.insert(sdl_id, NativeWindow { gpui, scale_factor });
    Ok(())
}

/// Call while the SDL-owned pointers inside `event` remain valid.
unsafe fn route_event(
    router: &mut SdlWindowRouter,
    event: &sdl::SDL_Event,
    runtime: &mut WgpuRuntime,
    bridge: &mut SdlPlatformBridge,
    windows: &mut HashMap<SdlWindowId, NativeWindow>,
) -> anyhow::Result<bool> {
    let mut outcome = Ok(true);
    let mut closed_window = None;
    // SAFETY: the caller guarantees that SDL populated the event and keeps
    // all borrowed event payloads alive for this complete call.
    unsafe {
        router.adapt_into(event, |routed| {
            if outcome.is_err() {
                return;
            }
            outcome = match routed {
                RoutedSdlHostEvent::Window { window_id, event } => {
                    let Some(window) = windows.get(&window_id) else {
                        return;
                    };
                    bridge
                        .dispatch_runtime_event(runtime, window.gpui, event, window.scale_factor)
                        .map(|()| true)
                }
                RoutedSdlHostEvent::CloseRequested { window_id } => {
                    let Some(window) = windows.get(&window_id) else {
                        return;
                    };
                    match runtime.request_close_window(window.gpui) {
                        Ok(CloseOutcome::Closed) => {
                            windows.remove(&window_id);
                            closed_window = Some(window_id);
                            Ok(true)
                        }
                        Ok(CloseOutcome::KeptOpen) => Ok(true),
                        Err(error) => Err(error),
                    }
                }
                RoutedSdlHostEvent::Quit => Ok(false),
            };
        });
    }
    if let Some(window_id) = closed_window {
        router.remove_window(window_id);
    }
    outcome
}

/// Moving a panel between an embedded and detached SDL window preserves its
/// GPUI `EntityId`; application state and subscriptions are not reconstructed.
fn move_panel<V: gpui::Render + 'static>(
    runtime: &mut WgpuRuntime,
    from: WgpuWindow,
    panel: &gpui::Entity<V>,
    destination_size: gpui::Size<gpui::Pixels>,
) -> anyhow::Result<WgpuWindow> {
    let panel = runtime.detach_window(from, panel)?;
    runtime.open_window_with_entity(destination_size, panel)
}

fn main() {
    // The engine supplies SDL initialization, native window/swapchain
    // creation, shared ExternalGpu, and one render_window call per redraw.
    let _ = register;
    let _ = route_event;
    let _ = move_panel::<PlaceholderPanel>;
}

struct PlaceholderPanel;

impl gpui::Render for PlaceholderPanel {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
    }
}
