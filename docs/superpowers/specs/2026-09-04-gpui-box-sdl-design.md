# GPUI Box SDL3 adapter design

## Goal

Add a reusable `gpui-box-sdl` crate that translates raw SDL3 events into
GPUI Box input without coupling SDL to the renderer. Prove the intended stack
with real SDL event pumping and an end-to-end SDL → adapter → `WgpuHost` → GPUI
listener test. `rust-engine` remains unchanged during this work.

## Context

- The standalone renderer lives in this repository as package
  `gpui-box-wgpu` and exports `WgpuHost` behind feature `host`.
- The engine uses `sdl3-sys 0.6.8` with `build-from-source` and consumes raw
  `SDL_Event` values from `SDL_PollEvent`.
- GPUI Box is pinned to revision
  `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`.
- `WgpuHost::dispatch` accepts `gpui::PlatformInput`; committed UTF-8 text uses
  `WgpuHost::dispatch_text` because GPUI has no committed-text
  `PlatformInput` variant.
- SDL retains ownership of window creation, the event loop, swapchain and
  presentation.

## Architecture

The repository becomes a Cargo workspace containing its current root package
and `crates/gpui-box-sdl`. The adapter crate depends on `gpui-box` and exactly
`sdl3-sys 0.6.8`; it does not depend on `gpui-box-wgpu`. A passthrough
`build-from-source` feature enables `sdl3-sys/build-from-source` for standalone
testing and matches the engine's final SDL stack without forcing that build
policy on every library consumer.

The public boundary is deliberately small:

```rust
pub struct SdlInputAdapter { /* retained input state */ }

pub enum SdlHostEvent {
    Input(gpui::PlatformInput),
    TextInput(String),
    TextEditing(TextEditing),
    WindowResized { width: u32, height: u32 },
    FocusChanged(bool),
    Quit,
}

impl SdlInputAdapter {
    pub fn new(viewport: Viewport) -> anyhow::Result<Self>;
    pub fn set_viewport(&mut self, viewport: Viewport) -> anyhow::Result<()>;
    pub unsafe fn adapt(
        &mut self,
        event: &sdl3_sys::everything::SDL_Event,
    ) -> Vec<SdlHostEvent>;
}
```

`adapt` returns a `Vec` because one SDL keyboard event can first produce a
`ModifiersChanged` input and then a `KeyDown` or `KeyUp`. Most events produce
zero or one result. This avoids a callback/trait abstraction and an additional
dependency.

The method is `unsafe` because a publicly constructible raw SDL union may
contain invalid text pointers. Its safety contract requires an event populated
by SDL whose borrowed string pointers remain valid for the call. Null text
pointers are still rejected without dereferencing them. Tests that construct
text events keep their `CString` alive across the call.

## Host events and ownership

`Input` contains events that can be passed directly to
`WgpuHost::dispatch`. `TextInput` contains owned committed UTF-8 text and maps
to `WgpuHost::dispatch_text`. Resize, focus and quit remain host control
events: the consumer decides whether to resize the render target, focus a
native window or end its loop.

`TextEditing` preserves SDL IME preedit text plus its character-based start and
length. The current GPUI headless host has no complete platform IME bridge, so
the adapter must not pretend that composition is committed text. Keeping this
event separate permits a later bidirectional platform adapter without changing
the raw SDL boundary.

Unsupported SDL categories return an empty vector. The initial crate does not
own or poll an event loop and does not create a window.

## Stateful conversion

`SdlInputAdapter` retains:

- current GPUI modifiers and Caps Lock state;
- last logical pointer position for wheel events;
- the active left, right, middle, back or forward mouse button;
- a validated viewport transform.

SDL mouse coordinates are logical window coordinates. The transform is:

```text
gpui_position = (sdl_logical_position - viewport_origin) / viewport_scale
```

Identity is correct when GPUI fills the SDL window. `viewport_scale` describes
an additional compositor transform, not the window's display/HiDPI scale.
Physical swapchain size and display scale continue to be supplied separately
to `WgpuHost::render_to_view`.

Viewport scale must be positive and finite, and origin coordinates must be
finite. Invalid transforms are rejected without changing the current adapter
state.

## Event mapping

