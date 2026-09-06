# Multi-Window WGPU Runtime Implementation Plan

> **For Codex:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the test-only single-window host path with one production GPUI runtime that renders any number of embedded windows into caller-owned `wgpu::TextureView`s, while preserving the current `WgpuHost`, Kit, image, and SDL input APIs.

**Architecture:** A production `EmbeddedPlatform` owns lightweight `EmbeddedWindow` callback/state objects and is driven by a realtime dispatcher on the caller's main thread. `WgpuRuntime` owns one GPUI `ApplicationHandle`, a generational public window registry, and shared GPU/atlas state; each live window keeps independent scene/render-target state. `WgpuHost` becomes a single-window deterministic compatibility facade over the same direct rendering primitives, not over GPUI's test platform.

**Tech Stack:** Rust 2024, vendored GPUI, wgpu 28, SDL3, slotmap, smallvec, cargo nextest/test.

---

### Task 1: Freeze public compile contracts and feature boundaries

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Modify: `crates/gpui-box-sdl/Cargo.toml`
- Modify: `crates/gpui-box-sdl/src/lib.rs`
- Test: `tests/runtime_api.rs`

**Steps:**
1. Add compile-contract tests for `WgpuRuntime`, `WgpuRuntimeBuilder`, `WgpuWindow`, `WgpuWindowState`, `CloseOutcome`, and `TextPreedit`, including the explicit external-device constructor and `TextureView` frame target.
2. Add the public types as minimal compiling shells and re-export them without changing existing exports.
3. Make the SDL dependency on `gpui_wgpu` optional behind a default-on `wgpu-runtime` feature; keep the adapter available with `--no-default-features`.
4. Run `cargo test --test runtime_api` and `cargo check -p gpui-box-sdl --no-default-features`.

### Task 2: Make the realtime dispatcher a production GPUI primitive

**Files:**
- Modify: `vendor/gpui-box/crates/gpui/src/platform.rs`
- Modify: `vendor/gpui-box/crates/gpui/src/platform/threaded_dispatcher.rs`
- Modify: `vendor/gpui-box/crates/gpui/CHANGELOG.md`
- Test: `vendor/gpui-box/crates/gpui/src/platform/threaded_dispatcher.rs`

**Steps:**
1. Remove the `test-support` gate from `ThreadedDispatcher` while keeping test-only helpers gated.
2. Rename documentation from a test dispatcher to a host-driven realtime dispatcher and retain `run_ready_main_tasks` as the nonblocking pump.
3. Add/retain tests for main-thread affinity, background handoff, and due timers.
4. Run `cargo test -p gpui threaded_dispatcher`.

### Task 3: Implement `EmbeddedPlatform` and `EmbeddedWindow`

**Files:**
- Create: `src/embedded_platform.rs`
- Modify: `src/lib.rs`
- Test: `src/embedded_platform.rs`

**Steps:**
1. Implement a minimal display, keyboard layout/mapper, text system, clipboard, cursor, active-window, and window registry using production GPUI traits.
2. Implement every `PlatformWindow` callback required by normal `App::open_window`, with re-entrancy-safe callback extraction and restoration.
3. Store per-window bounds, scale, focus/hover, modifiers, pointer, IME bounds, frame request, close callbacks, sprite atlas, and a render hook.
4. Add tests that open two GPUI windows, resize/focus them independently, request frames, and close them without affecting the sibling.
5. Run `cargo test embedded_platform`.

### Task 4: Add the direct embedded frame path

**Files:**
- Modify: `src/wgpu_context.rs`
- Modify: `src/wgpu_renderer.rs`
- Modify: `src/embedded_platform.rs`
- Test: `tests/runtime_render.rs`

**Steps:**
1. Add a crate-private renderer constructor from shared external context and atlas.
2. Add `render_embedded(scene, target, size, format)` that encodes directly into the caller's view and never calls `render_to_image`.
3. Wire each embedded window's GPUI `draw` callback to its current frame target; a draw before target installation only leaves the window dirty.
4. Add two-texture tests proving scene output and resize isolation.
5. Run `cargo test --test runtime_render`.

### Task 5: Implement the generational `WgpuRuntime` registry

**Files:**
- Create: `src/wgpu_runtime.rs`
- Modify: `src/lib.rs`
- Test: `tests/runtime_lifecycle.rs`

**Steps:**
1. Construct one `Application::new_inaccessible(...).with_assets(...).with_quit_mode(QuitMode::Explicit).run_embedded(...)` and keep the concrete platform beside its handle.
2. Back public window IDs with `slotmap`; validate runtime identity and generation on every call.
3. Implement open, update, render, resize, state, close-request, force-close, and main-task pumping APIs.
4. Ensure stale and cross-runtime handles return typed errors and never alias newly opened windows.
5. Run `cargo test --test runtime_lifecycle`.

### Task 6: Route complete input and platform state per window

**Files:**
- Modify: `src/wgpu_runtime.rs`
- Modify: `src/embedded_platform.rs`
- Test: `tests/runtime_input.rs`

