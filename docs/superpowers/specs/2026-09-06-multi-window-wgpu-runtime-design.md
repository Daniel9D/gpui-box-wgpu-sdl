# Multi-window external-WGPU runtime and SDL3 platform

**Date:** 2026-09-06

**Status:** Approved design

**Scope:** `gpui-box-wgpu`, its vendored GPUI Box where strictly required, and
the first-class `gpui-box-sdl` integration.

## Goal

Provide one production-capable embedded GPUI application runtime that can
open, update, render, detach, reattach, and close multiple GPUI windows while:

- rendering into caller-owned `wgpu::TextureView` targets;
- reusing the caller's `Instance`, `Adapter`, `Device`, and `Queue`;
- preserving entity identity and application state across windows;
- treating SDL3 as the primary supported platform integration;
- preserving every existing `WgpuHost`, `WgpuImage`, GPUI Box Kit, and SDL API;
- removing `TestPlatform`, `GpuiMode::test`, `FakeHttpClient`, and fake image
  rendering from the production host path;
- keeping realtime rendering free from synchronous GPU waits and CPU
  readback.

The caller continues to own native windows, WGPU surfaces, surface texture
acquisition, presentation, and docking policy.

## Why the previous design must change

The earlier design correctly introduced one shared runtime and caller-owned
window handles, but it retained `HeadlessAppContext` and a single mutable
`WgpuHeadlessRenderer`. The implementation plan then proposed extracting the
existing bridge without first replacing its test-platform lifecycle.

That approach has five correctness problems:

1. The `host` feature would continue enabling `gpui/test-support` and would
   keep `TestPlatform`, `GpuiMode::test`, and `FakeHttpClient` in production.
2. Rendering would still enter through `render_to_image()` and return a dummy
   zero-sized image after drawing to the external target.
3. A single renderer carries size-dependent intermediate textures and
   luminance values, so differently sized windows would repeatedly resize the
   same state and could observe one another's probes.
4. Calling `Window::draw` and `dispatch_event` directly bypasses parts of the
   normal `PlatformWindow` callback lifecycle used for frame callbacks, focus,
   text input, resize, and close.
5. A `closed: bool` does not prove that a `WgpuWindow` belongs to the runtime
   receiving it, and the default GPUI quit policy may terminate the
   application after the last window closes.

This design replaces those foundations while retaining the useful decisions
from the previous version: one runtime, one target format, sequential
rendering, temporary caller-owned targets, and a source-compatible
single-window facade.

## Decisions

1. SDL3 is a first-class supported integration, not a temporary adapter.
2. No new `EmbeddedAppContext` is introduced. The runtime reuses
   `Application::with_platform` and `Application::run_embedded`.
3. `EmbeddedPlatform` and `EmbeddedWindow` provide the production platform
   implementation required by the external run loop.
4. `WgpuRuntime::new` uses realtime execution. The builder may select
   deterministic execution.
5. `WgpuHost::new` remains deterministic so `tick(elapsed)` retains its current
   behavior.
6. One `WgpuRuntime` supports exactly one target texture format.
7. Windows render sequentially on the runtime's owner thread.
8. GPU-global objects are shared, but size-, probe-, and presentation-related
   renderer state is isolated per window.
9. Entities may move between windows in the same runtime without changing
   `EntityId` or reconstructing state.
10. Clipboard integration supports UTF-8 text only in this version.
11. Existing public APIs remain supported; new multi-window APIs are additive.

## Non-goals

- Owning or creating SDL windows.
- Owning WGPU surfaces or acquiring/presenting surface textures.
- Implementing a docking user interface or deciding detachable-layout policy.
- Supporting multiple target formats within one runtime.
- Rendering two GPUI windows concurrently from different threads.
- Moving entities between different runtimes.
- Supporting non-text clipboard MIME types through SDL.
- Adding touch, pen, gamepad, or accessibility integrations as part of this
  change.
- Optimizing allocations without a measured reason.

## Architecture