| SDL3 input | Output |
| --- | --- |
| `SDL_EVENT_MOUSE_MOTION` | `PlatformInput::MouseMove` with transformed position, pressed button and retained modifiers |
| `SDL_EVENT_MOUSE_BUTTON_DOWN` | `PlatformInput::MouseDown` with SDL click count and `first_mouse = false` |
| `SDL_EVENT_MOUSE_BUTTON_UP` | `PlatformInput::MouseUp` and button-state release |
| `SDL_EVENT_MOUSE_WHEEL` | `PlatformInput::ScrollWheel` using line delta, current pointer position and `TouchPhase::Moved`; `SDL_MOUSEWHEEL_FLIPPED` reverses both axes |
| `SDL_EVENT_WINDOW_MOUSE_LEAVE` | `PlatformInput::MouseExited` |
| `SDL_EVENT_KEY_DOWN` | optional `ModifiersChanged`, then `PlatformInput::KeyDown` |
| `SDL_EVENT_KEY_UP` | optional `ModifiersChanged`, then `PlatformInput::KeyUp` |
| `SDL_EVENT_TEXT_INPUT` | owned `TextInput`; empty text is ignored |
| `SDL_EVENT_TEXT_EDITING` | owned `TextEditing`; this is not dispatched as committed text |
| `SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED` | validated positive physical `WindowResized` |
| `SDL_EVENT_WINDOW_FOCUS_GAINED/LOST` | `FocusChanged`; focus loss also clears retained button/modifier state |
| `SDL_EVENT_QUIT` | `Quit` |

Mouse buttons map left, right, middle, back and forward to GPUI equivalents.
When SDL reports multiple pressed buttons but GPUI accepts only one on
`MouseMoveEvent`, priority is left, right, middle, back, then forward.

Keyboard mapping uses stable GPUI key names. Named keys cover arrows,
navigation/editing, Enter, Tab, Escape, Space, modifiers, F1–F12 and keypad.
Letters, digits and common punctuation are normalized to lowercase logical key
names. `key_char` remains `None` in key events so `SDL_EVENT_TEXT_INPUT` is the
single source of committed text and characters are never inserted twice.
SDL's repeat flag maps to `KeyDownEvent::is_held`.

SDL Ctrl maps to GPUI `control`; SDL GUI maps to `platform`; Alt, Shift,
Function and Caps Lock map independently. Ctrl is not aliased to `platform`.

## Testing strategy

Development follows strict red-green-refactor cycles.

1. Pure tests construct literal SDL event structs/unions and verify viewport
   validation, pointer transform, all supported buttons, wheel direction,
   modifier transitions, representative named/printable keys, repeat, UTF-8
   committed text, IME preservation, resize/focus/quit and ignored events.
2. A serialized event-pump test initializes `SDL_INIT_EVENTS`, sends a literal
   event through `SDL_PushEvent`, retrieves it with `SDL_PollEvent`, and adapts
   the retrieved raw event. This verifies the exact `sdl3-sys` ABI and linked
   SDL build used by the engine.
3. An integration test uses real GPU resources and `WgpuHost`, renders once to
   build GPUI's dispatch tree, then sends raw SDL mouse, keyboard and text
   events through `SdlInputAdapter`. Real GPUI listeners must observe the
   pointer action, shortcut key and literal `olá`.
4. GPU acquisition may explicitly skip only when no adapter exists. Once an
   adapter is acquired, render or dispatch errors fail the test.
5. The full workspace passes formatting, documentation and Clippy with
   `-D warnings -D clippy::cognitive_complexity`.

The tests derive expected GPUI values from literals rather than reusing
adapter helpers. Each test names the production mapping whose removal or
corruption it catches.

## Complexity and safety boundaries

- Both packages deny `clippy::cognitive_complexity` at the default threshold
  of 25.
- Event-category dispatch and each category conversion live in separate small
  functions.
- Raw union access is limited to the adapter module.
- Text pointers are copied during the `adapt` call and are never retained.
- Negative/zero resize dimensions and invalid viewport transforms are ignored
  or rejected before conversion to GPUI geometry.
- No SDL, GPUI or GPU global is initialized by normal library construction.

## Consumption

The engine can depend on both sibling packages by path during development, or
on one immutable Git revision after this repository is pushed. Its event loop
will call `adapt`, route `Input` and `TextInput` to `WgpuHost`, and handle
resize/focus/quit itself. Integrating that routing into `rust-engine` is a
separate change after this crate and its final-stack tests pass.

## Out of scope

- Modifying or repointing `rust-engine`.
- Owning the SDL event loop or SDL window.
- Clipboard, cursor-shape, accessibility or native dialog services.
- Pretending IME preedit is committed text.
- Touch, pen, gamepad, file drop or gesture mapping in the initial adapter.
- A generic event-sink trait or direct dependency on `WgpuHost`.
