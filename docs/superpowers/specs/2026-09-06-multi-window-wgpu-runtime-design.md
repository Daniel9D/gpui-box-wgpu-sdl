# Multi-window external-WGPU runtime

**Date:** 2026-09-06

## Goal

Extend `gpui-box-wgpu` with one GPUI application runtime that can host and
render multiple GPUI windows into caller-owned `wgpu::TextureView` targets.
All windows reuse the caller's existing `Instance`, `Adapter`, `Device`, and
`Queue`.

This repository remains platform-neutral. SDL windows, wgpu surfaces, docking,
detachable components, and egui composition belong to the consuming engine and
are explicitly outside this design.

## Current limitation

`WgpuHost` owns one `HeadlessAppContext`, one `AnyWindowHandle`, one logical
size, one scale factor, and one cursor state. Creating one host per native
window shares external GPU handles but duplicates GPUI application contexts,
atlases, text systems, assets, and application state. Views cannot move between
those independent contexts while preserving their entity identity.

## Public model

Add a global runtime and a caller-owned hosted-window handle:

```rust,ignore
let mut runtime = WgpuRuntime::new(
    external_gpu,
    target_format,
    text_system,
    asset_source,
)?;

let (mut window, root) = runtime.open_window(initial_size, |window, cx| {
    cx.new(|cx| RootView::new(cx))
})?;

runtime.tick(elapsed);
runtime.dispatch(&mut window, input)?;
runtime.render_window(&mut window, target, physical_size, scale_factor)?;
runtime.close_window(window)?;
```

`open_window` remains generic over the root view and returns both a non-generic
`WgpuWindow` and the typed `Entity<V>`. The entity is obtained through GPUI's
public `WindowHandle::entity` API; no capture slot or downcast hack is needed.

`WgpuWindow` stores only per-window GPUI state:

```text
WgpuWindow
├── AnyWindowHandle
├── logical size
├── scale factor
└── cursor style
```

The caller owns each `WgpuWindow` and decides how it maps to native windows and
render targets. A window handle is valid only with the runtime that created it;
using it after close returns an error.

## Runtime ownership

`WgpuRuntime` owns exactly one:

```text
WgpuRuntime
├── HeadlessAppContext
├── WgpuHeadlessRenderer
├── shared atlas
├── text system
└── asset source
```

The existing direct-render state continues to hold only the target for the
currently rendered window. Windows are rendered sequentially, so no persistent
target registry is required. `render_window` installs the supplied target,
draws the selected GPUI window, captures any renderer error, and clears the
temporary target before returning.

The runtime is initialized for one target format. Every target passed to it
must use that format. Supporting multiple render-pipeline formats is outside
the initial contract.

## Window operations

The initial API provides:

```rust,ignore
WgpuRuntime::open_window(...)
WgpuRuntime::close_window(window)
WgpuRuntime::update_window(&window, ...)
WgpuRuntime::dispatch(&mut window, input)
WgpuRuntime::dispatch_text(&mut window, text)
WgpuRuntime::render_window(&mut window, target, extent, scale)
WgpuRuntime::tick(elapsed)
WgpuRuntime::set_clipboard_text(...)
WgpuRuntime::clipboard_text()
WgpuWindow::cursor_style()
```

`close_window` calls GPUI's normal window removal path before invalidating the
hosted handle. Closing one window must not shut down the context while other
windows remain open.

Input, resize, scale, cursor, and focus are scoped to the selected window.
Clipboard and executor progress remain application-global.

## Rendering and synchronization

`render_window` receives a caller-owned view and never acquires or presents a
surface. It prepares the GPUI frame for that window's physical extent and
scale, then renders directly into the supplied view.

The renderer continues to use the external shared queue. Work submitted by the
caller before `render_window` is visible to GPUI in queue order; work submitted
afterward can overlay the GPUI result. No CPU wait, readback, image conversion,
or secondary GPU context is introduced.

Calls for multiple windows are sequential in the initial implementation. The
API does not promise parallel rendering from multiple threads.

## Compatibility

Keep `WgpuHost` as the supported single-window facade during migration. Its
public behavior and constructor remain compatible, but its implementation may
delegate to `WgpuRuntime` plus one `WgpuWindow`. Existing downstream code and
tests continue to compile.

`WgpuImage`, external texture handling, GPUI/GPUI Kit reexports, and the SDL
input adapter are unchanged.

## Errors and invariants

- Reject zero logical or physical sizes.
- Reject non-finite or non-positive scale factors.
- Reject use of a window after it has been closed.
- Always clear the temporary current target, including failed draws.
- A failed render for one window must not poison subsequent windows.
- Failure to open a new window leaves existing windows usable.
- Closing the last window does not implicitly destroy the runtime; dropping the
  runtime performs final GPUI shutdown.

## Testing

Development follows TDD with real external WGPU resources where relevant:

1. Open two roots in one runtime and render them alternately into distinct
   targets.
2. Verify both windows use the exact caller-provided device and queue.
3. Update one root without changing the other.
4. Route input, text, resize, scale, and cursor state to the selected window.
5. Close one window and prove the other continues to update and render.
6. Verify a failed render clears transient target state and the next window can
   still render.
7. Preserve all existing `WgpuHost`, `WgpuImage`, GPUI Kit, SDL, doctest, and
   Clippy checks.

## Acceptance criteria

- One `WgpuRuntime` renders at least two GPUI windows with different roots.
- Both windows share one GPUI context, atlas, external device, and external
  queue.
- The caller supplies every target and retains surface lifecycle ownership.
- A typed root entity is returned without a capture-slot workaround.
- Closing one window leaves all other windows functional.
- `WgpuHost` remains source-compatible for current consumers.
- Runtime rendering performs no CPU readback or image upload fallback.

## Non-goals

- Native-window or event-loop ownership.
- Surface creation, acquisition, configuration, or presentation.
- Docking or detachable-view policy.
- egui integration.
- Cross-runtime entity transfer.
- Concurrent multi-threaded rendering.
- Multiple target formats in one runtime.