```text
SDL application / engine
├── SDL windows and event loop
├── WGPU surfaces and presentation
├── SDL_WindowID -> WgpuWindow association
└── WgpuRuntime
    ├── ApplicationHandle
    ├── Rc<EmbeddedPlatform>
    ├── RuntimeId
    ├── RendererShared
    └── active window registry
        ├── WgpuWindow A
        │   └── Rc<EmbeddedWindow>
        │       └── RendererWindowState A
        └── WgpuWindow B
            └── Rc<EmbeddedWindow>
                └── RendererWindowState B
```

### Application ownership

Runtime construction uses the existing embedded application entry point:

```text
EmbeddedPlatform
-> Application::with_platform
-> Application::with_assets
-> Application::with_quit_mode(QuitMode::Explicit)
-> Application::run_embedded
-> ApplicationHandle
```

`Application::with_platform` already provides GPUI's null HTTP client. A real
HTTP client may be supplied through the runtime builder later without making
networking mandatory. Production code never constructs `FakeHttpClient`.

`QuitMode::Explicit` is required: closing the last hosted window does not
destroy or quit the runtime, and a new window may be opened afterward.

The runtime and all its windows are bound to the thread that creates the
runtime. This matches GPUI's `Rc<AppCell>` ownership and SDL's main-thread
window and text-input APIs.

### EmbeddedPlatform

`EmbeddedPlatform` implements `gpui::Platform` and owns application-global
platform state:

- foreground and background executors;
- realtime or deterministic dispatcher;
- the active GPUI window;
- the global GPUI clipboard item and its revision;
- global cursor visibility;
- registered platform/application callbacks;
- the registry of live `EmbeddedWindow` instances.

Unsupported native features return their existing neutral result or an
explicit unsupported error. They must not panic on ordinary embedded use.

The platform does not know about SDL handles. SDL synchronization remains in
`gpui-box-sdl`, keeping the WGPU runtime usable with another external event
loop without weakening SDL support.

### EmbeddedWindow

Each `EmbeddedWindow` implements `gpui::PlatformWindow` and stores state that
belongs to one hosted window:

- logical bounds and scale factor;
- active and hovered state;
- modifiers and caps-lock state when required by GPUI;
- current `PlatformInputHandler`;
- requested cursor style;
- desired text-input active state;
- logical IME/candidate bounds;
- registered input, focus, hover, resize, frame, close, and appearance
  callbacks;
- shared sprite atlas handle;
- temporary presentation target state for the active render call;
- the window's renderer state.

The runtime reaches the concrete `EmbeddedWindow` through its registry even
though GPUI owns it as `Box<dyn PlatformWindow>`.

### WgpuRuntime and WgpuWindow

`WgpuRuntime` owns the application and all shared GPU resources.
`WgpuWindow` is a non-clonable caller-owned capability for one GPUI window.

Conceptually, `WgpuWindow` contains:

```text
WgpuWindow
├── RuntimeId
├── opaque WgpuWindowId
├── AnyWindowHandle
├── Rc<EmbeddedWindow>
├── logical size and scale
└── open/closed state
```

`RendererWindowState` is owned exactly once, inside `EmbeddedWindow`, because
that is the object receiving `PlatformWindow::draw`. `WgpuWindow` reaches it
through the concrete platform-window handle and never duplicates it.

Every window-scoped operation validates both `RuntimeId` and open state before
touching GPUI or GPU state. A window from another runtime is rejected even if
its internal slot identifier happens to match.

### Renderer ownership

The current monolithic renderer is divided logically before it is divided
into files.

`RendererShared` contains:

- the caller-provided `Instance`, `Adapter`, `Arc<Device>`, and `Arc<Queue>`;
- target format and immutable device capabilities;
- pipelines, bind-group layouts, samplers, and shared uniform infrastructure;
- sprite atlas;
- external-image bind-group cache;
- atlas bind-group cache;
- device generation and shared error state.

`RendererWindowState` contains:

- current physical size;
- path and MSAA intermediate textures;
- backdrop sharp/blur textures and parameter buffers;
- in-flight luminance-probe readback ring;
- last completed luminance value for each slot;
- per-window frame and submission bookkeeping.

Frame buffers that are safe to reuse across sequential renders may stay in
`RendererShared`. Data whose lifetime crosses a submission or whose meaning
depends on window size stays in `RendererWindowState`.

## Public API

The signatures below describe the intended contract. Exact argument names may
change during implementation, but the ownership and behavior may not.

```rust,ignore
pub enum RuntimeMode {
    Realtime,
    Deterministic,
}

pub struct WgpuRuntime;
pub struct WgpuRuntimeBuilder;
pub struct WgpuWindow;
pub struct WgpuWindowId(/* private */);

pub struct TextPreedit {
    pub text: String,
    pub selection_utf16: Option<std::ops::Range<usize>>,
}

impl WgpuRuntime {
    pub fn new(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
    ) -> anyhow::Result<Self>;

    pub fn builder(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
    ) -> WgpuRuntimeBuilder;

    pub fn open_window<V: gpui::Render + 'static>(
        &mut self,
        initial_size: gpui::Size<gpui::Pixels>,
        build_root: impl FnOnce(
            &mut gpui::Window,
            &mut gpui::App,
        ) -> gpui::Entity<V>,
    ) -> anyhow::Result<(WgpuWindow, gpui::Entity<V>)>;

    pub fn update_window<R>(
        &mut self,
        window: &WgpuWindow,
        update: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R,
    ) -> anyhow::Result<R>;

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

    pub fn dispatch_text_editing(
        &mut self,
        window: &mut WgpuWindow,
        editing: TextPreedit,
    ) -> anyhow::Result<()>;

    pub fn set_window_focus(
        &mut self,
        window: &mut WgpuWindow,
        focused: bool,
    ) -> anyhow::Result<()>;

    pub fn render_window(
        &mut self,
        window: &mut WgpuWindow,
        target: &wgpu::TextureView,
        physical_size: wgpu::Extent3d,
        scale_factor: f32,
    ) -> anyhow::Result<()>;

    pub fn close_window(&mut self, window: &mut WgpuWindow)
        -> anyhow::Result<()>;

    pub fn pump(&mut self);
    pub fn advance(&mut self, elapsed: Duration) -> anyhow::Result<()>;
}
```

The concrete preedit type is renderer- and SDL-independent and contains owned
UTF-8 text plus an optional UTF-16 selection range. `gpui-box-sdl` performs the
SDL-specific range conversion before calling the runtime.

`WgpuWindow` exposes read-only state required by the embedder:

- opaque ID for routing;
- whether it is closed;
- requested cursor style and visibility;
- whether SDL text input should be active;
- current logical IME input area when available.

It does not expose renderer internals or a mutable GPUI window reference.

### Builder

`WgpuRuntime::new` is the realtime convenience constructor. The builder adds
only options with an established use:

- `RuntimeMode`;
- optional HTTP client.

No builder replaces `WgpuHost::new`, and no speculative configuration is
added.

### WgpuHost compatibility

The public `WgpuHost` methods retain their signatures and behavior:

- `new`;
- `update`;
- `tick`;
- `dispatch`;
- `dispatch_text`;
- `set_clipboard_text`;
- `clipboard_text`;
- `cursor_style`;
- `is_cursor_visible`;
- `render_to_view`.

Internally it contains one deterministic `WgpuRuntime` and one `WgpuWindow`.
`tick(elapsed)` pumps pending work, advances the deterministic clock by the
supplied duration, and pumps again, matching the current contract.

Real screenshot capture and `WgpuHeadlessRenderer` remain available to tests.
Only production's dummy `RgbaImage` return path is removed.

## Execution modes

### Realtime

Realtime is the default for `WgpuRuntime`:

- timers use monotonic wall time;
- background work uses production concurrency;
- `pump` drains main-thread work available at that moment;
- GPU probes are collected without blocking;
- normal render and resize never call `device.poll(PollType::Wait)`;
- `advance` returns an error because realtime has no virtual clock.

The background worker facility may be shared rather than creating one full
pool per runtime. Foreground queues, timer ownership, and wakeups remain scoped
to their runtime so pumping one application cannot execute another runtime's
main-thread work. Runtime construction verifies that calls occur on its owner
thread.

### Deterministic

Deterministic mode preserves virtual-time behavior for `WgpuHost`, tests, and
repeatable capture:

- time changes only through `advance`;
- scheduled work is ordered deterministically;
- probe collection may wait for a known submission;
- no `TestPlatform` or `GpuiMode::test` is involved.

The deterministic dispatcher is a production-capable generalization of the
current test dispatcher, with test-only inspection APIs remaining feature
gated.

## Window lifecycle

### Open

1. Validate logical size before allocating a GPUI slot.
2. Ask GPUI to open the window using the registered `EmbeddedPlatform`.
3. Build and retain the typed root entity.
4. Resolve the concrete `EmbeddedWindow` created for that GPUI handle.
5. Create its empty `RendererWindowState`.
6. Register the opaque window ID only after all prior steps succeed.
7. Return `(WgpuWindow, Entity<V>)`.

Failure rolls back every partially created registry entry and leaves existing
windows usable.

### Focus and hover

At most one window is active in `EmbeddedPlatform`.

On focus gain, the runtime deactivates the previous active window, updates the
platform's active handle, and invokes the registered active-status callbacks
in that order. Focus loss only clears the active handle if it still points to
that window.

Pointer motion marks the routed window hovered. Mouse leave clears hover.
Keyboard/text input remains explicitly routed by the caller; runtime methods
do not guess a destination.

### Close

`close_window` invokes GPUI's normal removal path. The caller-owned handle is
marked closed only after GPUI removes the window successfully. Renderer state
and pending probes for that window are then dropped.

Closing one window cannot close another. Closing the last window cannot quit
the runtime. Calling any window operation afterward, including a second
close, returns an error.

### Runtime drop

Dropping `WgpuRuntime` closes remaining GPUI windows and performs application
shutdown exactly once. Caller-owned SDL windows, WGPU surfaces, textures,
device, and queue remain governed by their own handles and lifetimes.

## Frame and render flow

```text
caller acquires surface texture/view
-> render_window(window, view, extent, scale)
-> validate runtime/window/geometry/format/thread/reentrancy
-> install temporary target using a scope guard
-> update EmbeddedWindow size and scale callbacks
-> request a forced frame with presentation required
-> GPUI frame callback runs next-frame callbacks and draws
-> EmbeddedWindow::draw(scene)
-> RendererShared + that window's RendererWindowState encode the frame
-> commands are submitted to the caller's Queue
-> renderer error is extracted
-> temporary target is always cleared
-> caller may add later queue work and present
```

Every `render_window` invocation uses `RequestFrameOptions` with
`require_presentation` and `force_render` set, so it draws into the supplied
view even when GPUI content has not changed; the caller may have acquired a
different surface texture. Frame lifecycle callbacks still run through the
registered `PlatformWindow` path.

The runtime never acquires, configures, or presents a surface. Queue ordering
guarantees that caller writes submitted before `render_window` are visible to
external images, and caller work submitted after it may overlay the result.

Only one temporary target may be installed at a time. Reentrant or concurrent
render attempts return an error before changing target state. The guard clears
the target on success, ordinary error, or unwind.

The target's base texture format is checked against the runtime format.
View-format overrides and subresource constraints that are not exposed by the
safe `TextureView` API are left to WGPU validation and surfaced through the
render error channel.

## Input and SDL3 routing

SDL remains the supported primary integration. `gpui-box-sdl` keeps its
existing APIs and adds multi-window routing.

