use std::{collections::HashMap, ffi::CStr, path::PathBuf};

use sdl3_sys::everything as sdl;

use crate::keyboard;

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

impl TextEditing {
    /// Converts SDL's UTF-8 character offsets to GPUI's UTF-16 selection range.
    pub fn selection_utf16(&self) -> Option<std::ops::Range<usize>> {
        let start = usize::try_from(self.start).ok()?;
        let length = usize::try_from(self.length).ok()?;
        let mut chars = self.text.chars();
        let utf16_start = chars.by_ref().take(start).map(char::len_utf16).sum();
        let utf16_length = chars.take(length).map(char::len_utf16).sum::<usize>();
        Some(utf16_start..utf16_start.saturating_add(utf16_length))
    }
}

pub type SdlWindowId = sdl::SDL_WindowID;

#[derive(Clone)]
pub enum RoutedSdlHostEvent {
    Window {
        window_id: SdlWindowId,
        event: SdlHostEvent,
    },
    CloseRequested {
        window_id: SdlWindowId,
    },
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub origin_x: f32,
    pub origin_y: f32,
    pub scale: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            scale: 1.0,
        }
    }
}

pub struct SdlInputAdapter {
    viewport: Viewport,
    modifiers: gpui::Modifiers,
    capslock: gpui::Capslock,
    pointer: gpui::Point<gpui::Pixels>,
    pressed_button: Option<gpui::MouseButton>,
    drop_paths: Vec<PathBuf>,
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
            drop_paths: Vec::new(),
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

    /// Translates one raw SDL event into zero or more host actions.
    ///
    /// # Safety
    ///
    /// `event` must have been populated by SDL, and any pointers carried by it
    /// must remain valid for this call.
    pub unsafe fn adapt(&mut self, event: &sdl::SDL_Event) -> Vec<SdlHostEvent> {
        let mut output = Vec::new();
        unsafe { self.adapt_into(event, |event| output.push(event)) };
        output
    }

    /// Translates one raw SDL event and emits results into caller-owned storage.
    ///
    /// # Safety
    ///
    /// `event` must have been populated by SDL, and any pointers carried by it
    /// must remain valid for this call.
    pub unsafe fn adapt_into(
        &mut self,
        event: &sdl::SDL_Event,
        mut emit: impl FnMut(SdlHostEvent),
    ) {
        match event.event_type() {
            sdl::SDL_EVENT_MOUSE_MOTION => emit(self.adapt_motion(unsafe { event.motion })),
            sdl::SDL_EVENT_MOUSE_BUTTON_DOWN | sdl::SDL_EVENT_MOUSE_BUTTON_UP => {
                if let Some(event) = self.adapt_button(unsafe { event.button }) {
                    emit(event);
                }
            }
            sdl::SDL_EVENT_MOUSE_WHEEL => emit(self.adapt_wheel(unsafe { event.wheel })),
            sdl::SDL_EVENT_WINDOW_MOUSE_LEAVE => emit(self.adapt_mouse_exit()),
            sdl::SDL_EVENT_KEY_DOWN | sdl::SDL_EVENT_KEY_UP => {
                self.adapt_keyboard(unsafe { event.key }, &mut emit)
            }
            sdl::SDL_EVENT_TEXT_INPUT => {
                if let Some(event) = self.adapt_text_input(unsafe { event.text }) {
                    emit(event);
                }
            }
            sdl::SDL_EVENT_TEXT_EDITING => {
                if let Some(event) = self.adapt_text_editing(unsafe { event.edit }) {
                    emit(event);
                }
            }
            sdl::SDL_EVENT_DROP_BEGIN
            | sdl::SDL_EVENT_DROP_FILE
            | sdl::SDL_EVENT_DROP_POSITION
            | sdl::SDL_EVENT_DROP_COMPLETE => {
                self.adapt_file_drop(unsafe { event.drop }, &mut emit)
            }
            sdl::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => {
                if let Some(event) = self.adapt_resize(unsafe { event.window }) {
                    emit(event);
                }
            }
            sdl::SDL_EVENT_WINDOW_FOCUS_GAINED => emit(SdlHostEvent::FocusChanged(true)),
            sdl::SDL_EVENT_WINDOW_FOCUS_LOST => self.adapt_focus_loss(&mut emit),
            sdl::SDL_EVENT_QUIT => emit(SdlHostEvent::Quit),
            _ => {}
        }
    }

