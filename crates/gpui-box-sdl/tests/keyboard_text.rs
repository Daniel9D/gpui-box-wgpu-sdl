use std::ffi::CString;

use gpui_sdl::{SdlHostEvent, SdlInputAdapter, TextEditing, Viewport};
use sdl3_sys::everything as sdl;

#[test]
fn key_down_emits_modifier_change_before_repeat_key() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let modifiers = sdl::SDL_Keymod(sdl::SDL_KMOD_CTRL.0 | sdl::SDL_KMOD_SHIFT.0);
    let raw = key_event(sdl::SDL_EVENT_KEY_DOWN, sdl::SDLK_C, modifiers, true);

    let events = unsafe { adapter.adapt(&raw) };
    let SdlHostEvent::Input(gpui::PlatformInput::ModifiersChanged(event)) = &events[0] else {
        panic!("expected modifiers")
    };
    assert!(event.control && event.shift);
    assert!(!event.platform);
    let SdlHostEvent::Input(gpui::PlatformInput::KeyDown(event)) = &events[1] else {
        panic!("expected key down")
    };
    assert_eq!(event.keystroke.key, "c");
    assert_eq!(event.keystroke.key_char, None);
    assert!(event.is_held);
}

#[test]
fn key_up_keeps_name_and_unchanged_modifiers_are_not_reemitted() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let down = key_event(
        sdl::SDL_EVENT_KEY_DOWN,
        sdl::SDLK_C,
        sdl::SDL_KMOD_CTRL,
        false,
    );
    assert_eq!(unsafe { adapter.adapt(&down) }.len(), 2);

    let up = key_event(
        sdl::SDL_EVENT_KEY_UP,
        sdl::SDLK_C,
        sdl::SDL_KMOD_CTRL,
        false,
    );
    let events = unsafe { adapter.adapt(&up) };
    assert_eq!(events.len(), 1);
    let SdlHostEvent::Input(gpui::PlatformInput::KeyUp(event)) = &events[0] else {
        panic!("expected key up")
    };
    assert_eq!(event.keystroke.key, "c");
    assert_eq!(event.keystroke.key_char, None);
}

#[test]
fn modifiers_preserve_platform_alt_caps_and_never_invent_function() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let raw_modifiers = sdl::SDL_Keymod(
        sdl::SDL_KMOD_GUI.0 | sdl::SDL_KMOD_ALT.0 | sdl::SDL_KMOD_CAPS.0 | sdl::SDL_KMOD_MODE.0,
    );
    let raw = key_event(sdl::SDL_EVENT_KEY_DOWN, sdl::SDLK_A, raw_modifiers, false);

    let events = unsafe { adapter.adapt(&raw) };
    let SdlHostEvent::Input(gpui::PlatformInput::ModifiersChanged(event)) = &events[0] else {
        panic!("expected modifiers")
    };
    assert!(event.platform && event.alt && event.capslock.on);
    assert!(!event.control && !event.shift && !event.function);
}

#[test]
fn named_printable_and_keypad_keys_are_normalized() {
    let cases = [
        (sdl::SDLK_LEFT, "left"),
        (sdl::SDLK_RETURN, "enter"),
        (sdl::SDLK_DELETE, "delete"),
        (sdl::SDLK_F12, "f12"),
        (sdl::SDLK_KP_7, "7"),
        (sdl::SDLK_A, "a"),
        (sdl::SDLK_0, "0"),
        (sdl::SDLK_SLASH, "/"),
        (sdl::SDLK_SEMICOLON, ";"),
    ];

    for (raw_key, expected) in cases {
        let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
        let raw = key_event(sdl::SDL_EVENT_KEY_DOWN, raw_key, sdl::SDL_KMOD_NONE, false);
        let events = unsafe { adapter.adapt(&raw) };
        let SdlHostEvent::Input(gpui::PlatformInput::KeyDown(event)) = &events[0] else {
            panic!("expected key down for {expected}")
        };
        assert_eq!(event.keystroke.key, expected);
    }
}

