# GPUI Box SDL3 Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a reusable raw-SDL3-to-GPUI input adapter and prove the final SDL → adapter → `WgpuHost` → GPUI listener stack.

**Architecture:** Convert the repository into a two-package workspace while keeping `gpui-box-wgpu` at the root. The independent `gpui-box-sdl` member translates caller-owned `SDL_Event` values into GPUI inputs or explicit host-control events; only its integration tests depend on the root renderer crate.

**Tech Stack:** Rust 2024, Rust 1.97+, `sdl3-sys = 0.6.8`, GPUI Box revision `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`, wgpu 30.0.1, Clippy

**Spec:** `docs/superpowers/specs/2026-09-04-gpui-box-sdl-design.md`

## Global Constraints

- Do not modify `C:\Users\daniel\Documents\GitHub\rust-engine`.
- `gpui-box-sdl` depends directly on `sdl3-sys = "=0.6.8"`, not the high-level `sdl3` crate.
- The adapter crate must not depend normally on `gpui-box-wgpu`; that dependency is test-only.
- SDL remains owner of the window and event loop.
- Key events never set `key_char`; committed text comes only from `SDL_EVENT_TEXT_INPUT`.
- IME preedit remains distinct from committed text.
- No clipboard, cursor, accessibility, touch, pen, gamepad, drop or gesture adapter is added.
- Every new mapping is introduced by a failing test first.
- Deny `clippy::cognitive_complexity` at its default threshold of 25; never add an allowance.

---

### Task 1: Create the workspace member and validated viewport

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/gpui-box-sdl/Cargo.toml`
- Create: `crates/gpui-box-sdl/src/lib.rs`
- Create: `crates/gpui-box-sdl/tests/viewport.rs`

**Interfaces:**
- Consumes: logical SDL coordinates and an embedded viewport origin/scale
- Produces: `Viewport`, `SdlInputAdapter::new`, `SdlInputAdapter::set_viewport`

- [ ] **Step 1: Add workspace and empty crate scaffolding**

Append to the root manifest:

```toml
[workspace]
members = ["crates/gpui-box-sdl"]
default-members = [".", "crates/gpui-box-sdl"]
resolver = "3"
```

Create the member manifest:

```toml
[package]
name = "gpui-box-sdl"
version = "0.1.0"
edition = "2024"
rust-version = "1.97"
publish = false
license = "Apache-2.0"
description = "SDL3 input adapter for GPUI Box hosts"

[lib]
name = "gpui_sdl"
path = "src/lib.rs"

[lints.clippy]
cognitive_complexity = "deny"

[features]
default = []
build-from-source = ["sdl3-sys/build-from-source"]

[dependencies]
anyhow = "1"
gpui = { package = "gpui-box", git = "https://github.com/fran0220/gpui-box.git", rev = "5c7e9eb6de8c8db3e7ff659934166218fb60f9f2" }
sdl3-sys = "=0.6.8"

[dev-dependencies]
gpui_wgpu = { package = "gpui-box-wgpu", path = "../..", features = ["host"] }
pollster = "0.4"
```

Create an empty `src/lib.rs`, run `cargo generate-lockfile`, then run
`cargo check --workspace`. Dependency resolution must finish before the first
behavior RED.

- [ ] **Step 2: Write viewport tests before its implementation**

Create `tests/viewport.rs`:

```rust
use gpui_sdl::{SdlInputAdapter, Viewport};

#[test]
fn viewport_converts_window_coordinates_to_gpui_coordinates() {
    let adapter = SdlInputAdapter::new(Viewport {
        origin_x: 20.0,
        origin_y: 10.0,
        scale: 2.0,
    })
    .unwrap();
    assert_eq!(adapter.map_position(60.0, 34.0), gpui::point(gpui::px(20.0), gpui::px(12.0)));
}