    fn adapt_motion(&mut self, event: sdl::SDL_MouseMotionEvent) -> SdlHostEvent {
        self.pointer = self.map_position(event.x, event.y);
        self.pressed_button = pressed_button(event.state);
        SdlHostEvent::Input(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
            position: self.pointer,
            pressed_button: self.pressed_button,
            modifiers: self.modifiers,
        }))
    }

    fn adapt_button(&mut self, event: sdl::SDL_MouseButtonEvent) -> Option<SdlHostEvent> {
        let button = mouse_button(event.button)?;
        self.pointer = self.map_position(event.x, event.y);
        if event.down {
            self.pressed_button = Some(button);
            return Some(SdlHostEvent::Input(gpui::PlatformInput::MouseDown(
                gpui::MouseDownEvent {
                    button,
                    position: self.pointer,
                    modifiers: self.modifiers,
                    click_count: usize::from(event.clicks),
                    first_mouse: false,
                },
            )));
        }
        if self.pressed_button == Some(button) {
            self.pressed_button = None;
        }
        Some(SdlHostEvent::Input(gpui::PlatformInput::MouseUp(
            gpui::MouseUpEvent {
                button,
                position: self.pointer,
                modifiers: self.modifiers,
                click_count: usize::from(event.clicks),
            },
        )))
    }

    fn adapt_wheel(&mut self, event: sdl::SDL_MouseWheelEvent) -> SdlHostEvent {
        self.pointer = self.map_position(event.mouse_x, event.mouse_y);
        let direction = if event.direction == sdl::SDL_MOUSEWHEEL_FLIPPED {
            -1.0
        } else {
            1.0
        };
        SdlHostEvent::Input(gpui::PlatformInput::ScrollWheel(gpui::ScrollWheelEvent {
            position: self.pointer,
            delta: gpui::ScrollDelta::Lines(gpui::point(event.x * direction, event.y * direction)),
            modifiers: self.modifiers,
            touch_phase: gpui::TouchPhase::Moved,
        }))
    }

    fn adapt_mouse_exit(&self) -> SdlHostEvent {
        SdlHostEvent::Input(gpui::PlatformInput::MouseExited(gpui::MouseExitEvent {
            position: self.pointer,
            pressed_button: self.pressed_button,
            modifiers: self.modifiers,
        }))
    }

    fn adapt_keyboard(
        &mut self,
        event: sdl::SDL_KeyboardEvent,
        emit: &mut impl FnMut(SdlHostEvent),
    ) {
        let (modifiers, capslock) = keyboard::modifiers(event.r#mod);
        if let Some(event) = self.update_modifiers(modifiers, capslock) {
            emit(event);
        }
        let Some(key) = keyboard::key_name(event.key) else {
            return;
        };
        let keystroke = gpui::Keystroke {
            modifiers,
            key,
            key_char: None,
        };
        let input = if event.down {
            gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                keystroke,
                is_held: event.repeat,
                prefer_character_input: false,
            })
        } else {
            gpui::PlatformInput::KeyUp(gpui::KeyUpEvent { keystroke })
        };
        emit(SdlHostEvent::Input(input));
    }

    fn update_modifiers(
        &mut self,
        modifiers: gpui::Modifiers,
        capslock: gpui::Capslock,
    ) -> Option<SdlHostEvent> {
        if self.modifiers == modifiers && self.capslock == capslock {
            return None;
        }
        self.modifiers = modifiers;
        self.capslock = capslock;
        Some(SdlHostEvent::Input(gpui::PlatformInput::ModifiersChanged(
            gpui::ModifiersChangedEvent {
                modifiers,
                capslock,
            },
        )))
    }

    fn adapt_text_input(&self, event: sdl::SDL_TextInputEvent) -> Option<SdlHostEvent> {
        copy_text(event.text)
            .filter(|text| !text.is_empty())
            .map(SdlHostEvent::TextInput)
    }

    fn adapt_text_editing(&self, event: sdl::SDL_TextEditingEvent) -> Option<SdlHostEvent> {
        let text = copy_text(event.text)?;
        Some(SdlHostEvent::TextEditing(TextEditing {
            text,
            start: event.start,
            length: event.length,
        }))
    }

    fn adapt_file_drop(&mut self, event: sdl::SDL_DropEvent, emit: &mut impl FnMut(SdlHostEvent)) {
        self.pointer = self.map_position(event.x, event.y);
        match event.r#type {
            sdl::SDL_EVENT_DROP_BEGIN => self.drop_paths.clear(),
            sdl::SDL_EVENT_DROP_FILE => {
                if let Some(path) = copy_text(event.data).filter(|path| !path.is_empty()) {
                    self.drop_paths.push(path.into());
                }
            }
            sdl::SDL_EVENT_DROP_COMPLETE if !self.drop_paths.is_empty() => {
                let paths = gpui::ExternalPaths(self.drop_paths.drain(..).collect());
                emit(SdlHostEvent::Input(gpui::PlatformInput::FileDrop(
                    gpui::FileDropEvent::Entered {
                        position: self.pointer,
                        paths,
                    },
                )));
                emit(SdlHostEvent::Input(gpui::PlatformInput::FileDrop(
                    gpui::FileDropEvent::Submit {
                        position: self.pointer,
                    },
                )));
                emit(SdlHostEvent::Input(gpui::PlatformInput::FileDrop(
                    gpui::FileDropEvent::Ended,
                )));
            }
            _ => {}
        }
    }

    fn adapt_resize(&self, event: sdl::SDL_WindowEvent) -> Option<SdlHostEvent> {
        let width = u32::try_from(event.data1).ok().filter(|value| *value > 0)?;
        let height = u32::try_from(event.data2).ok().filter(|value| *value > 0)?;
        Some(SdlHostEvent::WindowResized { width, height })
    }

    fn adapt_focus_loss(&mut self, emit: &mut impl FnMut(SdlHostEvent)) {
        if let Some(event) =
            self.update_modifiers(gpui::Modifiers::default(), gpui::Capslock::default())
        {
            emit(event);
        }
        self.pressed_button = None;
        emit(SdlHostEvent::FocusChanged(false));
    }
}

