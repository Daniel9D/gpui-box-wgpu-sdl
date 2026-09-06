# Multi-window WGPU Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add one external-WGPU GPUI runtime that opens, updates, renders, and closes multiple GPUI windows while preserving the existing single-window `WgpuHost` API.

**Architecture:** `WgpuRuntime` owns the single `HeadlessAppContext`, renderer, atlas, text system, and asset source. Each caller-owned `WgpuWindow` carries only its GPUI handle and per-window logical size, scale, cursor, and open/closed state; rendering installs one temporary caller-owned target at a time.

**Tech Stack:** Rust 2024, GPUI Box, WGPU 30.0.1, `WgpuHeadlessRenderer`, real-GPU integration tests.

**Spec:** `docs/superpowers/specs/2026-09-06-multi-window-wgpu-runtime-design.md`

## Global Constraints

- Work only in `C:\Users\daniel\Documents\GitHub\gpui-box-wgpu`.
- Keep the crate platform-neutral: no SDL window ownership, surfaces, docking, detachable components, or egui.
- Reuse the caller-provided `Instance`, `Adapter`, `Arc<Device>`, and `Arc<Queue>`.
- Support one target texture format per runtime.
- Render windows sequentially and clear the temporary target after success or failure.
- Keep `WgpuHost` source-compatible for current consumers.
- Do not add CPU readback or an image upload fallback to runtime code.
- Take the existing GPU-test serialization guard in every test that creates a device.

---

## File Structure

- `src/wgpu_runtime.rs`: shared external GPU context, direct renderer bridge, `WgpuRuntime`, `WgpuWindow`, frame validation, multiwindow operations.
- `src/wgpu_host.rs`: backward-compatible single-window facade delegating to `WgpuRuntime`.
- `src/gpui_wgpu.rs`: module declaration and public reexports.
- `tests/wgpu_host_gpu.rs`: real-GPU multiwindow rendering, isolation, input, close, and failure recovery tests alongside existing helpers.
- `tests/default_external_api.rs`: compile-time proof that the new public API is exported only with `host`.
- `README.md`: single-runtime/multiple-window usage and ownership contract.

---

### Task 1: Open and render two windows from one runtime

**Files:**
- Create: `src/wgpu_runtime.rs`
- Modify: `src/gpui_wgpu.rs`
- Modify: `tests/wgpu_host_gpu.rs`

**Interfaces:**
- Consumes: existing `ExternalGpu`, `WgpuHeadlessRenderer::from_external`, `HeadlessAppContext::open_window`, and the direct renderer bridge currently in `wgpu_host.rs`.
- Produces: `WgpuRuntime::new`, `WgpuRuntime::open_window`, `WgpuRuntime::render_window`, `WgpuRuntime::tick`, `WgpuWindow`, and typed root entities.

- [ ] **Step 1: Write the failing two-window GPU test**

Import `WgpuRuntime` in `tests/wgpu_host_gpu.rs`, add a full-size solid view, and add a test that uses the existing `gpu`, `read_texture`, `pixel`, and GPU serialization helpers:

```rust
struct SolidView(u32);

impl Render for SolidView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(self.0))
    }
}

#[test]
fn one_runtime_renders_two_independent_windows() {
    let _gpu = gpu_test_guard();
    let Some((instance, adapter, device, queue)) = gpu() else { return };
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let (red_texture, red_view) = render_target(&device, "runtime_red");
    let (green_texture, green_view) = render_target(&device, "runtime_green");
    let mut runtime = WgpuRuntime::new(
        ExternalGpu {
            instance,
            adapter,
            device: Arc::clone(&device),
            queue: Arc::clone(&queue),
        },
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
    )
    .unwrap();
    let (mut red_window, red_root) = runtime
        .open_window(gpui::size(px(256.), px(128.)), |_, cx| {
            cx.new(|_| SolidView(0xff0000))
        })
        .unwrap();
    let (mut green_window, _) = runtime
        .open_window(gpui::size(px(256.), px(128.)), |_, cx| {
            cx.new(|_| SolidView(0x00ff00))
        })
        .unwrap();

    runtime.render_window(&mut red_window, &red_view, EXTENT, 1.).unwrap();
    runtime.render_window(&mut green_window, &green_view, EXTENT, 1.).unwrap();
    let red = read_texture(&device, &queue, &red_texture, EXTENT);
    let green = read_texture(&device, &queue, &green_texture, EXTENT);
    assert_color_near(pixel(&red, EXTENT.width, 128, 64), [255, 0, 0, 255]);
    assert_color_near(pixel(&green, EXTENT.width, 128, 64), [0, 255, 0, 255]);

    runtime.update_window(&red_window, |_, cx| {
        red_root.update(cx, |view, cx| {
            view.0 = 0x0000ff;
            cx.notify();
        });
    }).unwrap();
    runtime.render_window(&mut red_window, &red_view, EXTENT, 1.).unwrap();
    let blue = read_texture(&device, &queue, &red_texture, EXTENT);
    assert_color_near(pixel(&blue, EXTENT.width, 128, 64), [0, 0, 255, 255]);
    let still_green = read_texture(&device, &queue, &green_texture, EXTENT);
    assert_color_near(pixel(&still_green, EXTENT.width, 128, 64), [0, 255, 0, 255]);
}
```

