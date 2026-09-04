# Standalone GPUI Box wgpu Fork Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the engine's GPUI Box wgpu fork into a standalone crate with default external-texture rendering and an optional platform-neutral host/input API.

**Architecture:** Preserve the pinned upstream renderer and its engine-compatible low-level API, but remove the `test-support` gate from external-GPU rendering. Add an opt-in `host` module that composes GPUI's feature-gated headless context with the renderer, accepts `gpui::PlatformInput`, and draws into a temporary caller-owned texture view without depending on SDL.

**Tech Stack:** Rust 2024, Rust 1.97+, GPUI Box revision `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`, wgpu 30.0.1, Clippy

**Spec:** `docs/superpowers/specs/2026-09-04-standalone-gpui-box-wgpu-design.md`

## Global Constraints

- Do not modify `C:\Users\daniel\Documents\GitHub\rust-engine` or remove its vendored fork.
- Keep package name `gpui-box-wgpu`, library name `gpui_wgpu`, edition 2024, and minimum Rust 1.97.
- Keep all GPUI Box packages pinned to revision `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`.
- The default feature set exposes external-device rendering without `image` or `gpui/test-support`.
- `host` enables `gpui/test-support` and `image`; `test-support` remains a compatibility alias for `host`.
- Do not add SDL, winit, or another native event dependency.
- Runtime rendering must not read a full frame back to the CPU or upload a CPU framebuffer.
- Deny `clippy::cognitive_complexity` at its default threshold of 25; do not add an allowance for it.
- Preserve Apache-2.0, bundled font licenses, Hash Function Prospector's Unlicense notice, and upstream provenance.

---

### Task 1: Import and normalize the standalone crate baseline

**Files:**
- Create: `Cargo.toml`
- Create: `Cargo.lock`
- Create: `.gitignore`
- Create: `LICENSE-APACHE`
- Create: `HASH-PROSPECTOR-UNLICENSE.txt`
- Create: `UPSTREAM.md`
- Create: `assets/fonts/**`
- Create: `src/backdrop_glass.wgsl`
- Create: `src/cosmic_text_system.rs`
- Create: `src/gpui_wgpu.rs`
- Create: `src/shaders.wgsl`
- Create: `src/shaders_storage.wgsl`
- Create: `src/shaders_subpixel.wgsl`
- Create: `src/shaders_webgl.wgsl`
- Create: `src/wgpu_atlas.rs`
- Create: `src/wgpu_context.rs`
- Create: `src/wgpu_renderer.rs`

**Interfaces:**
- Consumes: the exact source and assets under `rust-engine/vendor/gpui-box-wgpu`
- Produces: a standalone Cargo library baseline that builds independently and retains the fork's current behavior

- [ ] **Step 1: Copy the engine-vendored baseline without deleting its source**

Run from the new repository root:

```powershell
Copy-Item -Recurse -Force `
  'C:\Users\daniel\Documents\GitHub\rust-engine\vendor\gpui-box-wgpu\assets' `
  'C:\Users\daniel\Documents\GitHub\rust-engine\vendor\gpui-box-wgpu\src' `
  'C:\Users\daniel\Documents\GitHub\rust-engine\vendor\gpui-box-wgpu\LICENSE-APACHE' `
  'C:\Users\daniel\Documents\GitHub\rust-engine\vendor\gpui-box-wgpu\UPSTREAM.md' `
  .
Copy-Item -Force `
  'C:\Users\daniel\.cargo\git\checkouts\gpui-box-f90a8757c9b694e3\5c7e9eb\crates\gpui_wgpu\HASH-PROSPECTOR-UNLICENSE.txt' `
  .
```

Verify the source tree against the vendored baseline:

```powershell
git diff --no-index --stat -- `
  'C:\Users\daniel\Documents\GitHub\rust-engine\vendor\gpui-box-wgpu\src' `
  '.\src'