#[test]
fn extended_function_and_system_keys_are_named() {
    assert_key_names(&[
        (sdl::SDLK_F13, "f13"),
        (sdl::SDLK_F24, "f24"),
        (sdl::SDLK_PRINTSCREEN, "printscreen"),
        (sdl::SDLK_SCROLLLOCK, "scrolllock"),
        (sdl::SDLK_PAUSE, "pause"),
        (sdl::SDLK_NUMLOCKCLEAR, "numlock"),
        (sdl::SDLK_CAPSLOCK, "capslock"),
        (sdl::SDLK_APPLICATION, "menu"),
        (sdl::SDLK_MENU, "menu"),
        (sdl::SDLK_POWER, "power"),
        (sdl::SDLK_HELP, "help"),
        (sdl::SDLK_SYSREQ, "sysreq"),
        (sdl::SDLK_CANCEL, "cancel"),
        (sdl::SDLK_CLEAR, "clear"),
        (sdl::SDLK_RETURN2, "enter"),
        (sdl::SDLK_LEFT_TAB, "tab"),
        (sdl::SDLK_MULTI_KEY_COMPOSE, "compose"),
    ]);
}

#[test]
fn editing_media_and_browser_keys_are_named() {
    assert_key_names(&[
        (sdl::SDLK_EXECUTE, "execute"),
        (sdl::SDLK_SELECT, "select"),
        (sdl::SDLK_STOP, "stop"),
        (sdl::SDLK_AGAIN, "again"),
        (sdl::SDLK_UNDO, "undo"),
        (sdl::SDLK_CUT, "cut"),
        (sdl::SDLK_COPY, "copy"),
        (sdl::SDLK_PASTE, "paste"),
        (sdl::SDLK_FIND, "find"),
        (sdl::SDLK_MUTE, "mute"),
        (sdl::SDLK_VOLUMEUP, "volumeup"),
        (sdl::SDLK_VOLUMEDOWN, "volumedown"),
        (sdl::SDLK_MEDIA_PLAY, "mediaplay"),
        (sdl::SDLK_MEDIA_PLAY_PAUSE, "mediaplaypause"),
        (sdl::SDLK_MEDIA_NEXT_TRACK, "medianexttrack"),
        (sdl::SDLK_AC_NEW, "new"),
        (sdl::SDLK_AC_OPEN, "open"),
        (sdl::SDLK_AC_SAVE, "save"),
        (sdl::SDLK_AC_BACK, "back"),
        (sdl::SDLK_AC_FORWARD, "forward"),
        (sdl::SDLK_AC_REFRESH, "refresh"),
        (sdl::SDLK_AC_BOOKMARKS, "bookmarks"),
    ]);
}

#[test]
fn extended_keypad_keys_are_normalized() {
    assert_key_names(&[
        (sdl::SDLK_KP_PERIOD, "."),
        (sdl::SDLK_KP_EQUALS, "="),
        (sdl::SDLK_KP_COMMA, ","),
        (sdl::SDLK_KP_00, "00"),
        (sdl::SDLK_KP_000, "000"),
        (sdl::SDLK_KP_LEFTPAREN, "("),
        (sdl::SDLK_KP_RIGHTPAREN, ")"),
        (sdl::SDLK_KP_LEFTBRACE, "{"),
        (sdl::SDLK_KP_RIGHTBRACE, "}"),
        (sdl::SDLK_KP_TAB, "tab"),
        (sdl::SDLK_KP_BACKSPACE, "backspace"),
        (sdl::SDLK_KP_A, "a"),
        (sdl::SDLK_KP_F, "f"),
        (sdl::SDLK_KP_XOR, "xor"),
        (sdl::SDLK_KP_POWER, "^"),
        (sdl::SDLK_KP_PERCENT, "%"),
        (sdl::SDLK_KP_DBLAMPERSAND, "&&"),
        (sdl::SDLK_KP_DBLVERTICALBAR, "||"),
        (sdl::SDLK_KP_MEMSTORE, "memstore"),
        (sdl::SDLK_KP_MEMDIVIDE, "memdivide"),
        (sdl::SDLK_KP_PLUSMINUS, "+-"),
        (sdl::SDLK_KP_CLEARENTRY, "clearentry"),
        (sdl::SDLK_KP_HEXADECIMAL, "hexadecimal"),
    ]);
}