#[test]
fn invalid_viewport_does_not_replace_the_current_transform() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    assert!(adapter.set_viewport(Viewport { scale: 0.0, ..Viewport::default() }).is_err());
    assert!(adapter.set_viewport(Viewport { origin_x: f32::NAN, ..Viewport::default() }).is_err());
    assert_eq!(adapter.map_position(12.0, 8.0), gpui::point(gpui::px(12.0), gpui::px(8.0)));
}
```

- [ ] **Step 3: Run the viewport test and verify RED**

Run:

```powershell
cargo test -p gpui-box-sdl --test viewport
```

Expected: compilation fails because `Viewport` and `SdlInputAdapter` do not
exist. Fix only dependency or fixture errors until this is the reason.

- [ ] **Step 4: Implement the minimum viewport state**

In `src/lib.rs`, declare `mod adapter;` and re-export its public types. In
`src/adapter.rs`, implement:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub origin_x: f32,
    pub origin_y: f32,
    pub scale: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { origin_x: 0.0, origin_y: 0.0, scale: 1.0 }
    }
}

pub struct SdlInputAdapter {
    viewport: Viewport,
    modifiers: gpui::Modifiers,
    capslock: gpui::Capslock,
    pointer: gpui::Point<gpui::Pixels>,
    pressed_button: Option<gpui::MouseButton>,
}

impl SdlInputAdapter {
    pub fn new(viewport: Viewport) -> anyhow::Result<Self> {
        validate_viewport(viewport)?;
        Ok(Self {
            viewport,
            modifiers: gpui::Modifiers::default(),
            capslock: gpui::Capslock::default(),
            pointer: gpui::point(gpui::px(0.0), gpui::px(0.0)),
            pressed_button: None,
        })
    }

    pub fn set_viewport(&mut self, viewport: Viewport) -> anyhow::Result<()> {
        validate_viewport(viewport)?;
        self.viewport = viewport;
        Ok(())
    }

    pub fn map_position(&self, x: f32, y: f32) -> gpui::Point<gpui::Pixels> {
        gpui::point(
            gpui::px((x - self.viewport.origin_x) / self.viewport.scale),
            gpui::px((y - self.viewport.origin_y) / self.viewport.scale),
        )
    }
}

fn validate_viewport(viewport: Viewport) -> anyhow::Result<()> {
    anyhow::ensure!(viewport.origin_x.is_finite() && viewport.origin_y.is_finite(), "viewport origin must be finite");
    anyhow::ensure!(viewport.scale.is_finite() && viewport.scale > 0.0, "viewport scale must be positive and finite");
    Ok(())
}
```

- [ ] **Step 5: Verify GREEN and commit**

Run the focused test, `cargo check --workspace`, then commit:

```powershell
git add Cargo.toml Cargo.lock crates/gpui-box-sdl
git commit -m "build: add GPUI Box SDL adapter crate"
```

---

### Task 2: Translate pointer and wheel events

**Files:**
- Modify: `crates/gpui-box-sdl/src/adapter.rs`
- Create: `crates/gpui-box-sdl/tests/mouse.rs`

**Interfaces:**
- Consumes: raw SDL mouse motion/button/wheel and mouse-leave events
- Produces: `SdlHostEvent::Input(gpui::PlatformInput)` from `unsafe SdlInputAdapter::adapt`

- [ ] **Step 1: Write failing literal mouse-event tests**

Create helpers that construct the union directly, for example:

```rust
fn motion(x: f32, y: f32, state: sdl::SDL_MouseButtonFlags) -> sdl::SDL_Event {
    sdl::SDL_Event {
        motion: sdl::SDL_MouseMotionEvent {
            r#type: sdl::SDL_EVENT_MOUSE_MOTION,
            x,
            y,
            state,
            ..Default::default()
        },
    }
}
```

Tests must assert these literal outcomes by pattern matching:

```rust
#[test]
fn motion_applies_viewport_and_reports_pressed_left_button() {
    let mut adapter = SdlInputAdapter::new(Viewport { origin_x: 10.0, origin_y: 4.0, scale: 2.0 }).unwrap();
    let events = unsafe { adapter.adapt(&motion(30.0, 20.0, sdl::SDL_BUTTON_LMASK)) };
    let SdlHostEvent::Input(gpui::PlatformInput::MouseMove(event)) = &events[0] else { panic!("expected mouse move") };
    assert_eq!(event.position, gpui::point(gpui::px(10.0), gpui::px(8.0)));
    assert_eq!(event.pressed_button, Some(gpui::MouseButton::Left));
}
```

Add separate tests proving:

- left/right/middle/X1/X2 map to Left/Right/Middle/Navigate(Back)/Navigate(Forward);
- mouse down keeps SDL `clicks = 2` and mouse up releases the retained button;
- motion mask priority is left before right before middle before X1 before X2;
- a normal wheel `(2.0, -3.0)` yields `ScrollDelta::Lines(point(2.0, -3.0))` at its transformed `mouse_x/mouse_y`;
- `SDL_MOUSEWHEEL_FLIPPED` yields `(-2.0, 3.0)`;
- window mouse leave yields `MouseExited` at the last pointer position;
- an unsupported button and unrelated SDL event return an empty vector.

- [ ] **Step 2: Run mouse tests and verify RED**

```powershell
cargo test -p gpui-box-sdl --test mouse
```

Expected: compilation fails because `SdlHostEvent` and `adapt` do not exist.

- [ ] **Step 3: Implement event output and small mouse converters**

Add the public enum and IME payload now because later mappings share it:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEditing {
    pub text: String,
    pub start: i32,
    pub length: i32,
}

#[derive(Clone, Debug)]
pub enum SdlHostEvent {
    Input(gpui::PlatformInput),
    TextInput(String),
    TextEditing(TextEditing),
    WindowResized { width: u32, height: u32 },
    FocusChanged(bool),
    Quit,
}
```

Implement `adapt` as one event-type match that delegates only the four mouse
categories to `adapt_motion`, `adapt_button`, `adapt_wheel` and
`adapt_mouse_exit`; every other variant returns `Vec::new()`. Later TDD tasks
add keyboard, text and window arms only after their focused tests fail.

The public method must carry this safety documentation and signature:

```rust
/// # Safety
/// `event` must have been populated by SDL, and any pointers carried by it
/// must remain valid for this call.
pub unsafe fn adapt(&mut self, event: &sdl::SDL_Event) -> Vec<SdlHostEvent>
```

Use `sdl3_sys::everything as sdl`. Keep each converter under the Clippy
complexity threshold. `adapt_motion` derives its pressed button from the SDL
bit mask. `adapt_button` updates the retained button only after recognizing
the button. `adapt_wheel` uses the event's own mouse coordinates and retained
modifiers. All positions pass through `map_position`.

- [ ] **Step 4: Verify GREEN and commit**

```powershell
cargo test -p gpui-box-sdl --test mouse
cargo test -p gpui-box-sdl --test viewport
git add crates/gpui-box-sdl/src crates/gpui-box-sdl/tests/mouse.rs
git commit -m "feat: translate SDL pointer input to GPUI"
```

---

### Task 3: Translate keyboard, modifiers and text

**Files:**
- Create: `crates/gpui-box-sdl/src/keyboard.rs`
- Modify: `crates/gpui-box-sdl/src/adapter.rs`
- Create: `crates/gpui-box-sdl/tests/keyboard_text.rs`

**Interfaces:**
- Consumes: SDL key down/up, keycode, key modifiers, committed text and IME preedit
- Produces: GPUI modifier/key inputs plus `TextInput` and `TextEditing`

- [ ] **Step 1: Write failing keyboard and text tests**

Construct keyboard unions with `SDL_KeyboardEvent` and text unions with a live
`CString`. Add tests with these exact assertions:

```rust
#[test]
fn key_down_emits_modifier_change_before_repeat_key() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let raw = key_event(sdl::SDL_EVENT_KEY_DOWN, sdl::SDLK_C, sdl::SDL_KMOD_CTRL | sdl::SDL_KMOD_SHIFT, true);
    let events = unsafe { adapter.adapt(&raw) };
    let SdlHostEvent::Input(gpui::PlatformInput::ModifiersChanged(mods)) = &events[0] else { panic!("expected modifiers") };
    assert!(mods.control && mods.shift);
    assert!(!mods.platform);
    let SdlHostEvent::Input(gpui::PlatformInput::KeyDown(key)) = &events[1] else { panic!("expected key down") };
    assert_eq!(key.keystroke.key, "c");
    assert_eq!(key.keystroke.key_char, None);
    assert!(key.is_held);
}