```text
SDL_Event.windowID
-> SdlWindowRouter
-> per-window SdlInputAdapter
-> SdlHostEvent tagged with SDL_WindowID
-> caller's SDL_WindowID -> WgpuWindow association
-> WgpuRuntime operation on that WgpuWindow
```

Each SDL window has its own adapter so these states never leak across windows:

- modifiers and caps lock;
- pointer position and pressed button;
- viewport transform;
- drag-and-drop paths/session;
- focus-reset behavior.

The current `SdlInputAdapter::adapt -> Vec<SdlHostEvent>` contract remains.
An additive `adapt_into` API accepts caller-owned reusable storage. Replacing
the public return type with `SmallVec` is forbidden because that would break
source compatibility. A separate crate is not introduced unless another real
backend needs the renderer-independent adapter.

### Platform callback delivery

- Normal `PlatformInput` invokes the window's registered `on_input` callback.
- Focus invokes active-status callbacks and updates `active_window`.
- Resize updates platform state before invoking `on_resize`.
- Close requests follow the registered close/should-close callbacks.
- Text input operates through that window's `PlatformInputHandler`.

This replaces direct host calls that bypass the platform lifecycle while
preserving their observable input result.

## Text input and IME

SDL text-input state is window-specific and is synchronized only on the SDL
main thread.

Each `EmbeddedWindow` retains the latest GPUI `PlatformInputHandler`. The flow
is:

- committed UTF-8 text calls `replace_text_in_range(None, text)`;
- preedit text calls `replace_and_mark_text_in_range`;
- composition cancellation calls `unmark_text`;
- GPUI input focus changes update the desired SDL text-input active state;
- `PlatformWindow::update_ime_position` records logical candidate bounds;
- `SdlPlatformBridge` applies `SDL_StartTextInput`, `SDL_StopTextInput`,
  `SDL_ClearComposition`, and `SDL_SetTextInputArea` to the associated SDL
  window.

`SDL_TextEditingEvent.start` and `length` count UTF-8 characters and may be
`-1` when unspecified. GPUI selection ranges count UTF-16 code units.
`gpui-box-sdl` converts a non-negative character range by walking Rust `char`
boundaries and summing `char::len_utf16`; it rejects or safely omits malformed
negative/out-of-bounds selections rather than slicing UTF-8 by byte offset.
Committed text remains owned UTF-8.

IME bounds are converted from GPUI logical coordinates to SDL window
coordinates with the inverse viewport transform:

```text
sdl_position = viewport_origin + gpui_position * viewport_scale
```

The result is rounded and range-checked before building `SDL_Rect`.

Closing or defocusing a window clears only its composition. It cannot unmark
text or stop text input for another window.

## Clipboard and cursor

Clipboard is application-global. `EmbeddedPlatform` stores the complete GPUI
`ClipboardItem` plus a monotonically increasing revision. SDL synchronization
imports and exports UTF-8 text only:

```text
SDL clipboard
-> SdlPlatformBridge
-> EmbeddedPlatform ClipboardItem
-> GPUI

GPUI write + revision change
-> SdlPlatformBridge
-> SDL clipboard text
```

Unchanged text is not written back. A GPUI non-text item may remain stored
inside the runtime but is not exported through SDL in this version.

Cursor style is desired state per window. SDL's installed cursor and
visibility are process/global platform state, so the bridge applies the style
for the currently hovered SDL window. Mouse motion restores visibility under
the existing hide-until-motion behavior. Existing GPUI-to-SDL cursor mappings
and cursor handle reuse remain unchanged.

`SdlPlatformBridge` remains public and source-compatible with `WgpuHost`. New
overloads or adapter targets support `WgpuRuntime` plus `WgpuWindow`; the old
bridge is not deprecated by this design.

## Detach and reattach without state loss

Multi-window support guarantees the primitive needed by detachable UI: one
entity may stop rendering in one GPUI window and begin rendering in another
window of the same runtime without recreation.

The docking/application layer owns an origin slot and a strong `Entity<V>` or
type-erased equivalent:

```text
origin retains entity
-> remove entity from origin's rendered tree
-> open detached WgpuWindow with a wrapper root
-> wrapper renders the same entity
-> entity state may continue changing
-> close request starts reattachment
-> remove entity from detached tree
-> insert it back into origin slot
-> restore logical focus if appropriate
-> close detached WgpuWindow
```

The following survive the transfer:

- `EntityId`;
- entity state;
- subscriptions and application-global references;
- runtime, assets, text system, atlas, device, and queue.

The following are intentionally recalculated:

- layout and hitboxes;
- native focus/hover;
- active IME composition and candidate bounds;
- requested cursor;
- window-sized renderer resources.

The entity must be retained before it is removed from the old tree. Closing a
window before retaining or reattaching its only strong entity handle is caller
error. `WgpuRuntime` supplies safe window/update primitives but does not store
docking origins or implement docking UI; that policy belongs to GPUI Box Kit
or the consuming application.

Cross-runtime transfer is rejected.

## Luminance probes and synchronization

Probe state is per window. Each `RendererWindowState` owns a small ring of
readback buffers and tracks the submission associated with each entry.

Realtime behavior:

1. A frame encodes probe copies and schedules mapping.
2. A later frame polls without waiting.
3. Completed data replaces that window's last known values.
4. Incomplete data leaves the previous value unchanged.
5. `backdrop_luminance(slot)` returns the last completed value or `None`.

Deterministic behavior may wait for the known probe submission before
returning the next deterministic result. Synchronous waiting is confined to
this mode and explicit screenshot/readback APIs.

Resize and close discard only that window's pending ring. Alternating windows
cannot exchange probe values.

Normal resize does not wait for idle GPU work and does not explicitly destroy
resources still referenced by submissions. Replacing the owned handles lets
WGPU keep the old allocation alive for the necessary duration.

## Bind-group caches

Caching follows measured need and is implemented only after the uncached
multi-window path is correct.

### External images

The shared cache key is `ExternalImageId`. Each value contains:

- `Weak<ExternalImageHandle>`;
- `wgpu::BindGroup`;
- the device generation for which it was created.

The weak handle prevents the cache from extending the engine texture's
lifetime. Dead entries are pruned, and device recovery clears the complete
cache. IDs are process-unique and are never treated as reusable texture
versions.

The first insertion verifies that the payload is a compatible
`WgpuImagePayload` for the current device. A failed validation does not insert
an entry or poison later images.

### Atlas

Atlas texture bind groups are keyed by `AtlasTextureId` plus texture/view
generation. Entries are invalidated when the view changes, the atlas is
rebuilt, or the device generation changes. Cache lifetime is bounded by live
atlas textures.

### Frame allocations

Renderer vectors and buffers are reused only where ownership across GPU
submissions is already understood. SDL output optimization uses caller-owned
reusable `Vec` storage through `adapt_into`; no dependency or public return
type changes solely to remove a tiny allocation.

## Error handling and invariants

The public API continues to use `anyhow::Result`. No public error enum is
introduced until consumers demonstrate a need to match individual failures.

Every window operation checks:

- owner thread;
- matching `RuntimeId`;
- open state;
- absence of forbidden reentrancy.

Rendering additionally checks:

- non-zero width and height;
- one texture layer;
- positive finite scale;
- logical-size representability;
- runtime target format;
- required render-attachment capability where inspectable.

`PlatformWindow::draw` cannot return `Result`, so presentation state records
the first renderer error for the active render call. `render_window` extracts
that error after GPUI returns. A scoped target guard always clears the current
target before any error is propagated.

Required recovery behavior:

- failed open leaves all existing windows usable;
- failed render leaves the same and other windows usable;
- failed external image does not cache a bad bind group;
- resize failure retains a valid prior state or returns before mutation;
- closing one window drops only its resources;
- runtime drop shuts GPUI down exactly once.

## Compatibility matrix