#[test]
fn printable_unicode_keycodes_are_lowercased_without_stealing_text_input() {
    assert_key_names(&[
        (sdl::SDL_Keycode('É' as u32), "é"),
        (sdl::SDL_Keycode('Ж' as u32), "ж"),
        (sdl::SDLK_PLUSMINUS, "±"),
    ]);
}

#[test]
fn unknown_key_only_reports_a_real_modifier_change() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let raw = key_event(
        sdl::SDL_EVENT_KEY_DOWN,
        sdl::SDL_Keycode(u32::MAX),
        sdl::SDL_KMOD_SHIFT,
        false,
    );

    let events = unsafe { adapter.adapt(&raw) };
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        SdlHostEvent::Input(gpui::PlatformInput::ModifiersChanged(_))
    ));
}

#[test]
fn text_input_owns_utf8_and_ignores_null_or_empty_text() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let text = CString::new("olá").unwrap();

    let events = unsafe { adapter.adapt(&text_input(text.as_ptr())) };
    let SdlHostEvent::TextInput(value) = &events[0] else {
        panic!("expected text")
    };
    assert_eq!(value, "olá");
    assert!(
        unsafe { adapter.adapt(&text_input(std::ptr::null())) }.is_empty(),
        "null SDL text pointer must be ignored"
    );
    let empty = CString::new("").unwrap();
    assert!(unsafe { adapter.adapt(&text_input(empty.as_ptr())) }.is_empty());
}

#[test]
fn ime_preedit_remains_distinct_from_committed_text() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let text = CString::new("ção").unwrap();
    let raw = sdl::SDL_Event {
        edit: sdl::SDL_TextEditingEvent {
            r#type: sdl::SDL_EVENT_TEXT_EDITING,
            text: text.as_ptr(),
            start: 1,
            length: 2,
            ..Default::default()
        },
    };

    let events = unsafe { adapter.adapt(&raw) };
    let SdlHostEvent::TextEditing(event) = &events[0] else {
        panic!("expected IME preedit")
    };
    assert_eq!(
        event,
        &TextEditing {
            text: "ção".into(),
            start: 1,
            length: 2,
        }
    );
}

fn key_event(
    event_type: sdl::SDL_EventType,
    key: sdl::SDL_Keycode,
    modifiers: sdl::SDL_Keymod,
    repeat: bool,
) -> sdl::SDL_Event {
    sdl::SDL_Event {
        key: sdl::SDL_KeyboardEvent {
            r#type: event_type,
            key,
            r#mod: modifiers,
            down: event_type == sdl::SDL_EVENT_KEY_DOWN,
            repeat,
            ..Default::default()
        },
    }
}

fn text_input(text: *const std::ffi::c_char) -> sdl::SDL_Event {
    sdl::SDL_Event {
        text: sdl::SDL_TextInputEvent {
            r#type: sdl::SDL_EVENT_TEXT_INPUT,
            text,
            ..Default::default()
        },
    }
}

fn assert_key_names(cases: &[(sdl::SDL_Keycode, &str)]) {
    for &(raw_key, expected) in cases {
        let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
        let raw = key_event(sdl::SDL_EVENT_KEY_DOWN, raw_key, sdl::SDL_KMOD_NONE, false);
        let events = unsafe { adapter.adapt(&raw) };
        let SdlHostEvent::Input(gpui::PlatformInput::KeyDown(event)) = &events[0] else {
            panic!("expected key down for {expected}")
        };
        assert_eq!(event.keystroke.key, expected, "SDL keycode {}", raw_key.0);
        assert_eq!(event.keystroke.key_char, None);
    }
}
