# GPUI Box wgpu

A GPUI Box fork. Reuse your
wgpu device and queue, render GPUI into your texture, and keep window creation,
swapchain management, presentation, and native events in your application.
Requires Rust 1.97+ and wgpu 30.0.1.

GPUI Box is pinned to `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`.
See [UPSTREAM.md](UPSTREAM.md) for provenance and licenses.

## Add to a project

```toml
[dependencies]
gpui_wgpu = { package = "gpui-box-wgpu", path = "../gpui-box-wgpu" }
```

Use the `gpui_wgpu::wgpu` and `gpui_wgpu::gpui` re-exports so WGPU and GPUI
types always match this fork's vendored snapshot.

After pushing to your remote, replace these placeholders with its URL and an
immutable commit. This is a template, not a published URL:

```toml
gpui_wgpu = { package = "gpui-box-wgpu", git = "https://YOUR_HOST/YOUR_OWNER/gpui-box-wgpu", rev = "YOUR_40_CHARACTER_COMMIT" }
```

The package supports path and Git dependencies; crates.io publication is disabled.

| Feature         | Behavior                                                                    |
| --------------- | --------------------------------------------------------------------------- |
| default (empty) | Renderer, text system, native external-device rendering                     |
| `host`          | Native `WgpuHost`, GPUI headless context, image capture APIs                |
| `test-support`  | Compatibility alias enabling `host`                                         |
| `kit`           | `host` plus `gpui-box-kit` components with optional heavy features disabled |
| `font-kit`      | System font discovery                                                       |

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

## Render an engine-owned texture with `img`

Wrap the default full-resource view once and pass the cloneable handle directly
to `gpui::img`:

```no_run
use gpui_wgpu::{WgpuImage, gpui, wgpu};
use gpui::prelude::*;

fn image(texture: &wgpu::Texture) -> anyhow::Result<impl gpui::IntoElement> {
    let image = WgpuImage::new(texture.create_view(&Default::default()))?;
    Ok(gpui::img(image).size_full())
}
```

The texture stays owned by the engine. Queue writes submitted before
`WgpuHost::render_to_view` are visible in that frame without CPU readback or an
atlas upload. Recreate `WgpuImage` only when the engine recreates or resizes the
texture.

The underlying texture must be non-zero, single-sample, 2D, one layer, use a
filterable float format, include `TEXTURE_BINDING`, and belong to the host's WGPU
device. Pass its default full-resource view; array, cube, mip-subset, depth,
integer, multisampled, and device-foreign views are unsupported.

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

## SDL bridge

The workspace includes the renderer-independent
[`gpui-box-sdl`](crates/gpui-box-sdl/README.md) adapter. It converts raw SDL3
events into `gpui::PlatformInput` plus explicit text, IME, resize, focus, and
quit actions. SDL remains responsible for its window, event loop, swapchain,
and presentation. For an embedded viewport:

```text
gpui_position = (native_position - viewport_origin) / viewport_scale
```

| SDL category     | GPUI destination                  | Required state                                  |
| ---------------- | --------------------------------- | ----------------------------------------------- |
| mouse motion     | `MouseMoveEvent`                  | logical position, pressed button, modifiers     |
| mouse down/up    | `MouseDownEvent` / `MouseUpEvent` | position, button, click count, modifiers        |
| wheel            | `ScrollWheelEvent`                | pointer position, pixel/line delta, phase       |
| key down/up      | `KeyDownEvent` / `KeyUpEvent`     | normalized key, character, repeat, modifiers    |
| modifiers        | `ModifiersChangedEvent`           | Ctrl, Alt, Shift, platform, function, Caps Lock |
| committed text   | `WgpuHost::dispatch_text`         | UTF-8 text from `SDL_EVENT_TEXT_INPUT`          |
| composition      | `SdlHostEvent::TextEditing`       | marked range and `SDL_EVENT_TEXT_EDITING` data  |
| resize/HiDPI     | `render_to_view`                  | physical extent and positive scale factor       |
| focus            | `SdlHostEvent::FocusChanged`      | loss also clears retained input state           |
| cursor/clipboard | future bidirectional adapter      | GPUI platform requests and SDL responses        |

See the adapter README for `path`/Git dependencies, the raw-event safety
contract, and a complete routing example.

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

## GPUI Kit on the embedded host

Enable `features = ["kit"]` on `gpui-box-wgpu`. The feature uses
`gpui-box-kit` from the same pinned GPUI Box revision, with its optional native
media and terminal features disabled:

```toml
[dependencies]
gpui_wgpu = { package = "gpui-box-wgpu", path = "../gpui-box-wgpu", features = ["kit"] }
```

The `kit` feature enables `host`; default renderer builds do not compile the
kit. Import the matching vendored kit through `gpui_wgpu::gpui_kit`.

In the `WgpuHost::new` root builder, install the kit before constructing the
root view:

```ignore
// Pass Arc::new(gpui_kit::assets::Assets) as the host asset source.
move |window, cx| {
    use gpui_wgpu::gpui_kit;
    gpui_kit::install(cx);
    cx.new(|cx| MyView::new(window, cx))
}
```

Import controls through `gpui_kit::prelude` or their module paths. The kit does
not own the application bootstrap; SDL3 and your engine keep ownership of the
window and presentation.

Each engine frame:

1. Route SDL events through the SDL adapter and `host.dispatch`/`dispatch_text`.
2. Call `host.tick(elapsed)` even when there are no input events. Elapsed time
   is the delta since the last tick, not time since startup. This pumps tasks
   and advances the deterministic host clock for timers and animations.
3. Render into an engine-owned UI texture with `host.render_to_view`.
4. Composite that texture over the engine scene on the GPU, then present.

The UI target must use the same device and constructor format, with
`RENDER_ATTACHMENT | TEXTURE_BINDING` usage. The renderer clears its target;
use a separate target to preserve your scene. Configure transparent UI/root
backgrounds where the scene should remain visible. Match the composition blend
state to the rendered texture's alpha representation; no CPU readback is needed.

`host.update(|window, cx| ...)` allows changes to entities, focus, globals and
clipboard. Clipboard in this headless host is in-process: synchronize it with
SDL explicitly for OS copy/paste. IME composition, native cursors, accessibility
transport, native menus, additional OS windows and real-I/O scheduling are not
provided by this integration. The headless context installs a fake HTTP client
that returns 404; components needing network resources require an
engine-provided service. `tick` retains the test dispatcher, not a native
production event loop.

The focused integration check is:

```text
cargo test --features kit --test wgpu_host_gpu -- --test-threads=1
```

It exercises external-device rendering, a kit button, Unicode input and
in-process clipboard paste. The host test suite separately covers
engine-driven timers. A GPU adapter is required for execution; without one the
GPU tests report a skip.