```

Expected: no source differences.

- [ ] **Step 2: Create the standalone manifest and development dependencies**

Create `Cargo.toml` from the vendored manifest, retain its pinned dependencies,
set `publish = false`, remove the incorrect upstream `repository` field, and use:

```toml
[package]
name = "gpui-box-wgpu"
version = "0.1.2"
edition = "2024"
rust-version = "1.97"
publish = false
license = "Apache-2.0"
description = "Embeddable wgpu renderer and generic host for GPUI Box"

[lib]
name = "gpui_wgpu"
path = "src/gpui_wgpu.rs"

[features]
default = []
font-kit = ["dep:font-kit"]
host = ["gpui/test-support", "dep:image"]
test-support = ["host"]
```

Retain the existing normal and target-specific dependency versions. Add the
upstream test-only dependencies that the vendored manifest omitted:

```toml
[dev-dependencies]
env_logger = "0.11"
naga = { version = "=30.0.0", features = ["wgsl-in"] }
```

- [ ] **Step 3: Add repository hygiene and generate a reproducible lockfile**

Create `.gitignore`:

```gitignore
/target/
```

Run:

```powershell
cargo generate-lockfile
cargo check
cargo test --all-targets --all-features --no-run
```

Expected: dependency resolution succeeds, including `js-sys 0.3.104`; all
existing test targets compile now that `env_logger` and `naga` are present.

- [ ] **Step 4: Confirm the engine was not changed**

Run:

```powershell
git -C 'C:\Users\daniel\Documents\GitHub\rust-engine' status --short
```

Record the output for comparison at final verification; do not alter it.

- [ ] **Step 5: Commit the baseline**

```powershell
git add Cargo.toml Cargo.lock .gitignore LICENSE-APACHE HASH-PROSPECTOR-UNLICENSE.txt UPSTREAM.md assets src
git commit -m "build: import standalone GPUI wgpu fork"
```

---

### Task 2: Expose external-device rendering in the default feature set

**Files:**
- Create: `tests/default_external_api.rs`
- Modify: `src/wgpu_renderer.rs`

**Interfaces:**
- Consumes: existing `WgpuContext::from_external` and engine-compatible renderer methods
- Produces: default-feature `WgpuHeadlessRenderer::from_external` and `render_scene_to_view`; image capture stays behind `host`

- [ ] **Step 1: Write the failing default-feature API test**

Create `tests/default_external_api.rs`:

```rust
#![cfg(not(target_family = "wasm"))]

use gpui_wgpu::WgpuHeadlessRenderer;

#[test]
fn external_renderer_api_is_available_without_optional_features() {
    let _constructor = WgpuHeadlessRenderer::from_external;
    let _render = WgpuHeadlessRenderer::render_scene_to_view;
}
```

The production break caught by this test is restoring the old
`feature = "test-support"` gate around either method.

- [ ] **Step 2: Run the test with default features and verify RED**

Run:

```powershell
cargo test --no-default-features --test default_external_api
```

Expected: compilation fails because `WgpuHeadlessRenderer` is not exported
without `test-support`.

- [ ] **Step 3: Move only the external rendering path outside the feature gate**

In `src/wgpu_renderer.rs`:

- gate `WgpuHeadlessRenderer` and its base `impl` only with
  `#[cfg(not(target_family = "wasm"))]`;
- gate `WgpuHeadlessRenderer::new`, `WgpuRenderer::new_headless`,
  `render_scene_to_image`, `render_scene_offscreen`,
  `render_scene_to_texture`, and the `PlatformHeadlessRenderer` implementation
  with `#[cfg(all(not(target_family = "wasm"), any(test, feature = "host")))]`;
- gate `new_headless_with_format`, `prepare_headless_frame`,
  `WgpuRenderer::render_scene_to_view`, `WgpuHeadlessRenderer::from_external`,
  and `WgpuHeadlessRenderer::render_scene_to_view` only by native target;
- add host-only crate-visible forwarding methods needed by Task 4:

```rust
#[cfg(feature = "host")]
pub(crate) fn sprite_atlas(&self) -> Arc<dyn gpui::PlatformAtlas> {
    self.renderer.sprite_atlas().clone()
}

#[cfg(feature = "host")]
pub(crate) fn backdrop_luminance(&mut self, slot: u32) -> Option<f32> {
    self.renderer.backdrop_luminance(slot)
}
```

