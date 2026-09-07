# GPUI Box SDL

`gpui-box-sdl` translates raw SDL3 events into renderer-independent GPUI Box
input and explicit host actions. It does not own an SDL window, event loop,
swapchain, GPU device, or presentation.

The crate pins `sdl3-sys` to `0.6.8` and builds SDL from source by default so
the standalone tests and the intended engine stack use the same ABI. Consumers
that provide a compatible system SDL3 can disable default features.

## Dependency

Use both workspace packages by path:

```toml
[dependencies]
gpui_sdl = { package = "gpui-box-sdl", path = "../gpui-box-wgpu/crates/gpui-box-sdl", features = ["build-from-source"] }
gpui_wgpu = { package = "gpui-box-wgpu", path = "../gpui-box-wgpu", features = ["host"] }
```

After pushing this repository, the same packages can come from one Git source.
Replace the URL and revision with the repository's real remote and an immutable
commit:

```toml
[dependencies]
gpui_sdl = { package = "gpui-box-sdl", git = "https://github.com/YOUR_OWNER/gpui-box-wgpu.git", rev = "YOUR_40_CHARACTER_COMMIT", features = ["build-from-source"] }
gpui_wgpu = { package = "gpui-box-wgpu", git = "https://github.com/YOUR_OWNER/gpui-box-wgpu.git", rev = "YOUR_40_CHARACTER_COMMIT", features = ["host"] }
```

All direct GPUI dependencies in the consumer must use the same GPUI Box source
and revision as these crates so their Rust types are identical.

## Event routing

Call `adapt` while the raw event and any SDL-owned string pointers inside it
are still valid. Route normal input to the GPUI host and retain control of
resize, focus, quit, and IME policy in the application:

```no_run
use gpui_sdl::{RoutedSdlHostEvent, SdlPlatformBridge, SdlWindowId, SdlWindowRouter};
use gpui_wgpu::{WgpuRuntime, WgpuWindow};
use sdl3_sys::everything as sdl;
use std::collections::HashMap;

fn deliver(
    runtime: &mut WgpuRuntime,
    bridge: &mut SdlPlatformBridge,
    windows: &HashMap<SdlWindowId, WgpuWindow>,
    events: Vec<RoutedSdlHostEvent>,
) -> anyhow::Result<()> {
    for event in events {
        match event {
            RoutedSdlHostEvent::Window { window_id, event } => {
                if let Some(&window) = windows.get(&window_id) {
                    bridge.dispatch_runtime_event(runtime, window, event, 1.0)?;
                }
            }
            RoutedSdlHostEvent::CloseRequested { window_id } => {
                // Ask GPUI first; destroy the SDL window only if it closes.
                let _ = windows.get(&window_id);
            }
            RoutedSdlHostEvent::Quit => break,
        }
    }
    Ok(())
}

unsafe fn adapt_one(
    router: &mut SdlWindowRouter,
    event: &sdl::SDL_Event,
    runtime: &mut WgpuRuntime,
    bridge: &mut SdlPlatformBridge,
    windows: &HashMap<SdlWindowId, WgpuWindow>,
) -> anyhow::Result<()> {
    // SAFETY: SDL populated `event`; its borrowed pointers remain valid for
    // the complete call to `adapt`.
    deliver(runtime, bridge, windows, unsafe { router.adapt(event) })
}

let _router = SdlWindowRouter::new();

fn sync_platform(
    runtime: &WgpuRuntime,
    window: WgpuWindow,
    bridge: &mut SdlPlatformBridge,
) -> anyhow::Result<()> {
    // Pull before dispatching a paste shortcut.
    bridge.pull_runtime_clipboard(runtime)?;
    // Push after GPUI actions, then apply the cursor after input or rendering.
    bridge.push_runtime_clipboard(runtime)?;
    bridge.sync_runtime_cursor(runtime, window)
}
# Ok::<(), anyhow::Error>(())
```

`adapt` is unsafe because raw `SDL_Event` unions are publicly constructible and
text events contain borrowed pointers. The caller must pass an event populated
by SDL whose pointers remain valid for the duration of the call. Null text
pointers are safely ignored by the adapter.

Mouse coordinates are logical SDL window coordinates. For an embedded GPUI
region, the adapter applies:

```text
gpui_position = (sdl_position - viewport_origin) / viewport_scale
```

`SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED` produces physical dimensions. Pass that
extent and the current display scale to `WgpuRuntime::render_window`; do not apply
the input viewport transform to physical resize values.

## SDL3 input coverage