Add this local helper beside the existing texture helpers:

```rust
fn render_target(device: &wgpu::Device, label: &str) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: EXTENT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    (texture, view)
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```powershell
cargo test --features host --test wgpu_host_gpu one_runtime_renders_two_independent_windows -- --nocapture
```

Expected: compilation fails because `gpui_wgpu::WgpuRuntime` does not exist.

- [ ] **Step 3: Extract the common runtime types**

Create `src/wgpu_runtime.rs`. Move `ExternalGpu`, `HostFrame`, validation, `CurrentTarget`, `DirectRendererState`, and `DirectRenderer` from `wgpu_host.rs` without behavior changes. Add:

```rust
pub struct WgpuRuntime {
    context: gpui::HeadlessAppContext,
    renderer: Rc<RefCell<DirectRendererState>>,
}

pub struct WgpuWindow {
    handle: gpui::AnyWindowHandle,
    logical_size: gpui::Size<gpui::Pixels>,
    scale_factor: Option<f32>,
    cursor_style: gpui::CursorStyle,
    closed: bool,
}

impl WgpuWindow {
    fn ensure_open(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.closed, "GPUI window is closed");
        Ok(())
    }

    pub fn cursor_style(&self) -> gpui::CursorStyle {
        self.cursor_style
    }
}
```

Implement construction and window opening with these exact signatures:

```rust
impl WgpuRuntime {
    pub fn new(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
    ) -> anyhow::Result<Self>;

    pub fn open_window<V: gpui::Render + 'static>(
        &mut self,
        initial_size: gpui::Size<gpui::Pixels>,
        build_root: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> gpui::Entity<V>,
    ) -> anyhow::Result<(WgpuWindow, gpui::Entity<V>)>;
}
```

The body of `open_window` must validate size and use the public typed handle:

```rust
validate_logical_size(initial_size)?;
let handle = self.context.open_window(initial_size, build_root)?;
let root = handle.entity(&self.context)?;
let window = WgpuWindow {
    handle: handle.into(),
    logical_size: initial_size,
    scale_factor: None,
    cursor_style: gpui::CursorStyle::default(),
    closed: false,
};
Ok((window, root))
```

- [ ] **Step 4: Implement selected-window update and rendering**

Add:

```rust
pub fn update_window<R>(
    &mut self,
    window: &WgpuWindow,
    update: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R,
) -> anyhow::Result<R>;

pub fn render_window(
    &mut self,
    window: &mut WgpuWindow,
    target: &wgpu::TextureView,
    physical_size: wgpu::Extent3d,
    scale_factor: f32,
) -> anyhow::Result<()>;

pub fn tick(&mut self, elapsed: std::time::Duration);
```

`render_window` must call `window.ensure_open()`, build `HostFrame`, install the target, draw only `window.handle`, and call `finish()` before propagating either draw or renderer errors:

```rust
let frame = HostFrame::new(physical_size, scale_factor)?;
self.renderer.borrow_mut().begin(target, frame.device_size);
let draw_result = self.draw_frame(window, frame.logical_size, scale_factor);
let render_error = self.renderer.borrow_mut().finish();
draw_result?;
if let Some(error) = render_error {
    anyhow::bail!(error);
}
Ok(())
```

- [ ] **Step 5: Export the runtime and verify GREEN**

In `src/gpui_wgpu.rs` add the host-gated module and reexport:

```rust
#[cfg(all(not(target_family = "wasm"), feature = "host"))]
mod wgpu_runtime;