Do not change the five-argument `from_external` signature used by the engine.

- [ ] **Step 4: Run default and feature tests and verify GREEN**

Run:

```powershell
cargo test --no-default-features --test default_external_api
cargo test --all-targets --features host --no-run
```

Expected: both commands pass.

- [ ] **Step 5: Commit the default external renderer API**

```powershell
git add src/wgpu_renderer.rs tests/default_external_api.rs
git commit -m "feat: expose external renderer by default"
```

---

### Task 3: Add validated host frame geometry

**Files:**
- Create: `src/wgpu_host.rs`
- Modify: `src/gpui_wgpu.rs`

**Interfaces:**
- Consumes: `wgpu::Extent3d`, scale factor, and GPUI pixel types
- Produces: private `HostFrame::new(extent, scale_factor)` used by the generic host render path

- [ ] **Step 1: Write failing pure tests for frame validation**

Start `src/wgpu_host.rs` with a private `HostFrame` test module covering these
literal cases:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_physical_extent_to_logical_size() {
        let frame = HostFrame::new(
            wgpu::Extent3d {
                width: 300,
                height: 150,
                depth_or_array_layers: 1,
            },
            1.5,
        )
        .unwrap();

        assert_eq!(frame.device_size, gpui::size(gpui::DevicePixels(300), gpui::DevicePixels(150)));
        assert_eq!(frame.logical_size, gpui::size(gpui::px(200.0), gpui::px(100.0)));
    }

    #[test]
    fn rejects_invalid_frame_geometry() {
        let extent = wgpu::Extent3d {
            width: 300,
            height: 150,
            depth_or_array_layers: 2,
        };
        assert!(HostFrame::new(extent, 1.0).is_err());

        let flat = wgpu::Extent3d { depth_or_array_layers: 1, ..extent };
        assert!(HostFrame::new(wgpu::Extent3d { width: 0, ..flat }, 1.0).is_err());
        assert!(HostFrame::new(flat, 0.0).is_err());
        assert!(HostFrame::new(flat, f32::NAN).is_err());
    }
}
```

The breaks caught are incorrect HiDPI division and acceptance of an invalid
texture target.

- [ ] **Step 2: Export the empty host module and verify RED**

Add to `src/gpui_wgpu.rs`:

```rust
#[cfg(all(not(target_family = "wasm"), feature = "host"))]
mod wgpu_host;

#[cfg(all(not(target_family = "wasm"), feature = "host"))]
pub use wgpu_host::*;
```

Run:

```powershell
cargo test --features host wgpu_host::tests::
```

Expected: compilation fails because `HostFrame` is undefined.

- [ ] **Step 3: Implement the minimal validated conversion**

Add:

```rust
#[derive(Clone, Copy, Debug)]
struct HostFrame {
    device_size: gpui::Size<gpui::DevicePixels>,
    logical_size: gpui::Size<gpui::Pixels>,
}

impl HostFrame {
    fn new(extent: wgpu::Extent3d, scale_factor: f32) -> anyhow::Result<Self> {
        anyhow::ensure!(extent.width > 0 && extent.height > 0, "target dimensions must be non-zero");
        anyhow::ensure!(extent.depth_or_array_layers == 1, "target must have exactly one layer");
        anyhow::ensure!(scale_factor.is_finite() && scale_factor > 0.0, "scale factor must be positive and finite");

        let width = i32::try_from(extent.width).context("target width exceeds i32")?;
        let height = i32::try_from(extent.height).context("target height exceeds i32")?;
        Ok(Self {
            device_size: gpui::size(gpui::DevicePixels(width), gpui::DevicePixels(height)),
            logical_size: gpui::size(
                gpui::px(extent.width as f32 / scale_factor),
                gpui::px(extent.height as f32 / scale_factor),
            ),
        })
    }
}
```

Import `anyhow::Context as _` at the top of the module.

- [ ] **Step 4: Verify GREEN and commit**

```powershell
cargo test --features host wgpu_host::tests::
git add src/gpui_wgpu.rs src/wgpu_host.rs
git commit -m "feat: validate embedded host frame geometry"
```

---

### Task 4: Build the generic host and direct render lifecycle

**Files:**
- Modify: `src/wgpu_host.rs`
- Create: `tests/wgpu_host_gpu.rs`

**Interfaces:**
- Consumes: `ExternalGpu`, caller text/assets/root view, and a caller-owned texture view
- Produces: `WgpuHost::new` and `WgpuHost::render_to_view`

- [ ] **Step 1: Write the failing same-device pixel integration test**

Create `tests/wgpu_host_gpu.rs`, gated with
`#![cfg(all(not(target_family = "wasm"), feature = "host"))]`. Reuse the
engine test's `read_texture`, `pixel`, and `assert_color_near` helpers, but make
adapter acquisition return `Option` so unavailable CI hardware prints a skip
message and returns.

