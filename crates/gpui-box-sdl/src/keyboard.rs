use sdl3_sys::everything as sdl;

pub(crate) fn key_name(key: sdl::SDL_Keycode) -> Option<String> {
    let named = match key {
        sdl::SDLK_LEFT => "left",
        sdl::SDLK_RIGHT => "right",
        sdl::SDLK_UP => "up",
        sdl::SDLK_DOWN => "down",
        sdl::SDLK_HOME => "home",
        sdl::SDLK_END => "end",
        sdl::SDLK_PAGEUP => "pageup",
        sdl::SDLK_PAGEDOWN => "pagedown",
        sdl::SDLK_INSERT => "insert",
        sdl::SDLK_DELETE => "delete",
        sdl::SDLK_ESCAPE => "escape",
        sdl::SDLK_TAB => "tab",
        sdl::SDLK_BACKSPACE => "backspace",
        sdl::SDLK_RETURN | sdl::SDLK_KP_ENTER => "enter",
        sdl::SDLK_SPACE => "space",
        sdl::SDLK_F1 => "f1",
        sdl::SDLK_F2 => "f2",
        sdl::SDLK_F3 => "f3",
        sdl::SDLK_F4 => "f4",
        sdl::SDLK_F5 => "f5",
        sdl::SDLK_F6 => "f6",
        sdl::SDLK_F7 => "f7",
        sdl::SDLK_F8 => "f8",
        sdl::SDLK_F9 => "f9",
        sdl::SDLK_F10 => "f10",
        sdl::SDLK_F11 => "f11",
        sdl::SDLK_F12 => "f12",
        sdl::SDLK_KP_0 => "0",
        sdl::SDLK_KP_1 => "1",
        sdl::SDLK_KP_2 => "2",
        sdl::SDLK_KP_3 => "3",
        sdl::SDLK_KP_4 => "4",
        sdl::SDLK_KP_5 => "5",
        sdl::SDLK_KP_6 => "6",
        sdl::SDLK_KP_7 => "7",
        sdl::SDLK_KP_8 => "8",
        sdl::SDLK_KP_9 => "9",
        sdl::SDLK_KP_PLUS => "+",
        sdl::SDLK_KP_MINUS => "-",
        sdl::SDLK_KP_MULTIPLY => "*",
        sdl::SDLK_KP_DIVIDE => "/",
        sdl::SDLK_KP_DECIMAL => ".",
        sdl::SDLK_LCTRL | sdl::SDLK_RCTRL => "control",
        sdl::SDLK_LALT | sdl::SDLK_RALT => "alt",
        sdl::SDLK_LSHIFT | sdl::SDLK_RSHIFT => "shift",
        sdl::SDLK_LGUI | sdl::SDLK_RGUI => "platform",
        _ => return printable_key(key),
    };
    Some(named.to_owned())
}

pub(crate) fn modifiers(raw: sdl::SDL_Keymod) -> (gpui::Modifiers, gpui::Capslock) {
    (
        gpui::Modifiers {
            control: raw.0 & sdl::SDL_KMOD_CTRL.0 != 0,
            alt: raw.0 & sdl::SDL_KMOD_ALT.0 != 0,
            shift: raw.0 & sdl::SDL_KMOD_SHIFT.0 != 0,
            platform: raw.0 & sdl::SDL_KMOD_GUI.0 != 0,
            function: false,
        },
        gpui::Capslock {
            on: raw.0 & sdl::SDL_KMOD_CAPS.0 != 0,
        },
    )
}

fn printable_key(key: sdl::SDL_Keycode) -> Option<String> {
    let character = char::from_u32(key.0)?;
    if character == ' ' || character.is_ascii_graphic() {
        Some(character.to_ascii_lowercase().to_string())
    } else {
        None
    }
}
