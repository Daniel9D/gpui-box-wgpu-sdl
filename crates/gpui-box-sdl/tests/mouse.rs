use gpui_sdl::{SdlHostEvent, SdlInputAdapter, Viewport};
use sdl3_sys::everything as sdl;

#[test]
fn motion_applies_viewport_and_reports_pressed_left_button() {
    let mut adapter = SdlInputAdapter::new(Viewport {
        origin_x: 10.0,
        origin_y: 4.0,
        scale: 2.0,
    })
    .unwrap();

    let events = unsafe { adapter.adapt(&motion(30.0, 20.0, sdl::SDL_BUTTON_LMASK)) };
    let SdlHostEvent::Input(gpui::PlatformInput::MouseMove(event)) = &events[0] else {
        panic!("expected mouse move")
    };
    assert_eq!(event.position, gpui::point(gpui::px(10.0), gpui::px(8.0)));
    assert_eq!(event.pressed_button, Some(gpui::MouseButton::Left));
}

#[test]
fn motion_uses_stable_button_priority() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let state = sdl::SDL_MouseButtonFlags(
        sdl::SDL_BUTTON_RMASK.0 | sdl::SDL_BUTTON_MMASK.0 | sdl::SDL_BUTTON_X1MASK.0,
    );

    let events = unsafe { adapter.adapt(&motion(1.0, 2.0, state)) };
    let SdlHostEvent::Input(gpui::PlatformInput::MouseMove(event)) = &events[0] else {
        panic!("expected mouse move")
    };
    assert_eq!(event.pressed_button, Some(gpui::MouseButton::Right));
}

#[test]
fn buttons_map_click_count_and_release_state() {
    let cases = [
        (sdl::SDL_BUTTON_LEFT as u8, gpui::MouseButton::Left),
        (sdl::SDL_BUTTON_RIGHT as u8, gpui::MouseButton::Right),
        (sdl::SDL_BUTTON_MIDDLE as u8, gpui::MouseButton::Middle),
        (
            sdl::SDL_BUTTON_X1 as u8,
            gpui::MouseButton::Navigate(gpui::NavigationDirection::Back),
        ),
        (
            sdl::SDL_BUTTON_X2 as u8,
            gpui::MouseButton::Navigate(gpui::NavigationDirection::Forward),
        ),
    ];

    for (raw, expected) in cases {
        let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
        let down = unsafe { adapter.adapt(&button(sdl::SDL_EVENT_MOUSE_BUTTON_DOWN, raw, 2)) };
        let SdlHostEvent::Input(gpui::PlatformInput::MouseDown(event)) = &down[0] else {
            panic!("expected mouse down")
        };
        assert_eq!(event.button, expected);
        assert_eq!(event.click_count, 2);
        assert_eq!(event.position, gpui::point(gpui::px(18.0), gpui::px(9.0)));

        let up = unsafe { adapter.adapt(&button(sdl::SDL_EVENT_MOUSE_BUTTON_UP, raw, 2)) };
        let SdlHostEvent::Input(gpui::PlatformInput::MouseUp(event)) = &up[0] else {
            panic!("expected mouse up")
        };
        assert_eq!(event.button, expected);

        let leave = unsafe { adapter.adapt(&window(sdl::SDL_EVENT_WINDOW_MOUSE_LEAVE)) };
        let SdlHostEvent::Input(gpui::PlatformInput::MouseExited(event)) = &leave[0] else {
            panic!("expected mouse exit")
        };
        assert_eq!(event.pressed_button, None);
    }
}

#[test]
fn wheel_uses_event_position_and_honors_flipped_direction() {
    let mut adapter = SdlInputAdapter::new(Viewport {
        origin_x: 10.0,
        origin_y: 6.0,
        scale: 2.0,
    })
    .unwrap();

    let normal = unsafe { adapter.adapt(&wheel(sdl::SDL_MOUSEWHEEL_NORMAL)) };
    assert_wheel(&normal[0], 2.0, -3.0);
    let flipped = unsafe { adapter.adapt(&wheel(sdl::SDL_MOUSEWHEEL_FLIPPED)) };
    assert_wheel(&flipped[0], -2.0, 3.0);
}

#[test]
fn unsupported_button_and_event_are_ignored() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();

    assert!(unsafe { adapter.adapt(&button(sdl::SDL_EVENT_MOUSE_BUTTON_DOWN, 99, 1)) }.is_empty());
    assert!(unsafe { adapter.adapt(&sdl::SDL_Event { r#type: 0 }) }.is_empty());
}

fn assert_wheel(output: &SdlHostEvent, x: f32, y: f32) {
    let SdlHostEvent::Input(gpui::PlatformInput::ScrollWheel(event)) = output else {
        panic!("expected wheel")
    };
    assert_eq!(event.position, gpui::point(gpui::px(10.0), gpui::px(7.0)));
    let gpui::ScrollDelta::Lines(delta) = event.delta else {
        panic!("expected line delta")
    };
    assert_eq!(delta, gpui::point(x, y));
    assert_eq!(event.touch_phase, gpui::TouchPhase::Moved);
}

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

fn button(event_type: sdl::SDL_EventType, button: u8, clicks: u8) -> sdl::SDL_Event {
    sdl::SDL_Event {
        button: sdl::SDL_MouseButtonEvent {
            r#type: event_type,
            button,
            down: event_type == sdl::SDL_EVENT_MOUSE_BUTTON_DOWN,
            clicks,
            x: 18.0,
            y: 9.0,
            ..Default::default()
        },
    }
}

fn wheel(direction: sdl::SDL_MouseWheelDirection) -> sdl::SDL_Event {
    sdl::SDL_Event {
        wheel: sdl::SDL_MouseWheelEvent {
            r#type: sdl::SDL_EVENT_MOUSE_WHEEL,
            x: 2.0,
            y: -3.0,
            direction,
            mouse_x: 30.0,
            mouse_y: 20.0,
            ..Default::default()
        },
    }
}

fn window(event_type: sdl::SDL_EventType) -> sdl::SDL_Event {
    sdl::SDL_Event {
        window: sdl::SDL_WindowEvent {
            r#type: event_type,
            ..Default::default()
        },
    }
}