Define a root view whose literal colors are `0x172033`, `0xef4444`, and
`0x3b82f6`, then construct:

```rust
let mut host = WgpuHost::new(
    ExternalGpu {
        instance,
        adapter,
        device: Arc::clone(&device),
        queue: Arc::clone(&queue),
    },
    view_format,
    gpui::size(gpui::px(256.0), gpui::px(128.0)),
    Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
    Arc::new(()),
    |_, cx| cx.new(|_| ProbeView),
)
.unwrap();

host.render_to_view(&view, extent, 1.0).unwrap();
```

Read back only in the test and assert the same three literal pixel colors as
`rust-engine/tests/gpui_direct_gpu.rs`. The production break caught is drawing
through another device or failing to submit GPUI commands to the supplied
view.

- [ ] **Step 2: Run the GPU test and verify RED**

```powershell
cargo test --features host --test wgpu_host_gpu -- --nocapture
```

Expected: compilation fails because `ExternalGpu` and `WgpuHost` are missing.

- [ ] **Step 3: Implement external resources and renderer delegation**

Add the public resource type exactly as specified:

```rust
pub struct ExternalGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}
```

Move the engine's proven `CurrentTarget`, `DirectRendererState`, and
`DirectRenderer` pattern into `wgpu_host.rs`. Implement
`gpui::PlatformHeadlessRenderer` for `DirectRenderer`; both trait render
methods route through one `render_direct` helper, and `render_scene_to_image`
returns only `image::RgbaImage::new(0, 0)` after the direct GPU draw.

- [ ] **Step 4: Implement generic construction and rendering**

Add the public constructor signature from the spec:

```rust
pub fn new<V: gpui::Render + 'static>(
    gpu: ExternalGpu,
    target_format: wgpu::TextureFormat,
    initial_size: gpui::Size<gpui::Pixels>,
    text_system: Arc<dyn gpui::PlatformTextSystem>,
    asset_source: Arc<dyn gpui::AssetSource>,
    build_root: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> gpui::Entity<V>,
) -> anyhow::Result<Self>
```

Construct one `WgpuHeadlessRenderer`, install the direct renderer factory in
`HeadlessAppContext::with_platform`, open the logical window with
`initial_size`, and store only its `AnyWindowHandle`.

Implement `render_to_view` using `HostFrame::new`. Set `CurrentTarget`, update
and resize the GPUI window, call `window.draw(cx).clear(cx)` followed by
`window.render_to_image().map(drop)`, then always clear the target and extract
the delegate's stored error before propagating either error. Keep target setup,
drawing, cleanup, and result combination in separate helpers so each function
stays below the cognitive-complexity threshold.

- [ ] **Step 5: Verify GREEN and commit**

```powershell
cargo test --features host --test wgpu_host_gpu -- --nocapture
cargo test --features host wgpu_host::tests::
git add src/wgpu_host.rs tests/wgpu_host_gpu.rs
git commit -m "feat: add generic external-texture GPUI host"
```

---

### Task 5: Add generic event and committed-text dispatch

**Files:**
- Modify: `src/wgpu_host.rs`
- Modify: `tests/wgpu_host_gpu.rs`