#[test]
fn text_input_owns_utf8_without_turning_it_into_a_key_event() {
    let text = std::ffi::CString::new("olá").unwrap();
    let raw = text_input(text.as_ptr());
    let events = unsafe { SdlInputAdapter::new(Viewport::default()).unwrap().adapt(&raw) };
    let SdlHostEvent::TextInput(value) = &events[0] else { panic!("expected text") };
    assert_eq!(value, "olá");
}
```

Also test:

- key up emits `KeyUp` with the same normalized key;
- GUI maps only to `platform`, Alt to `alt`, and Caps Lock to `Capslock { on: true }`;
- repeating an unchanged modifier set does not emit another `ModifiersChanged`;
- literal keys Left, Return, Delete, F12, keypad 7, `A`, `0`, slash and semicolon normalize to `left`, `enter`, `delete`, `f12`, `7`, `a`, `0`, `/`, `;`;
- an unknown key yields only a modifier change, if one occurred;
- null or empty committed-text pointers produce no output;
- `SDL_EVENT_TEXT_EDITING` preserves literal `"ção"`, `start = 1`, `length = 2` and is not a `TextInput`.

- [ ] **Step 2: Run focused tests and verify RED**

```powershell
cargo test -p gpui-box-sdl --test keyboard_text
```

Expected: the mouse-only stubs return no keyboard/text events and assertions
fail with the expected variants missing.

- [ ] **Step 3: Implement key normalization in a dedicated module**

Create `keyboard.rs` with:

```rust
pub(crate) fn key_name(key: sdl::SDL_Keycode) -> Option<String>
pub(crate) fn modifiers(raw: sdl::SDL_Keymod) -> (gpui::Modifiers, gpui::Capslock)
```

`key_name` returns static names for arrows, Home/End/PageUp/PageDown,
Insert/Delete, Escape/Tab/Backspace/Enter/Space, F1–F12 and keypad operations.
For keycode values in the ASCII ranges `A..=Z`, `0..=9` and common printable
punctuation, convert the underlying scalar with `char::from_u32`, lowercase it
and return its string. Map keypad digits explicitly to `"0"` through `"9"`.
Return `None` for unknown/noncharacter values.

`modifiers` tests SDL masks independently:

```rust
gpui::Modifiers {
    control: raw.0 & sdl::SDL_KMOD_CTRL.0 != 0,
    alt: raw.0 & sdl::SDL_KMOD_ALT.0 != 0,
    shift: raw.0 & sdl::SDL_KMOD_SHIFT.0 != 0,
    platform: raw.0 & sdl::SDL_KMOD_GUI.0 != 0,
    function: false,
}
```

Caps Lock uses `SDL_KMOD_CAPS` only. `SDL_KMOD_MODE` and
`SDL_KMOD_LEVEL5` do not map to GPUI's Fn modifier. `adapt_keyboard` emits a
`ModifiersChangedEvent` only when either retained value changes, then emits
the key event if `key_name` succeeds. `KeyDownEvent` uses `is_held = repeat`,
`prefer_character_input = false`; both down and up keystrokes use
`key_char = None`.

Text conversion checks for null before `CStr::from_ptr`, copies with
`to_string_lossy().into_owned()`, and drops an empty committed string. Preedit
conversion retains empty strings because SDL may use one to clear composition.

- [ ] **Step 4: Verify GREEN and commit**

```powershell
cargo test -p gpui-box-sdl --test keyboard_text
cargo test -p gpui-box-sdl
git add crates/gpui-box-sdl/src crates/gpui-box-sdl/tests/keyboard_text.rs
git commit -m "feat: translate SDL keyboard and text input"
```

---

### Task 4: Translate host controls and verify the real SDL event pump

**Files:**
- Modify: `crates/gpui-box-sdl/src/adapter.rs`
- Create: `crates/gpui-box-sdl/tests/host_events.rs`
- Create: `crates/gpui-box-sdl/tests/event_pump.rs`

**Interfaces:**
- Consumes: SDL quit, pixel resize, focus and real pushed/polled events
- Produces: explicit host events and verified SDL ABI/linkage

- [ ] **Step 1: Write failing host-control tests**

Construct `SDL_WindowEvent` unions and assert:

- pixel resize `data1 = 1920`, `data2 = 1080` produces exactly
  `WindowResized { width: 1920, height: 1080 }`;
- zero or negative dimensions produce no output;
- logical `SDL_EVENT_WINDOW_RESIZED` is ignored;
- focus gained/lost produce `FocusChanged(true/false)`;
- focus loss after Ctrl was active first emits a default `ModifiersChanged`,
  then `FocusChanged(false)`, and a subsequent mouse-leave event reports no
  pressed button or modifiers;
- an event with `r#type = SDL_EVENT_QUIT.0` produces `Quit`.

- [ ] **Step 2: Run host-control tests and verify RED**