**Steps:**
1. Route `PlatformInput`, committed UTF-8 text, `TextPreedit`, focus, hover, resize, wake, and redraw to a selected window.
2. Forward committed/preedit text through the installed `PlatformInputHandler`; store and expose IME candidate bounds.
3. Keep clipboard and cursor state runtime-wide but associate cursor requests and redraw with the active window.
4. Test two-window focus/IME/input isolation plus clipboard/cursor round trips.
5. Run `cargo test --test runtime_input`.

### Task 7: Cut SDL over to the runtime and remove host coupling

**Files:**
- Modify: `crates/gpui-box-sdl/src/adapter.rs`
- Modify: `crates/gpui-box-sdl/src/platform.rs`
- Modify: `crates/gpui-box-sdl/src/lib.rs`
- Modify: `crates/gpui-box-sdl/README.md`
- Test: `crates/gpui-box-sdl/tests/input_adapter.rs`

**Steps:**
1. Add SDL window identity to routed host events and implement `adapt_into` so callers can avoid an intermediate `Vec`; keep `adapt` compatible.
2. Route resize, focus, text, preedit, file drop, quit/close, and all existing keyboard/mouse events to `WgpuWindow`.
3. Change `SdlPlatformBridge` runtime methods to synchronize UTF-8 clipboard and cursor through `WgpuRuntime`; retain deprecated host shims only where source compatibility requires them.
4. Add adapter-only compile coverage and multi-window routing tests.
5. Run `cargo test -p gpui-box-sdl` and `cargo check -p gpui-box-sdl --no-default-features`.

### Task 8: Preserve detachable entity identity

**Files:**
- Modify: `src/wgpu_runtime.rs`
- Test: `tests/runtime_detach.rs`

**Steps:**
1. Add APIs that open a window from an existing GPUI entity and return the entity on close/reattach.
2. Keep entity identity stable across detach, render, close, and reattach; reject double attachment.
3. Test repeated detach/reattach cycles and stale window handles.
4. Run `cargo test --test runtime_detach`.

### Task 9: Convert `WgpuHost` into a compatibility facade

**Files:**
- Modify: `src/wgpu_host.rs`
- Modify: `Cargo.toml`
- Test: `tests/host_render.rs`
- Test: `tests/host_interaction.rs`

**Steps:**
1. Reimplement the existing single-window API on the new embedded primitives with deterministic pumping where existing tests require it.
2. Delete `DirectRenderer: PlatformHeadlessRenderer`, the dummy `render_scene_to_image`, `HeadlessAppContext`, `TestPlatform`, `GpuiMode::test`, and `FakeHttpClient` from the production dependency path.
3. Remove `gpui/test-support` from the root `host` feature while leaving test support only in dev/test dependencies.
4. Run all existing host, Kit, image, and compile-contract tests unchanged.

### Task 10: Remove synchronous GPU waits from realtime rendering

**Files:**
- Modify: `src/wgpu_renderer.rs`
- Modify: `src/wgpu_runtime.rs`
- Test: `tests/runtime_render.rs`

**Steps:**
1. Split luminance probes into crate-private deterministic behavior for compatibility tests and realtime async readback for `WgpuRuntime`.
2. Poll completed readbacks opportunistically during pump/render and never call `device.poll(PollType::Wait)` on the runtime frame path.
3. Test that a pending probe does not block rendering and eventually publishes a value after pumping.
4. Run focused renderer and runtime tests.

### Task 11: Add bind-group caches and frame reuse

**Files:**
- Modify: `src/wgpu_renderer.rs`
- Test: `tests/runtime_render.rs`

**Steps:**
1. Cache `ExternalImageId -> BindGroup`, invalidating on unregister/replacement and pruning dead weak registrations.
2. Cache atlas bind groups by atlas texture identity.
3. Reuse frame vectors and staging buffers across frames without changing draw order or public APIs.
4. Add cache hit/invalidation counters under test cfg and verify behavior.
5. Run focused renderer tests and luminance parity tests.

### Task 12: Physically split the renderer without API changes

**Files:**
- Create: `src/wgpu_renderer/{mod.rs,shared.rs,window.rs,passes.rs,probes.rs,cache.rs}`
- Remove: `src/wgpu_renderer.rs`
- Modify: `src/lib.rs`

**Steps:**
1. Move GPU-global pipelines/layouts/atlas/caches into `RendererShared` and target-sized resources into `WindowRendererState`.
2. Move render passes, probes, and cache helpers into focused modules, keeping all existing public names and signatures.
3. Run `cargo fmt`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and the full native suite.

### Task 13: Finish builder, documentation, examples, and CI gates

**Files:**
- Modify: `src/wgpu_runtime.rs`
- Modify: `README.md`
- Modify: `crates/gpui-box-sdl/README.md`
- Modify: `.github/workflows/ci.yml`
- Create: `examples/sdl_multi_window.rs`

**Steps:**
1. Complete `WgpuRuntimeBuilder` for assets, execution mode, scale/appearance defaults, and shared external GPU objects.
2. Document the runtime lifecycle, SDL event routing, detachable windows, UTF-8-only clipboard, and compatibility facade.
3. Add a two-window SDL example with close-to-origin behavior.
4. Gate CI on native all-features, SDL adapter-only, clippy, docs, vendored-source drift, and the existing WASM check.
5. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features`, `cargo doc --workspace --all-features --no-deps`, and the vendored-source checker.