**Interfaces:**
- Consumes: `gpui::PlatformInput` and committed UTF-8 text
- Produces: `WgpuHost::dispatch` and `WgpuHost::dispatch_text`

- [ ] **Step 1: Write failing behavior tests against a real GPUI view**

Extend the GPU host test view with a `gpui::FocusHandle`, shared
`Rc<Cell<bool>>` mouse state, and `Rc<RefCell<String>>` text state. Track and
focus that handle when constructing the root. Register an `on_mouse_move`
handler that sets the first cell and an `on_key_down` handler that copies
`event.keystroke.key_char` into the text state. After one initial render builds
the dispatch tree, dispatch:

```rust
let result = host.dispatch(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
    position: gpui::point(gpui::px(12.0), gpui::px(12.0)),
    pressed_button: None,
    modifiers: gpui::Modifiers::default(),
}))?;
```

Assert the real view state changed. Call `host.dispatch_text("olá")` and assert
the key listener's state contains the literal `olá`. These are real GPUI event
listeners, not mocks. The breaks caught are bypassing GPUI's normal event path
and dropping committed Unicode text before it reaches the focused dispatch
path.

- [ ] **Step 2: Run the focused tests and verify RED**

```powershell
cargo test --features host --test wgpu_host_gpu dispatches_ -- --nocapture
```

Expected: compilation fails because the dispatch methods do not exist.

- [ ] **Step 3: Implement platform-neutral dispatch**

Add:

```rust
pub fn dispatch(
    &mut self,
    input: gpui::PlatformInput,
) -> anyhow::Result<gpui::DispatchEventResult> {
    let result = self.context.update_window(self.window, |_, window, cx| {
        window.dispatch_event(input, cx)
    })?;
    self.context.run_until_parked();
    Ok(result)
}
```

Add committed-text dispatch through GPUI's existing input-handler route:

```rust
pub fn dispatch_text(&mut self, text: &str) -> anyhow::Result<()> {
    let keystroke = gpui::Keystroke {
        modifiers: gpui::Modifiers::default(),
        key: String::new(),
        key_char: Some(text.to_owned()),
    };
    self.context.update_window(self.window, |_, window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    })?;
    self.context.run_until_parked();
    Ok(())
}
```

Reject an empty `text` string with `anyhow::ensure!` so it cannot create a
meaningless key event.

- [ ] **Step 4: Verify GREEN and commit**

```powershell
cargo test --features host --test wgpu_host_gpu dispatches_ -- --nocapture
cargo test --all-targets --all-features
git add src/wgpu_host.rs tests/wgpu_host_gpu.rs
git commit -m "feat: dispatch generic GPUI host input"
```

---

### Task 6: Document consumption and the future SDL bridge

**Files:**
- Create: `README.md`
- Modify: `UPSTREAM.md`

**Interfaces:**
- Consumes: finalized default renderer and `host` APIs
- Produces: path/Git setup, runnable API examples, ownership guarantees, and explicit future SDL input map

- [ ] **Step 1: Write README sections with exact supported behavior**

Create `README.md` with:

- project purpose and pinned upstream revision;
- `path` dependency using
  `gpui_wgpu = { package = "gpui-box-wgpu", path = "../gpui-box-wgpu" }`;
- Git dependency syntax clearly marked for replacement with the user's eventual
  remote URL and immutable revision;
- a low-level `WgpuHeadlessRenderer::from_external` example;
- a `features = ["host"]` example for `WgpuHost`;
- ownership rules: host supplies GPU resources and texture, crate retains
  `Arc<Device>`/`Arc<Queue>`, and the view is temporary;
- no CPU framebuffer/readback in runtime;
- no SDL/winit dependency.

- [ ] **Step 2: Add the future SDL event table**

Document this exact map:

| SDL category | GPUI destination | Required state |
|---|---|---|
| mouse motion | `MouseMoveEvent` | logical position, pressed button, modifiers |
| mouse down/up | `MouseDownEvent` / `MouseUpEvent` | position, button, click count, modifiers |
| wheel | `ScrollWheelEvent` | pointer position, pixel/line delta, phase |
| key down/up | `KeyDownEvent` / `KeyUpEvent` | normalized key, character, repeat, modifiers |
| modifiers | `ModifiersChangedEvent` | Ctrl, Alt, Shift, platform, function, Caps Lock |
| committed text | `WgpuHost::dispatch_text` | UTF-8 text from `SDL_EVENT_TEXT_INPUT` |
| composition | future platform adapter | marked range and `SDL_EVENT_TEXT_EDITING` data |
| resize/HiDPI | `render_to_view` | physical extent and positive scale factor |
| focus/cursor/clipboard | future bidirectional adapter | GPUI platform requests and SDL responses |

State the viewport transform:

```text
gpui_position = (native_position - viewport_origin) / viewport_scale
```

- [ ] **Step 3: Update provenance**

Update `UPSTREAM.md` to identify this standalone repository, retain the source
URL/revision/date/licenses, list the low-level external rendering delta, list
the optional generic host delta, and state that the engine vendor directory is
an unchanged earlier copy.

- [ ] **Step 4: Build documentation and commit**

```powershell
cargo doc --no-deps --all-features
git add README.md UPSTREAM.md
git commit -m "docs: explain embedding and future SDL input"
```

---

### Task 7: Enforce warning-free complexity gates and verify the release state

**Files:**
- Modify: `src/cosmic_text_system.rs`
- Modify: `src/wgpu_atlas.rs`
- Modify: `src/wgpu_renderer.rs`
- Modify: `src/wgpu_host.rs`

**Interfaces:**
- Consumes: complete standalone crate
- Produces: clean formatting, tests, docs, warnings, and cognitive-complexity verification

- [ ] **Step 1: Fix the three known Rust 1.98 Clippy warnings**

The baseline audit found exactly these `chunks_exact_mut(4)` warnings:

- `src/cosmic_text_system.rs:372` over `image.data`;
- `src/wgpu_atlas.rs:392` over `data`; and
- `src/wgpu_renderer.rs:1883` over `pixels`.

Replace each iterator with the suggested fixed-size slice form:

```rust
for pixel in image.data.as_chunks_mut::<4>().0 {
```

and equivalently `data.as_chunks_mut::<4>().0` / `pixels.as_chunks_mut::<4>().0`.
The remainder is deliberately ignored exactly as before because each buffer is
RGBA data and the existing code already ignored incomplete tails.

- [ ] **Step 2: Format and run the full test matrix**

```powershell
cargo fmt --all
cargo test --all-targets
cargo test --all-targets --all-features
```

Expected: all tests pass. A GPU test may print its explicit no-adapter skip and
return successfully; it must not silently catch rendering failures after an
adapter was acquired.

- [ ] **Step 3: Run the exact lint and documentation gates**

```powershell
cargo clippy --all-targets --all-features -- -D warnings -D clippy::cognitive_complexity
cargo fmt --all --check
cargo doc --no-deps --all-features
```

Expected: every command exits zero with no warning. If a new host function
exceeds cognitive complexity, split it along the setup/draw/cleanup boundary
defined in Task 4 and rerun the full command; do not add an `allow` attribute.

- [ ] **Step 4: Verify dependency and runtime boundaries**

```powershell
cargo tree --no-default-features | Select-String -Pattern '^image v|gpui-box.*test-support'
rg -n 'sdl|winit|read_texture|map_async|queue\.write_texture' Cargo.toml src
git -C 'C:\Users\daniel\Documents\GitHub\rust-engine' status --short
git status --short
```

Expected:

- default dependency output contains no direct optional `image` activation;
- no SDL or winit dependency/reference exists in production code;
- readback helpers exist only in integration tests;
- engine status matches the Task 1 snapshot; and
- only the intended final lint edits are uncommitted before the last commit.

- [ ] **Step 5: Commit the quality gate fixes**

```powershell
git add src/cosmic_text_system.rs src/wgpu_atlas.rs src/wgpu_renderer.rs
git commit -m "chore: enforce clean Clippy complexity gates"
git status --short --branch
```

Expected final status: clean `main` branch.
