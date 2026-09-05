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