| Existing capability | Required result after migration |
| --- | --- |
| `WgpuHost::new` and all current methods | Same signatures and behavior |
| Deterministic `WgpuHost::tick(elapsed)` | Preserved |
| Caller-owned external device and queue | Same handles used by every window |
| Direct render to `TextureView` | Preserved, without dummy image |
| `WgpuImage` in `gpui::img` | Preserved without CPU copy/readback |
| External queue-write ordering | Preserved |
| GPUI Box Kit feature/reexport | Preserved |
| Existing renderer/headless APIs | Preserved |
| Real screenshot capture in tests | Preserved under test support |
| SDL mouse and complete keyboard mapping | Preserved |
| SDL committed UTF-8 text and preedit event | Preserved and completed through IME |
| SDL multi-file drag-and-drop | Preserved per window |
| UTF-8 clipboard | Preserved |
| GPUI-to-SDL cursor mapping | Preserved per hovered window |
| Resize, focus, and quit host events | Preserved and routed by window |
| Default build and WASM renderer | Must continue compiling |

## Test strategy

Existing tests remain active and unchanged unless an additive assertion is
needed. A refactor is not allowed to weaken a test to obtain a pass.

### Public API and feature tests

- Compile the existing `WgpuHost` API.
- Compile `WgpuImage -> gpui::ImageSource`.
- Compile default features without `host`.
- Compile `host` without `gpui/test-support`.
- Compile `kit` and its GPUI Box Kit reexport.
- Compile existing `gpui-box-sdl` APIs.
- Compile `WgpuRuntime`, `WgpuWindow`, and builder APIs.
- Check WASM without native host APIs.

### Runtime tests without a GPU

- Reject invalid logical and physical geometry.
- Reject invalid scale.
- Reject closed and wrong-runtime handles.
- Reject second close and render reentrancy.
- Close the last window and open another.
- Roll back a failed open.
- Shut down exactly once.
- Verify realtime timer progress.
- Verify deterministic ordering and virtual time.
- Verify legacy `WgpuHost::tick` behavior.

### Serialized GPU integration tests

- Render two distinct roots to two distinct targets.
- Update one root without changing the other.
- Verify both windows use the caller's exact device and queue.
- Alternate different window sizes and scales.
- Render the same `WgpuImage` in two windows without copy.
- Recover in another window after a render failure.
- Isolate bright/dark luminance probes between windows.
- Verify realtime probe collection does not block.
- Verify deterministic probes are reproducible.
- Verify external bind-group reuse and weak-lifetime eviction.
- Verify atlas cache invalidation.
- Verify closing one window releases only its renderer state.

### SDL and platform tests

- Preserve every existing keyboard, mouse, text, file-drop, clipboard, cursor,
  resize, focus, and end-to-end host test.
- Route two SDL window IDs through independent adapters.
- Keep modifiers, pointer state, viewport, and drops isolated.
- Apply focus and hover to only the routed window.
- Start and stop text input per SDL window.
- Transform IME bounds through the inverse viewport.
- Convert preedit selections containing BMP text, emoji, and combining code
  points from UTF-8 character offsets to UTF-16 units.
- Close one composing window without changing another.
- Apply the hovered window's cursor while retaining global visibility rules.

### Detach/reattach test

The test records an entity's `EntityId`, renders it in an origin, changes its
state, presents it in a detached window, changes its state again, reattaches it
before closing, and renders the origin. It verifies identical `EntityId`,
accumulated state, live subscriptions, restorable focus, and continued origin
rendering.

### CI

CI is established before the architectural refactor:

- Windows: format, check, tests, Clippy, docs, and WARP GPU tests when
  available.
- Linux: format, check, tests, Clippy, docs, and llvmpipe GPU tests when
  available.
- macOS: compile and non-headless-adapter-dependent tests.
- WASM: check the renderer without `host`.
- Feature matrix: default, `host`, `kit`, and all features.
- GPU-creating tests use the repository's serialization guard.
- Workspace tests may use `-j 1` on constrained Windows runners.

A GPU test may skip only when no compatible adapter is found. Once an adapter
and device exist, any later error is a test failure.