#[derive(Default)]
pub struct SdlWindowRouter {
    adapters: HashMap<SdlWindowId, SdlInputAdapter>,
}

impl SdlWindowRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_window(
        &mut self,
        window_id: SdlWindowId,
        viewport: Viewport,
    ) -> anyhow::Result<()> {
        self.adapters
            .insert(window_id, SdlInputAdapter::new(viewport)?);
        Ok(())
    }

    pub fn remove_window(&mut self, window_id: SdlWindowId) -> bool {
        self.adapters.remove(&window_id).is_some()
    }

    /// Routes one SDL event while preserving its window identity.
    ///
    /// # Safety
    ///
    /// `event` must have been populated by SDL, and any pointers carried by it
    /// must remain valid for this call.
    pub unsafe fn adapt(&mut self, event: &sdl::SDL_Event) -> Vec<RoutedSdlHostEvent> {
        let mut output = Vec::new();
        unsafe { self.adapt_into(event, |event| output.push(event)) };
        output
    }

    /// Routes one SDL event into caller-owned output storage.
    ///
    /// # Safety
    ///
    /// `event` must have been populated by SDL, and any pointers carried by it
    /// must remain valid for this call.
    pub unsafe fn adapt_into(
        &mut self,
        event: &sdl::SDL_Event,
        mut emit: impl FnMut(RoutedSdlHostEvent),
    ) {
        let event_type = event.event_type();
        if event_type == sdl::SDL_EVENT_QUIT {
            emit(RoutedSdlHostEvent::Quit);
            return;
        }
        let Some(window_id) = (unsafe { event_window_id(event) }) else {
            return;
        };
        if event_type == sdl::SDL_EVENT_WINDOW_CLOSE_REQUESTED {
            emit(RoutedSdlHostEvent::CloseRequested { window_id });
            return;
        }
        let Some(adapter) = self.adapters.get_mut(&window_id) else {
            return;
        };
        unsafe {
            adapter.adapt_into(event, |event| {
                emit(RoutedSdlHostEvent::Window { window_id, event })
            })
        };
    }
}

