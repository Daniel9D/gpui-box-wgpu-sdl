use gpui_sdl::{SdlHostEvent, SdlInputAdapter, Viewport};
use sdl3_sys::everything as sdl;

#[test]
fn physical_pixel_resize_is_exposed_to_the_host() {
    let mut adapter = adapter();
    let events = unsafe {
        adapter.adapt(&window_event(
            sdl::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED,
            1920,
            1080,
        ))
    };

    assert!(matches!(
        events.as_slice(),
        [SdlHostEvent::WindowResized {
            width: 1920,
            height: 1080
        }]
    ));
}

#[test]
fn invalid_and_logical_sizes_are_ignored() {
    let mut adapter = adapter();

    for (width, height) in [(0, 1080), (-1, 1080), (1920, 0), (1920, -1)] {
        let raw = window_event(sdl::SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED, width, height);
        assert!(unsafe { adapter.adapt(&raw) }.is_empty());
    }

    let logical = window_event(sdl::SDL_EVENT_WINDOW_RESIZED, 960, 540);
    assert!(unsafe { adapter.adapt(&logical) }.is_empty());
}

#[test]
fn focus_events_are_exposed_without_inventing_state() {
    let mut adapter = adapter();

    let gained = window_event(sdl::SDL_EVENT_WINDOW_FOCUS_GAINED, 0, 0);
    assert!(matches!(
        unsafe { adapter.adapt(&gained) }.as_slice(),
        [SdlHostEvent::FocusChanged(true)]
    ));

    let lost = window_event(sdl::SDL_EVENT_WINDOW_FOCUS_LOST, 0, 0);
    assert!(matches!(
        unsafe { adapter.adapt(&lost) }.as_slice(),
        [SdlHostEvent::FocusChanged(false)]
    ));
}

#[test]
fn focus_loss_releases_retained_input_state_before_notifying_host() {
    let mut adapter = adapter();
    let key = sdl::SDL_Event {
        key: sdl::SDL_KeyboardEvent {
            r#type: sdl::SDL_EVENT_KEY_DOWN,
            key: sdl::SDLK_LCTRL,
            r#mod: sdl::SDL_KMOD_CTRL,
            down: true,
            ..Default::default()
        },
    };
    unsafe { adapter.adapt(&key) };
    let button = sdl::SDL_Event {
        button: sdl::SDL_MouseButtonEvent {
            r#type: sdl::SDL_EVENT_MOUSE_BUTTON_DOWN,
            button: sdl::SDL_BUTTON_LEFT as u8,
            down: true,
            ..Default::default()
        },
    };
    unsafe { adapter.adapt(&button) };

    let lost = window_event(sdl::SDL_EVENT_WINDOW_FOCUS_LOST, 0, 0);
    let events = unsafe { adapter.adapt(&lost) };
    assert_eq!(events.len(), 2);
    let SdlHostEvent::Input(gpui::PlatformInput::ModifiersChanged(modifiers)) = &events[0] else {
        panic!("expected released modifiers before focus loss")
    };
    assert_eq!(modifiers.modifiers, gpui::Modifiers::default());
    assert_eq!(modifiers.capslock, gpui::Capslock::default());
    assert!(matches!(events[1], SdlHostEvent::FocusChanged(false)));

    let leave = window_event(sdl::SDL_EVENT_WINDOW_MOUSE_LEAVE, 0, 0);
    let output = unsafe { adapter.adapt(&leave) };
    let SdlHostEvent::Input(gpui::PlatformInput::MouseExited(exit)) = &output[0] else {
        panic!("expected mouse exit")
    };
    assert_eq!(exit.pressed_button, None);
    assert_eq!(exit.modifiers, gpui::Modifiers::default());
}

#[test]
fn quit_is_exposed_to_the_host() {
    let mut adapter = adapter();
    let raw = sdl::SDL_Event {
        r#type: sdl::SDL_EVENT_QUIT.0,
    };

    assert!(matches!(
        unsafe { adapter.adapt(&raw) }.as_slice(),
        [SdlHostEvent::Quit]
    ));
}

fn adapter() -> SdlInputAdapter {
    SdlInputAdapter::new(Viewport::default()).unwrap()
}

fn window_event(event_type: sdl::SDL_EventType, data1: i32, data2: i32) -> sdl::SDL_Event {
    sdl::SDL_Event {
        window: sdl::SDL_WindowEvent {
            r#type: event_type,
            data1,
            data2,
            ..Default::default()
        },
    }
}