```powershell
cargo test -p gpui-box-sdl --test host_events
```

Expected: assertions fail because host/window stubs still return empty output.

- [ ] **Step 3: Implement host-control conversion**

Convert only physical `WINDOW_PIXEL_SIZE_CHANGED`, using `u32::try_from` and
rejecting zero. Focus loss emits a default `ModifiersChangedEvent` when the
retained modifiers or Caps Lock were non-default, clears `modifiers`,
`capslock` and `pressed_button`, then returns `FocusChanged(false)`. Focus gain
does not invent state. Quit is a direct single result.

- [ ] **Step 4: Add and run a real event-pump characterization test**

Gate `tests/event_pump.rs` with
`#![cfg(feature = "build-from-source")]`. Serialize SDL lifetime with a static
`Mutex`. The test must execute this real flow:

```rust
assert!(unsafe { sdl::SDL_Init(sdl::SDL_INIT_EVENTS) });
let mut pushed = sdl::SDL_Event {
    motion: sdl::SDL_MouseMotionEvent {
        r#type: sdl::SDL_EVENT_MOUSE_MOTION,
        x: 42.0,
        y: 24.0,
        ..Default::default()
    },
};
assert!(unsafe { sdl::SDL_PushEvent(&mut pushed) });
let mut polled = std::mem::MaybeUninit::<sdl::SDL_Event>::uninit();
assert!(unsafe { sdl::SDL_PollEvent(polled.as_mut_ptr()) });
let polled = unsafe { polled.assume_init() };
let output = unsafe { adapter.adapt(&polled) };
unsafe { sdl::SDL_Quit() };
```

Assert the resulting GPUI position is exactly `(42, 24)`. Use an RAII test
guard whose `Drop` calls `SDL_Quit` so a failed assertion cannot leave global
SDL initialized.

This test adds no production behavior; it characterizes the external SDL ABI
after the adapter mappings have already completed their RED/GREEN cycles. Run:

```powershell
cargo test -p gpui-box-sdl --features build-from-source --test event_pump -- --test-threads=1
```

- [ ] **Step 5: Verify GREEN and commit**

```powershell
cargo test -p gpui-box-sdl --test host_events
cargo test -p gpui-box-sdl --features build-from-source --test event_pump -- --test-threads=1
git add crates/gpui-box-sdl/src/adapter.rs crates/gpui-box-sdl/tests
git commit -m "feat: expose SDL host control events"
```

---

### Task 5: Prove SDL input reaches real GPUI listeners through WgpuHost

**Files:**
- Create: `crates/gpui-box-sdl/tests/wgpu_host_stack.rs`

**Interfaces:**
- Consumes: `SdlHostEvent`, test-only `WgpuHost`, one caller-owned texture
- Produces: final-stack behavioral evidence without production coupling

- [ ] **Step 1: Write the end-to-end listener integration test**

Create a `ProbeView` with `FocusHandle`, `Rc<Cell<bool>>` mouse state and
`Rc<RefCell<Vec<String>>>` key/text state. Its rendered full-size `div` tracks
focus and registers `on_mouse_move` plus `on_key_down`. Create an external
wgpu instance with all backends and default flags, request a high-performance
adapter without a compatible surface, then request a device with empty
features and downlevel limits. Wrap the resulting device/queue in `Arc`. Create
a 128 × 64 `Rgba8UnormSrgb` texture with
`RENDER_ATTACHMENT | TEXTURE_BINDING`, expose an `Rgba8Unorm` view, and build a
`WgpuHost` using `CosmicTextSystem::new_without_system_fonts("sans-serif")`.
Return early with an explicit skip message only if adapter acquisition fails;
device acquisition and later failures use `expect`/`unwrap`.

After the initial `render_to_view`, construct three raw SDL events:

```rust
SDL_MouseMotionEvent { r#type: SDL_EVENT_MOUSE_MOTION, x: 12.0, y: 12.0, ..Default::default() }
SDL_KeyboardEvent { r#type: SDL_EVENT_KEY_DOWN, key: SDLK_X, down: true, ..Default::default() }
let committed = CString::new("olá").unwrap();
SDL_TextInputEvent { r#type: SDL_EVENT_TEXT_INPUT, text: committed.as_ptr(), ..Default::default() }
```

Keep the `CString` binding alive. Feed each raw event into the adapter and
route outputs in test code only:

```rust
fn deliver(host: &mut WgpuHost, events: Vec<SdlHostEvent>) -> anyhow::Result<()> {
    for event in events {
        match event {
            SdlHostEvent::Input(input) => { host.dispatch(input)?; }
            SdlHostEvent::TextInput(text) => host.dispatch_text(&text)?,
            SdlHostEvent::TextEditing(_) |
            SdlHostEvent::WindowResized { .. } |
            SdlHostEvent::FocusChanged(_) |
            SdlHostEvent::Quit => {}
        }
    }
    Ok(())
}
```

Assert the real view observed mouse movement and the key listener received
`"x"` followed by committed `"olá"`. The key listener must record the
keystroke key when `key_char` is absent, so the assertion proves both paths.

- [ ] **Step 2: Run the composed stack test**

```powershell
cargo test -p gpui-box-sdl --test wgpu_host_stack -- --nocapture
```

Expected: PASS. This task adds no production API; it proves the independently
tested adapter and host compose through their public seam. A failure indicates
a real integration mismatch and must be reduced to a new focused failing test
before changing production code.

- [ ] **Step 3: Verify the boundary and commit**

```powershell
cargo test -p gpui-box-sdl --test wgpu_host_stack -- --nocapture
cargo test --workspace --all-features --all-targets
rg -n 'gpui_wgpu|WgpuHost' crates/gpui-box-sdl/src crates/gpui-box-sdl/Cargo.toml
git add crates/gpui-box-sdl/tests/wgpu_host_stack.rs
git commit -m "test: prove SDL input reaches GPUI host"
```

Expected: production source has no `gpui_wgpu`/`WgpuHost` reference; only the
manifest's dev dependency and integration test use the renderer.

---

### Task 6: Document consumption and enforce release gates

**Files:**
- Modify: `README.md`
- Create: `crates/gpui-box-sdl/README.md`
- Modify: `crates/gpui-box-sdl/src/lib.rs`

**Interfaces:**
- Consumes: finalized adapter API
- Produces: path/Git setup, safety contract, routing example and clean workspace gates

- [ ] **Step 1: Document both dependency modes and event routing**

Document path usage:

```toml
gpui_sdl = { package = "gpui-box-sdl", path = "../gpui-box-wgpu/crates/gpui-box-sdl", features = ["build-from-source"] }
gpui_wgpu = { package = "gpui-box-wgpu", path = "../gpui-box-wgpu", features = ["host"] }
```

Document Git usage with one repository URL and immutable revision, using
`package` to select each package. Include the exact `unsafe` safety contract
and the `deliver` match from Task 5. State that SDL logical mouse coordinates
use the viewport transform, while physical resize and display scale go to
`render_to_view`.

- [ ] **Step 2: Make the member README its crate-level documentation**

Add `#![doc = include_str!("../README.md")]` to the top of its `src/lib.rs`.
Mark the routing example `no_run`; it must compile under doctest without
requiring SDL or GPU initialization.

- [ ] **Step 3: Run the exact final verification matrix**

```powershell
cargo fmt --all
cargo test --workspace --all-targets
cargo test --workspace --all-targets --all-features -- --test-threads=1
cargo test --workspace --doc --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::cognitive_complexity
cargo fmt --all --check
cargo doc --workspace --no-deps --all-features
git diff --check
git -C 'C:\Users\daniel\Documents\GitHub\rust-engine' status --short
```

Expected: all commands exit zero; the engine status remains the same clean
output recorded before implementation. A GPU test may explicitly skip only
before adapter acquisition; acquired-device failures must fail.

- [ ] **Step 4: Audit dependency boundaries**

```powershell
cargo tree -p gpui-box-sdl -e normal
cargo tree -p gpui-box-sdl -e normal | Select-String 'gpui-box-wgpu'
rg -n 'gpui_wgpu|WgpuHost' crates/gpui-box-sdl/src crates/gpui-box-sdl/Cargo.toml
```

Expected: the normal dependency tree and production sources contain no
renderer dependency; `WgpuHost` appears only in tests/docs and the manifest's
dev dependency.

- [ ] **Step 5: Commit documentation and gate cleanup**

```powershell
git add Cargo.toml Cargo.lock README.md crates/gpui-box-sdl
git commit -m "docs: explain GPUI Box SDL adapter"
git status --short --branch
```

Expected final status: clean `main` branch.