unsafe fn event_window_id(event: &sdl::SDL_Event) -> Option<SdlWindowId> {
    let window_id = match event.event_type() {
        sdl::SDL_EVENT_MOUSE_MOTION => unsafe { event.motion.windowID },
        sdl::SDL_EVENT_MOUSE_BUTTON_DOWN | sdl::SDL_EVENT_MOUSE_BUTTON_UP => unsafe {
            event.button.windowID
        },
        sdl::SDL_EVENT_MOUSE_WHEEL => unsafe { event.wheel.windowID },
        sdl::SDL_EVENT_KEY_DOWN | sdl::SDL_EVENT_KEY_UP => unsafe { event.key.windowID },
        sdl::SDL_EVENT_TEXT_INPUT => unsafe { event.text.windowID },
        sdl::SDL_EVENT_TEXT_EDITING => unsafe { event.edit.windowID },
        sdl::SDL_EVENT_DROP_BEGIN
        | sdl::SDL_EVENT_DROP_FILE
        | sdl::SDL_EVENT_DROP_POSITION
        | sdl::SDL_EVENT_DROP_COMPLETE => unsafe { event.drop.windowID },
        sdl::SDL_EVENT_WINDOW_MOUSE_LEAVE
        | sdl::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED
        | sdl::SDL_EVENT_WINDOW_FOCUS_GAINED
        | sdl::SDL_EVENT_WINDOW_FOCUS_LOST
        | sdl::SDL_EVENT_WINDOW_CLOSE_REQUESTED => unsafe { event.window.windowID },
        _ => return None,
    };
    (window_id != 0).then_some(window_id)
}

fn copy_text(text: *const std::ffi::c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn mouse_button(button: u8) -> Option<gpui::MouseButton> {
    match i32::from(button) {
        sdl::SDL_BUTTON_LEFT => Some(gpui::MouseButton::Left),
        sdl::SDL_BUTTON_RIGHT => Some(gpui::MouseButton::Right),
        sdl::SDL_BUTTON_MIDDLE => Some(gpui::MouseButton::Middle),
        sdl::SDL_BUTTON_X1 => Some(gpui::MouseButton::Navigate(gpui::NavigationDirection::Back)),
        sdl::SDL_BUTTON_X2 => Some(gpui::MouseButton::Navigate(
            gpui::NavigationDirection::Forward,
        )),
        _ => None,
    }
}

fn pressed_button(state: sdl::SDL_MouseButtonFlags) -> Option<gpui::MouseButton> {
    let priorities = [
        (sdl::SDL_BUTTON_LMASK, gpui::MouseButton::Left),
        (sdl::SDL_BUTTON_RMASK, gpui::MouseButton::Right),
        (sdl::SDL_BUTTON_MMASK, gpui::MouseButton::Middle),
        (
            sdl::SDL_BUTTON_X1MASK,
            gpui::MouseButton::Navigate(gpui::NavigationDirection::Back),
        ),
        (
            sdl::SDL_BUTTON_X2MASK,
            gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward),
        ),
    ];
    priorities
        .into_iter()
        .find_map(|(mask, button)| (state.0 & mask.0 != 0).then_some(button))
}

fn validate_viewport(viewport: Viewport) -> anyhow::Result<()> {
    anyhow::ensure!(
        viewport.origin_x.is_finite() && viewport.origin_y.is_finite(),
        "viewport origin must be finite"
    );
    anyhow::ensure!(
        viewport.scale.is_finite() && viewport.scale > 0.0,
        "viewport scale must be positive and finite"
    );
    Ok(())
}