#[cfg(all(not(target_family = "wasm"), feature = "host"))]
pub use wgpu_runtime::*;
```

Run the command from Step 2. Expected: PASS and distinct red/green targets, followed by a blue update isolated to the first window.

- [ ] **Step 6: Commit**

```powershell
git add src/wgpu_runtime.rs src/gpui_wgpu.rs tests/wgpu_host_gpu.rs
git commit -m "feat: add multi-window WGPU runtime"
```

---

### Task 2: Route input and close windows independently

**Files:**
- Modify: `src/wgpu_runtime.rs`
- Modify: `tests/wgpu_host_gpu.rs`

**Interfaces:**
- Consumes: `WgpuRuntime`, `WgpuWindow`, and typed roots from Task 1.
- Produces: per-window `dispatch`, `dispatch_text`, cursor state, clipboard access, and `close_window` with closed-handle rejection.

- [ ] **Step 1: Write failing per-window input and close tests**

Add a two-window test using the existing `ProbeView` and `ProbeState`:

```rust
#[test]
fn input_and_close_are_scoped_to_one_runtime_window() {
    let _gpu = gpu_test_guard();
    let Some((instance, adapter, device, queue)) = gpu() else { return };
    let first_state = ProbeState::default();
    let second_state = ProbeState::default();
    let mut runtime = WgpuRuntime::new(
        ExternalGpu {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
        },
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
    ).unwrap();
    let first_root_state = first_state.clone();
    let second_root_state = second_state.clone();
    let (mut first, _) = runtime.open_window(gpui::size(px(256.), px(128.)), move |window, cx| {
        focused_probe(window, cx, first_root_state)
    }).unwrap();
    let (mut second, _) = runtime.open_window(gpui::size(px(256.), px(128.)), move |window, cx| {
        focused_probe(window, cx, second_root_state)
    }).unwrap();

    runtime.dispatch_text(&mut second, "janela dois").unwrap();
    assert!(first_state.text.borrow().is_empty());
    assert_eq!(&*second_state.text.borrow(), "janela dois");

    runtime.close_window(&mut second).unwrap();
    assert!(runtime.dispatch_text(&mut second, "falha").is_err());
    runtime.dispatch_text(&mut first, "ativa").unwrap();
    assert_eq!(&*first_state.text.borrow(), "ativa");

    runtime.close_window(&mut first).unwrap();
    let (mut third, _) = runtime.open_window(
        gpui::size(px(256.), px(128.)),
        |_, cx| cx.new(|_| SolidView(0x0000ff)),
    ).unwrap();
    assert!(runtime.dispatch_text(&mut third, "runtime vivo").is_ok());
}
```

Extract the focus setup already used in `fixture()` into:

```rust
fn focused_probe(window: &mut Window, cx: &mut App, state: ProbeState) -> gpui::Entity<ProbeView> {
    let view = cx.new(|cx| ProbeView {
        focus: cx.focus_handle(),
        state,
    });
    view.read(cx).focus.focus(window, cx);
    view
}
```

- [ ] **Step 2: Run and verify RED**

Run:

```powershell
cargo test --features host --test wgpu_host_gpu input_and_close_are_scoped_to_one_runtime_window -- --nocapture
```

Expected: compilation fails because `dispatch_text` and `close_window` are missing.

- [ ] **Step 3: Implement per-window input, clipboard, and close**

Add these methods to `WgpuRuntime`:

```rust
pub fn dispatch(
    &mut self,
    window: &mut WgpuWindow,
    input: gpui::PlatformInput,
) -> anyhow::Result<gpui::DispatchEventResult>;

pub fn dispatch_text(
    &mut self,
    window: &mut WgpuWindow,
    text: &str,
) -> anyhow::Result<()>;

