# Standalone GPUI Box wgpu Fork Design

## Objective

Extract the locally extended `gpui-box-wgpu` crate from
`C:\Users\daniel\Documents\GitHub\rust-engine\vendor\gpui-box-wgpu` into this
repository as an independent Rust crate. The extracted crate must be reusable
from any native Rust project through a Cargo `path` or Git dependency, while
the original vendored copy and the engine remain unchanged.

The crate provides two levels of integration:

1. a low-level renderer that reuses host-owned wgpu resources and renders a
   GPUI `Scene` into a caller-owned `TextureView`; and
2. a platform-neutral GPUI host that owns the logical GPUI application/window
   state, accepts GPUI input events, and renders into a caller-owned target.

SDL, winit, and other window/event libraries remain outside this repository.

## Source and compatibility boundary

The initial source is the `gpui-box-wgpu` crate copied from the engine vendor
directory. That copy is based on GPUI Box revision
`5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`, source path
`crates/gpui_wgpu`, under Apache-2.0.

The extraction preserves:

- the Apache-2.0 license and bundled font licenses;
- the upstream source revision and provenance record;
- the crate package name `gpui-box-wgpu` and library name `gpui_wgpu`;
- the existing public API used by `rust-engine`;
- all GPUI Box dependencies at the same pinned Git revision; and
- native and WebAssembly code inherited from upstream, except that the new
  external-GPU and host APIs are native-only.

The repository does not modify, delete, or repoint the engine's vendored copy.
The package is intended for Cargo `path` and Git dependencies. Publishing to
crates.io is outside scope because its GPUI sibling dependencies are pinned to
a Git revision.

## Package and feature model

The repository root is a single library crate. Its default build exposes the
native external-GPU rendering API without requiring screenshot or generic-host
support.

The existing features remain narrowly scoped:

- `font-kit` enables system font discovery;
- `host` enables `gpui/test-support`, the direct `image` dependency required
  by GPUI's headless renderer trait, and the generic `WgpuHost` API;
- `test-support` enables `host` and preserves the existing feature name for
  compatibility; and
- the default feature set remains empty.

The `image` dependency stays optional and is not required for rendering a
scene into a host texture. APIs whose signatures contain `image::RgbaImage`
remain gated by `host`. The low-level external renderer is not gated by
`host` or `test-support` on native platforms.

This feature boundary is required by the pinned GPUI Box revision:
`HeadlessAppContext`, `PlatformHeadlessRenderer`, and the synchronous scene
dispatch hook are exported only by `gpui/test-support`. Making the generic
host available without that feature would require maintaining a fork of the
central `gpui-box` crate, which is deliberately outside scope.

## Low-level external renderer

The existing compatibility API remains source-compatible:

```rust
impl WgpuContext {
    pub fn from_external(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
    ) -> anyhow::Result<Self>;
}

impl WgpuHeadlessRenderer {
    pub fn from_external(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        target_format: wgpu::TextureFormat,
    ) -> anyhow::Result<Self>;

    pub fn render_scene_to_view(
        &mut self,
        scene: &gpui::Scene,
        size: gpui::Size<gpui::DevicePixels>,
        target: &wgpu::TextureView,
    ) -> anyhow::Result<()>;
}
```

Construction retains the exact host-provided `Device` and `Queue`, installs
GPUI's device callbacks, selects an atlas format supported by the adapter, and
validates the requested target format. Rendering validates non-zero dimensions
and maximum texture size, updates the drawable size, advances the atlas frame,
and submits GPUI draw commands directly to the supplied view.

The renderer never creates a native window, event loop, or swapchain. The
runtime path performs no full-frame GPU readback or CPU re-upload and does not
retain the target view after the render call.

## Generic host API

With the `host` feature, the crate adds a native `WgpuHost` that composes
GPUI's headless application context with `WgpuHeadlessRenderer`. It owns:

- `gpui::HeadlessAppContext`;
- the logical GPUI window handle;
- the direct renderer delegate and temporary target state; and
- the current logical size and scale factor.

GPU ownership is expressed with a small value object:

```rust
pub struct ExternalGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}
```

The constructor accepts the external GPU resources, target format, initial
logical size, platform text system, asset source, and a closure that creates
the caller's root GPUI view. The view type is generic only at construction and
is erased behind GPUI's `AnyWindowHandle`, so `WgpuHost` itself is not generic.

The primary methods are:

```rust
impl WgpuHost {
    pub fn new<V: gpui::Render + 'static>(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        initial_size: gpui::Size<gpui::Pixels>,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
        build_root: impl FnOnce(
            &mut gpui::Window,
            &mut gpui::App,
        ) -> gpui::Entity<V>,
    ) -> anyhow::Result<Self>;

pub fn dispatch(
    &mut self,
    input: gpui::PlatformInput,
) -> anyhow::Result<gpui::DispatchEventResult>;

pub fn dispatch_text(&mut self, text: &str) -> anyhow::Result<()>;

pub fn render_to_view(
    &mut self,
    target: &wgpu::TextureView,
    physical_size: wgpu::Extent3d,
    scale_factor: f32,
) -> anyhow::Result<()>;
}
```

`dispatch` forwards mouse, keyboard, modifier, wheel, touch, and drop events
through `Window::dispatch_event`. Its result lets a containing application
decide whether to continue propagating an event to other systems.

`dispatch_text` handles committed UTF-8 text. It does not claim to implement
IME composition. Text editing/composition remains a separate platform bridge
because it requires bidirectional interaction with GPUI's input handler.

