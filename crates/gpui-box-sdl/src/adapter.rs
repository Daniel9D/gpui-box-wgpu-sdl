use std::ffi::CStr;

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

    /// Translates one raw SDL event into zero or more host actions.
    ///
    /// # Safety
    ///
    /// `event` must have been populated by SDL, and any pointers carried by it
    /// must remain valid for this call.
    pub unsafe fn adapt(&mut self, event: &sdl::SDL_Event) -> Vec<SdlHostEvent> {
        match event.event_type() {
            sdl::SDL_EVENT_MOUSE_MOTION => vec![self.adapt_motion(unsafe { event.motion })],
            sdl::SDL_EVENT_MOUSE_BUTTON_DOWN | sdl::SDL_EVENT_MOUSE_BUTTON_UP => self
                .adapt_button(unsafe { event.button })
                .into_iter()
                .collect(),
            sdl::SDL_EVENT_MOUSE_WHEEL => vec![self.adapt_wheel(unsafe { event.wheel })],
            sdl::SDL_EVENT_WINDOW_MOUSE_LEAVE => vec![self.adapt_mouse_exit()],
            sdl::SDL_EVENT_KEY_DOWN | sdl::SDL_EVENT_KEY_UP => {
                self.adapt_keyboard(unsafe { event.key })
            }
            sdl::SDL_EVENT_TEXT_INPUT => self.adapt_text_input(unsafe { event.text }),
            sdl::SDL_EVENT_TEXT_EDITING => self.adapt_text_editing(unsafe { event.edit }),
            sdl::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED => self
                .adapt_resize(unsafe { event.window })
                .into_iter()
                .collect(),
            sdl::SDL_EVENT_WINDOW_FOCUS_GAINED => vec![SdlHostEvent::FocusChanged(true)],
            sdl::SDL_EVENT_WINDOW_FOCUS_LOST => self.adapt_focus_loss(),
            sdl::SDL_EVENT_QUIT => vec![SdlHostEvent::Quit],
            _ => Vec::new(),
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

    fn adapt_keyboard(&mut self, event: sdl::SDL_KeyboardEvent) -> Vec<SdlHostEvent> {
        let (modifiers, capslock) = keyboard::modifiers(event.r#mod);
        let mut output = self.update_modifiers(modifiers, capslock);
        let Some(key) = keyboard::key_name(event.key) else {
            return output;
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
        output.push(SdlHostEvent::Input(input));
        output
    }

    fn update_modifiers(
        &mut self,
        modifiers: gpui::Modifiers,
        capslock: gpui::Capslock,
    ) -> Vec<SdlHostEvent> {
        if self.modifiers == modifiers && self.capslock == capslock {
            return Vec::new();
        }
        self.modifiers = modifiers;
        self.capslock = capslock;
        vec![SdlHostEvent::Input(gpui::PlatformInput::ModifiersChanged(
            gpui::ModifiersChangedEvent {
                modifiers,
                capslock,
            },
        ))]
    }

    fn adapt_text_input(&self, event: sdl::SDL_TextInputEvent) -> Vec<SdlHostEvent> {
        let Some(text) = copy_text(event.text) else {
            return Vec::new();
        };
        if text.is_empty() {
            Vec::new()
        } else {
            vec![SdlHostEvent::TextInput(text)]
        }
    }

    fn adapt_text_editing(&self, event: sdl::SDL_TextEditingEvent) -> Vec<SdlHostEvent> {
        let Some(text) = copy_text(event.text) else {
            return Vec::new();
        };
        vec![SdlHostEvent::TextEditing(TextEditing {
            text,
            start: event.start,
            length: event.length,
        })]
    }

    fn adapt_resize(&self, event: sdl::SDL_WindowEvent) -> Option<SdlHostEvent> {
        let width = u32::try_from(event.data1).ok().filter(|value| *value > 0)?;
        let height = u32::try_from(event.data2).ok().filter(|value| *value > 0)?;
        Some(SdlHostEvent::WindowResized { width, height })
    }

    fn adapt_focus_loss(&mut self) -> Vec<SdlHostEvent> {
        let mut output =
            self.update_modifiers(gpui::Modifiers::default(), gpui::Capslock::default());
        self.pressed_button = None;
        output.push(SdlHostEvent::FocusChanged(false));
        output
    }
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