pub fn close_window(&mut self, window: &mut WgpuWindow) -> anyhow::Result<()>;
pub fn set_clipboard_text(&mut self, text: String);
pub fn clipboard_text(&mut self) -> Option<String>;
pub fn is_cursor_visible(&self) -> bool;
```

Every window-scoped method starts with `window.ensure_open()?`. `dispatch`
updates only `window.cursor_style`. `dispatch_text` rejects empty committed
text. Close through the normal GPUI path and mark the handle closed only after
success:

```rust
self.context.update_window(window.handle, |_, gpui_window, _| {
    gpui_window.remove_window();
})?;
window.closed = true;
Ok(())
```

- [ ] **Step 4: Verify input/close and render-failure recovery**

Port the existing oversized-frame recovery assertion to call
`WgpuRuntime::render_window` for one window, then successfully render the other:

```rust
let oversized = wgpu::Extent3d {
    width: device.limits().max_texture_dimension_2d + 1,
    ..EXTENT
};
assert!(runtime.render_window(&mut first, &first_view, oversized, 1.).is_err());
runtime.render_window(&mut second, &second_view, EXTENT, 1.).unwrap();
```

Run:

```powershell
cargo test --features host --test wgpu_host_gpu input_and_close_are_scoped_to_one_runtime_window -- --nocapture
cargo test --features host --test wgpu_host_gpu runtime_recovers_on_another_window_after_render_error -- --nocapture
```

Expected: both PASS; closing or failing one window leaves the other functional.

- [ ] **Step 5: Commit**

```powershell
git add src/wgpu_runtime.rs tests/wgpu_host_gpu.rs
git commit -m "feat: isolate GPUI window lifecycle and input"
```

---

### Task 3: Preserve the WgpuHost facade and document the runtime

**Files:**
- Modify: `src/wgpu_host.rs`
- Modify: `tests/default_external_api.rs`
- Modify: `README.md`

**Interfaces:**
- Consumes: complete `WgpuRuntime` and `WgpuWindow` APIs from Tasks 1-2.
- Produces: existing `WgpuHost` behavior implemented as one runtime plus one window, and documented public multiwindow usage.

- [ ] **Step 1: Add a public API compilation test**

In `tests/default_external_api.rs`, under the same native/host cfg used for
`WgpuHost`, add:

```rust
#[cfg(all(not(target_family = "wasm"), feature = "host"))]
#[test]
fn exports_multi_window_runtime() {
    let _new = gpui_wgpu::WgpuRuntime::new;
    let _cursor = gpui_wgpu::WgpuWindow::cursor_style;
}
```

- [ ] **Step 2: Refactor WgpuHost into a delegating facade**

Replace its duplicated context/render fields with:

```rust
pub struct WgpuHost {
    runtime: WgpuRuntime,
    window: WgpuWindow,
}
```

The constructor creates `WgpuRuntime`, opens one window, and discards only the
returned typed root handle. Delegate all existing methods without changing
their signatures:

```rust
self.runtime.update_window(&self.window, update)
self.runtime.tick(elapsed)
self.runtime.dispatch(&mut self.window, input)
self.runtime.dispatch_text(&mut self.window, text)
self.runtime.render_window(&mut self.window, target, physical_size, scale_factor)
```

Clipboard and cursor access delegate to the corresponding runtime/window APIs.

- [ ] **Step 3: Run existing compatibility tests**

Run:

```powershell
cargo test --features host --test default_external_api
cargo test --features host --test wgpu_host_gpu
cargo test -p gpui-box-sdl --test wgpu_host_stack
```

Expected: all existing `WgpuHost`, external-image, input, clock, SDL stack, and
new multiwindow tests pass unchanged.

- [ ] **Step 4: Document ownership and multiwindow usage**

Add a `WgpuRuntime` section to `README.md` using this minimal example:

```rust,ignore
let mut runtime = WgpuRuntime::new(gpu, format, text_system, assets)?;
let (mut first, _) = runtime.open_window(first_size, build_first)?;
let (mut second, _) = runtime.open_window(second_size, build_second)?;

runtime.render_window(&mut first, &first_target, first_extent, first_scale)?;
runtime.render_window(&mut second, &second_target, second_extent, second_scale)?;
```

State explicitly that the caller owns native windows, surfaces, target
acquisition, and presentation; all targets use the runtime's format; rendering
is sequential; and `WgpuHost` remains the single-window convenience API.

- [ ] **Step 5: Run full verification**

Run:

```powershell
cargo fmt --all --check
cargo test --workspace --all-features -j 1
cargo clippy --workspace --all-targets --all-features -j 1 -- -D warnings
cargo doc --workspace --all-features --no-deps
rg -n "map_async|copy_texture_to_buffer|get_mapped_range" src/wgpu_runtime.rs src/wgpu_host.rs
git diff --check
```

Expected: every Cargo command and `git diff --check` exit zero; `rg` prints no
matches and therefore exits one. Linker informational messages on MSVC do not
count as crate warnings unless Clippy exits nonzero.

- [ ] **Step 6: Commit**

```powershell
git add src/wgpu_host.rs tests/default_external_api.rs README.md
git commit -m "refactor: build WgpuHost on multi-window runtime"
```