## Revised implementation order

1. Freeze public compatibility in compile tests and establish the
   multiplatform CI matrix.
2. Split renderer ownership logically into shared and per-window state.
3. Add execution modes, per-window asynchronous probes, and remove waits from
   realtime resize/render.
4. Implement `EmbeddedPlatform` and `EmbeddedWindow` on the existing
   `Application::run_embedded` path.
5. Implement direct presentation through `PlatformWindow` callbacks in one
   single-window vertical slice.
6. Add `WgpuRuntime`/`WgpuWindow`, runtime identity, multi-window lifecycle,
   and independent close/recovery.
7. Complete focus, hover, IME, clipboard, and cursor platform behavior.
8. Add SDL window routing while preserving all current SDL APIs.
9. Implement and test identity-preserving detach/reattach primitives.
10. Rebuild `WgpuHost` as one deterministic runtime plus one window; then
    remove production's test-support dependency and dummy image path.
11. Add bounded external-image and atlas bind-group caches after profiling.
12. Add measured buffer/vector reuse and `SdlInputAdapter::adapt_into`.
13. Split `wgpu_renderer.rs` physically along the proven shared/window/cache
    boundaries without changing public renderer APIs.
14. Finish builder options, documentation, examples, and full verification.

Each phase must leave existing tests green. No phase removes the old path
before its compatibility facade runs on the new path.

## Acceptance criteria

- One runtime renders at least two GPUI windows with independent roots,
  dimensions, scales, input state, cursor state, IME state, and probes.
- All windows share the caller's external GPU objects, application state, text
  system, assets, and atlas.
- The caller owns every target and all surface lifecycle operations.
- Realtime rendering, resize, and probe polling perform no synchronous wait or
  CPU readback.
- Deterministic mode retains reproducible timers and probe results.
- A failed or closed window cannot poison another window.
- Closing the last window leaves the runtime reusable.
- Wrong-runtime handles are rejected.
- A detached entity returns to its origin with the same `EntityId`, state, and
  subscriptions.
- `WgpuHost` remains source- and behavior-compatible.
- `SdlPlatformBridge` and all existing SDL input capabilities remain
  supported.
- The `host` feature no longer enables `gpui/test-support`.
- Production code contains no `TestPlatform`, `GpuiMode::test`,
  `FakeHttpClient`, or dummy `render_to_image` path.
- Real screenshot and headless test APIs continue to work under test support.
- Default, host, kit, SDL, WASM, docs, Clippy, and multiplatform checks pass.

## Comparison with the previously proposed order

| Previous phase | Retained | Changed |
| --- | --- | --- |
| Embedded platform/context | Production platform is required | Reuse `ApplicationHandle`; do not create another context. Execution mode is designed at the same time. |
| Direct `render_embedded` | Dummy image path is removed | Direct rendering is delivered atomically with the platform and uses registered window callbacks. |
| Decouple SDL from `WgpuHost` | Adapter internals stay renderer-independent | SDL remains first-class; current bridge stays supported and gains runtime/window routing. |
| Focus, IME, clipboard, cursor | All are completed | They move before compatibility cutover and are explicitly scoped global vs per-window. |
| Bind-group caches | Cache both external images and atlas | Add weak lifetime, texture generation, device invalidation, and measurement gate. |
| Realtime/deterministic | Both modes remain | Move before public multi-window release; split renderer state so probes cannot cross windows. |
| Reuse allocations/SmallVec | Allocation work remains possible | Preserve `Vec` API and add `adapt_into`; optimize only after measurement. |
| Split renderer file | Public API remains unchanged | Split logical ownership early, physical files only after boundaries stabilize. |
| Builder and CI | Both remain | CI becomes phase zero; builder is introduced when execution mode needs configuration, without replacing existing constructors. |

The result preserves the original plan's external ownership model while
removing its reliance on test infrastructure and adding the lifecycle,
resource isolation, SDL routing, and detach guarantees required for a real
multi-window editor.