`SdlInputAdapter::adapt` currently recognizes these SDL3 events:

| Category | SDL3 events | GPUI Box output or behavior |
| --- | --- | --- |
| Pointer motion | `SDL_EVENT_MOUSE_MOTION` | `MouseMove`, with coordinates transformed through `Viewport` and the retained pressed-button state. |
| Pointer buttons | `SDL_EVENT_MOUSE_BUTTON_DOWN`, `SDL_EVENT_MOUSE_BUTTON_UP` | `MouseDown` and `MouseUp` for left, right, middle, X1, and X2. X1/X2 map to back/forward navigation. |
| Mouse wheel | `SDL_EVENT_MOUSE_WHEEL` | `ScrollWheel` in line units, including `SDL_MOUSEWHEEL_FLIPPED`. |
| Pointer exit | `SDL_EVENT_WINDOW_MOUSE_LEAVE` | `MouseExited`. |
| Keyboard | `SDL_EVENT_KEY_DOWN`, `SDL_EVENT_KEY_UP` | `KeyDown` and `KeyUp`, including repeat and modifier state. |
| Committed text | `SDL_EVENT_TEXT_INPUT` | `SdlHostEvent::TextInput`; routed to the focused GPUI input handler as UTF-8. |
| IME preedit | `SDL_EVENT_TEXT_EDITING` | `SdlHostEvent::TextEditing`; SDL character offsets are converted to GPUI UTF-16 ranges. |
| File drop | `SDL_EVENT_DROP_BEGIN`, `SDL_EVENT_DROP_FILE`, `SDL_EVENT_DROP_POSITION`, `SDL_EVENT_DROP_COMPLETE` | Accumulates UTF-8 file paths and emits one GPUI `FileDrop` session: `Entered`, `Submit`, then `Ended`. Drop positions update the pointer; files are submitted on completion. |
| Physical resize | `SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED` | `SdlHostEvent::WindowResized` with physical pixel dimensions. |
| Focus | `SDL_EVENT_WINDOW_FOCUS_GAINED`, `SDL_EVENT_WINDOW_FOCUS_LOST` | `SdlHostEvent::FocusChanged`; losing focus also clears retained buttons and modifiers. |
| Window close | `SDL_EVENT_WINDOW_CLOSE_REQUESTED` | `RoutedSdlHostEvent::CloseRequested` retaining the SDL `WindowID`. |
| Application quit | `SDL_EVENT_QUIT` | `SdlHostEvent::Quit`. |

### Keyboard coverage

- Printable ASCII and Unicode keycodes are supported. Printable key names are
  normalized to lowercase for stable shortcut matching.
- Ctrl, Alt, Shift, GUI/Command, and Caps Lock are tracked. Key-repeat is
  preserved on `KeyDown`.
- Named keys cover navigation and editing, F1-F24, locks and system keys,
  standard and extended keypad keys, clipboard/edit commands, volume and media
  controls, browser/application-control keys, and mobile/meta/hyper keys.
- Keyboard events do not populate `key_char`. Text is emitted exclusively from
  `SDL_EVENT_TEXT_INPUT`, avoiding duplicate text when a key also has a printable
  keycode.
- Unknown, non-printable keycodes without a named mapping produce no key event.
  Modifier state is still updated when applicable.

### Platform bridge

Clipboard and cursor support are synchronized explicitly through
`SdlPlatformBridge`; they are not emitted as `SdlHostEvent` input events.

- The system clipboard supports UTF-8 text import and export only. Images,
  custom MIME formats, and primary selections are not supported.
- Every GPUI `CursorStyle` is mapped to the closest SDL system cursor. Cursor
  visibility and native cursor lifetime are managed by the bridge.

### Not currently supported

- Touch, pen/tablet, gesture, joystick, gamepad, and sensor input.
- `SDL_EVENT_DROP_TEXT`; drag-and-drop currently accepts files only.
- Clipboard change notifications such as `SDL_EVENT_CLIPBOARD_UPDATE`; callers
  pull and push UTF-8 text explicitly through the platform bridge.
- Full SDL text-input session start/stop and candidate-window control. GPUI
  exposes the active state and IME area through `WgpuRuntime::window_state`.
- Mouse buttons other than left, right, middle, X1, and X2.
- Window events other than pixel resize, focus, mouse leave, and close request.

All other SDL3 event categories return no adapter output.

See `examples/multi_window.rs` for zero-allocation callback routing, veto-aware
close handling, and the detach/reattach operation that preserves a panel's
`EntityId` when it returns to its origin.
