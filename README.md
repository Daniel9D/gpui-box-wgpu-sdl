# GPUI Box wgpu

A standalone renderer extracted from `rust-engine`'s GPUI Box fork. Reuse your
wgpu device and queue, render GPUI into your texture, and keep window creation,
swapchain management, presentation, and native events in your application.
Requires Rust 1.97+ and wgpu 30.0.1.

GPUI Box is pinned to `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`.
See [UPSTREAM.md](UPSTREAM.md) for provenance and licenses.

## Add to a project

```toml
[dependencies]
gpui_wgpu = { package = "gpui-box-wgpu", path = "../gpui-box-wgpu" }
gpui = { package = "gpui-box", git = "https://github.com/fran0220/gpui-box.git", rev = "5c7e9eb6de8c8db3e7ff659934166218fb60f9f2" }
```

Use `gpui_wgpu::wgpu` for matching wgpu types. All GPUI dependencies in the
consuming project must use the same source and revision to share Rust types.

After pushing to your remote, replace these placeholders with its URL and an
immutable commit. This is a template, not a published URL:

```toml
gpui_wgpu = { package = "gpui-box-wgpu", git = "https://YOUR_HOST/YOUR_OWNER/gpui-box-wgpu", rev = "YOUR_40_CHARACTER_COMMIT" }
```

The package supports path and Git dependencies; crates.io publication is disabled.

| Feature | Behavior |
| --- | --- |
| default (empty) | Renderer, text system, native external-device rendering |
| `host` | Native `WgpuHost`, GPUI headless context, image capture APIs |
| `test-support` | Compatibility alias enabling `host` |
| `font-kit` | System font discovery |

`host` enables `gpui/test-support` and the direct optional `image` dependency
because the pinned GPUI headless APIs require them. Default production builds
do not activate those features; upstream GPUI may use image libraries
transitively. Development tests enable GPUI test support separately.

## Render an existing GPUI scene

```no_run
use gpui_wgpu::{WgpuHeadlessRenderer, wgpu};
use std::sync::Arc;

fn render(
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    scene: &gpui::Scene,
    target: &wgpu::TextureView,
) -> anyhow::Result<()> {
    // Construct once and retain the renderer between frames in your application.
    let mut renderer = WgpuHeadlessRenderer::from_external(
        instance, adapter, device, queue, wgpu::TextureFormat::Rgba8Unorm,
    )?;
    renderer.render_scene_to_view(
        scene,
        gpui::size(gpui::DevicePixels(800), gpui::DevicePixels(600)),
        target,
    )
}
```

Text and sprites require scene resources from the renderer's atlas. Use
`WgpuHost` to let GPUI build the scene and share the atlas automatically.

## Host a GPUI view

Enable `features = ["host"]` on the `gpui_wgpu` dependency:

```no_run
# #[cfg(all(not(target_family = "wasm"), feature = "host"))]
fn embed(
    gpu: gpui_wgpu::ExternalGpu,
    target: &gpui_wgpu::wgpu::TextureView,
) -> anyhow::Result<()> {
    use gpui::{prelude::*, div, px, rgb, size};
    use gpui_wgpu::{CosmicTextSystem, WgpuHost, wgpu};
    use std::sync::Arc;

    struct Root;
    impl gpui::Render for Root {
        fn render(
            &mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            div().size_full().bg(rgb(0x172033)).child("Hello GPUI")
        }
    }

    let mut host = WgpuHost::new(
        gpu,
        wgpu::TextureFormat::Rgba8Unorm,
        size(px(800.0), px(600.0)),
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
        |_, cx| cx.new(|_| Root),
    )?;
    // Initial rendering builds GPUI's hit-test and focus dispatch tree.
    host.render_to_view(target, wgpu::Extent3d {
        width: 800, height: 600, depth_or_array_layers: 1,
    }, 1.0)?;
    let result = host.dispatch(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
        position: gpui::point(px(12.0), px(12.0)),
        pressed_button: None,
        modifiers: gpui::Modifiers::default(),
    }))?;
    // Let other systems handle events GPUI did not consume.
    let _ = result.propagate;
    host.dispatch_text("olá")?;
    Ok(())
}
```

Retain the host between frames and render after state changes. It does not own
an OS event loop or schedule presentation. GPUI's headless context uses its test
platform and deterministic executor, not a full native platform. Committed text
goes through the focused keystroke/input-handler path. Native clipboard, cursor,
IME composition, accessibility, and other platform services need an app adapter.

## GPU ownership and target contract

- The application creates GPU resources and the output texture. The renderer
  retains `Arc<Device>` and `Arc<Queue>` and installs error/device-lost callbacks
  on that shared device.
- Supply a same-device, single-sample 2D color view matching the constructor's
  format, with `RENDER_ATTACHMENT` usage and matching physical dimensions. The
  format must also support `TEXTURE_BINDING`. GPUI clears and draws the target;
  composite separately to preserve another renderer's output.
- External rendering submits directly to the shared queue, with no CPU
  framebuffer or full-output readback/re-upload. Atlas uploads and optional small
  backdrop-luminance readbacks are renderer internals. Explicit image-capture
  APIs separately read pixels back when requested.
- `render_to_view` retains a temporary view handle only during the call and
  releases it on success or a returned error. The application presents afterward.
- Physical dimensions must be non-zero, one layer, and within device limits.
  Scale must be positive and finite; logical size is physical size divided by scale.

## Future SDL bridge

There is no SDL or winit dependency. Convert native events to
`gpui::PlatformInput` in the consuming application. For an embedded viewport:

```text
gpui_position = (native_position - viewport_origin) / viewport_scale
```

| SDL category | GPUI destination | Required state |
| --- | --- | --- |
| mouse motion | `MouseMoveEvent` | logical position, pressed button, modifiers |
| mouse down/up | `MouseDownEvent` / `MouseUpEvent` | position, button, click count, modifiers |
| wheel | `ScrollWheelEvent` | pointer position, pixel/line delta, phase |
| key down/up | `KeyDownEvent` / `KeyUpEvent` | normalized key, character, repeat, modifiers |
| modifiers | `ModifiersChangedEvent` | Ctrl, Alt, Shift, platform, function, Caps Lock |
| committed text | `WgpuHost::dispatch_text` | UTF-8 text from `SDL_EVENT_TEXT_INPUT` |
| composition | future platform adapter | marked range and `SDL_EVENT_TEXT_EDITING` data |
| resize/HiDPI | `render_to_view` | physical extent and positive scale factor |
| focus/cursor/clipboard | future bidirectional adapter | GPUI platform requests and SDL responses |

## Checks

```text
cargo fmt --all --check
cargo test --all-targets
cargo test --all-targets --all-features
cargo test --doc --all-features
cargo clippy --all-targets --all-features -- -D warnings -D clippy::cognitive_complexity
cargo doc --no-deps --all-features
```

Clippy cognitive complexity is denied at its default threshold of 25. This is
Clippy's supported complexity metric, not a numeric cyclomatic-complexity
measurement. New GPU tests explicitly skip if no adapter exists; failures after
acquisition fail the test. Inherited deterministic pixel tests require a software
adapter (Windows WARP or Linux llvmpipe).
