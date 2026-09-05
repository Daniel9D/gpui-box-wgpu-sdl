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
use gpui_sdl::{SdlHostEvent, SdlInputAdapter, Viewport};
use gpui_wgpu::WgpuHost;
use sdl3_sys::everything as sdl;

fn deliver(host: &mut WgpuHost, events: Vec<SdlHostEvent>) -> anyhow::Result<()> {
    for event in events {
        match event {
            SdlHostEvent::Input(input) => {
                host.dispatch(input)?;
            }
            SdlHostEvent::TextInput(text) => host.dispatch_text(&text)?,
            SdlHostEvent::TextEditing(editing) => {
                // Forward preedit state to the application's future IME bridge.
                let _ = editing;
            }
            SdlHostEvent::WindowResized { width, height } => {
                // Recreate/resize the caller-owned target before render_to_view.
                let _ = (width, height);
            }
            SdlHostEvent::FocusChanged(focused) => {
                let _ = focused;
            }
            SdlHostEvent::Quit => {}
        }
    }
    Ok(())
}

unsafe fn adapt_one(
    adapter: &mut SdlInputAdapter,
    event: &sdl::SDL_Event,
    host: &mut WgpuHost,
) -> anyhow::Result<()> {
    // SAFETY: SDL populated `event`; its borrowed pointers remain valid for
    // the complete call to `adapt`.
    deliver(host, unsafe { adapter.adapt(event) })
}

let _adapter = SdlInputAdapter::new(Viewport::default())?;
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
extent and the current display scale to `WgpuHost::render_to_view`; do not apply
the input viewport transform to physical resize values.

## Initial event coverage

- Mouse motion, five buttons, wheel, and mouse leave.
- Key down/up, repeat, Ctrl/Alt/Shift/GUI, and Caps Lock.
- Committed UTF-8 text and distinct IME preedit data.
- Physical pixel resize, focus gained/lost, and quit.

Unsupported event categories return no output. Clipboard, cursor commands,
touch, gamepads, accessibility, drag-and-drop, and full bidirectional IME
integration remain application or future adapter responsibilities.