`render_to_view` derives the logical window size from physical dimensions and
the positive, finite scale factor; resizes and draws the logical GPUI window;
and renders the resulting scene into the temporary target. The target is
cleared from host state after every attempt, including failures.

## Event flow and platform boundary

The intended platform-independent flow is:

```text
native event library
  -> application-specific event normalization
  -> gpui::PlatformInput
  -> WgpuHost::dispatch
  -> GPUI focus, hit testing, handlers, and state updates
  -> WgpuHost::render_to_view
  -> host-owned TextureView
  -> application compositor or swapchain
```

The crate does not define a second generic input enum. `gpui::PlatformInput`
already provides the narrow common contract, and another enum would add
mapping code without isolating a real dependency.

For embedded viewports, callers transform pointer coordinates before dispatch:

```text
gpui_position = (native_position - viewport_origin) / viewport_scale
```

The README includes a concise dispatch example but no SDL-specific dependency
or implementation.

## Future SDL input map

The following work is documented but not implemented in this extraction:

- SDL mouse motion to `gpui::MouseMoveEvent`;
- SDL mouse buttons to `MouseDownEvent` and `MouseUpEvent`, including pressed
  button state and click count;
- SDL wheel deltas to `ScrollWheelEvent` with the appropriate pixel/line unit;
- SDL key down/up and repeat state to GPUI keystrokes;
- SDL modifier changes to `ModifiersChangedEvent`;
- viewport-origin, logical-pixel, physical-pixel, and HiDPI conversion;
- committed `SDL_EVENT_TEXT_INPUT` text;
- `SDL_EVENT_TEXT_EDITING` composition, marked ranges, and IME candidate bounds;
- GPUI-driven start/stop text input and cursor selection;
- focus, mouse capture, clipboard, drag-and-drop, touch, and accessibility.

Complete IME, clipboard, cursor, and accessibility support requires a
bidirectional platform adapter. It must live in the consuming application or a
future dedicated adapter crate, not in the wgpu renderer.

## Errors and invariants

- External construction fails when the adapter cannot support the atlas or
  target format required by the renderer.
- `render_to_view` rejects zero dimensions, depth other than one, non-finite or
  non-positive scale factors, and logical dimensions outside GPUI's range.
- The target texture must have been created from the supplied device. wgpu
  validation is the final identity check.
- The temporary target is removed from shared state whether drawing succeeds
  or fails, preventing stale views from surviving resize or error recovery.
- Dispatch fails if the logical window no longer exists.
- Platform event conversion errors belong to the platform adapter, not this
  crate.

## Complexity policy

The crate treats `clippy::cognitive_complexity` as an error with Clippy's
default threshold of 25. This is the available Rust/Clippy lint corresponding
to the requested cyclomatic-complexity constraint.

No new `allow(clippy::cognitive_complexity)` exemption may be introduced.
Functions reported by the lint are split by responsibility while preserving
behavior. Existing targeted allowances inherited from upstream for unrelated
lints remain only where their rationale still applies.

## Test strategy

Development follows red-green-refactor cycles for behavior changes after the
source baseline is copied unchanged.

1. A default-feature API test proves that `ExternalGpu`, `WgpuHost`,
   `WgpuHeadlessRenderer::from_external`, and `render_scene_to_view` are
   available without `test-support`.
2. Pure tests cover physical-to-logical size validation and reject invalid
   dimensions and scale factors without requiring a GPU.
3. Host tests dispatch representative pointer and keyboard inputs into a real
   GPUI view and assert observable view state rather than mocks.
4. A native GPU integration test creates one instance, adapter, device, and
   queue, renders into a texture from that device, reads it back only in the
   test, and verifies distinct expected pixels. It reports an explicit skip
   when no compatible adapter exists.
5. `host` tests exercise the generic host and image-renderer bridge separately
   from the default external-rendering path; `test-support` remains a
   compatibility alias for this capability.
6. Documentation examples are compiled where practical.

Completion requires fresh successful runs of:

```text
cargo fmt --check
cargo test --all-targets
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings -D clippy::cognitive_complexity
cargo doc --no-deps --all-features
```

## Documentation and consumption

The README documents both supported dependency forms:

```toml
gpui_wgpu = { package = "gpui-box-wgpu", path = "../gpui-box-wgpu" }
```

```toml
gpui_wgpu = { package = "gpui-box-wgpu", git = "https://host/owner/gpui-box-wgpu", rev = "40-character-git-commit" }
```

The README labels the Git URL and revision as a substitution pattern because
this local directory currently has no configured remote or published commit.
It must not present invented repository metadata as a usable dependency.

The README also distinguishes the low-level renderer API from `WgpuHost`,
shows host-owned GPU resources and a caller-owned texture target, and links to
the future SDL input map.

## Success criteria

- This repository is a standalone `gpui-box-wgpu` library crate.
- It is consumable by Cargo through a local path or Git checkout.
- The engine repository and its vendored fork are unchanged.
- The existing engine-facing external renderer API remains compatible.
- External rendering is available without `test-support` or optional image
  capture dependencies.
- With `features = ["host"]`, `WgpuHost` can construct a caller-defined GPUI
  view, dispatch generic GPUI input, accept committed text, and render directly
  into a host texture.
- Runtime rendering contains no full-frame readback or CPU upload.
- The future SDL/IME/platform work is explicitly mapped without introducing an
  SDL dependency.
- Formatting, tests, documentation, warning-free Clippy, and the cognitive
  complexity lint all pass.
