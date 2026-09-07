use gpui_sdl::{
    RoutedSdlHostEvent, SdlHostEvent, SdlInputAdapter, SdlWindowRouter, TextEditing, Viewport,
};
use sdl3_sys::everything as sdl;

#[test]
fn adapt_into_emits_without_owning_the_output_storage() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let raw = motion(sdl::SDL_WindowID(7), 12.0, 18.0);
    let mut output = Vec::new();

    unsafe { adapter.adapt_into(&raw, |event| output.push(event)) };

    assert!(matches!(
        output.as_slice(),
        [SdlHostEvent::Input(gpui::PlatformInput::MouseMove(_))]
    ));
}

#[test]
fn text_editing_converts_sdl_character_offsets_to_utf16() {
    let editing = TextEditing {
        text: "a👋界".to_owned(),
        start: 1,
        length: 1,
    };
    assert_eq!(editing.selection_utf16(), Some(1..3));

    let unset = TextEditing {
        start: -1,
        length: -1,
        ..editing
    };
    assert_eq!(unset.selection_utf16(), None);
}

#[test]
fn router_keeps_adapter_state_and_window_identity_isolated() {
    let first_id = sdl::SDL_WindowID(7);
    let second_id = sdl::SDL_WindowID(9);
    let mut router = SdlWindowRouter::new();
    router
        .register_window(first_id, Viewport::default())
        .unwrap();
    router
        .register_window(
            second_id,
            Viewport {
                origin_x: 10.0,
                origin_y: 20.0,
                scale: 2.0,
            },
        )
        .unwrap();

    let first = unsafe { router.adapt(&motion(first_id, 12.0, 18.0)) };
    let second = unsafe { router.adapt(&motion(second_id, 12.0, 18.0)) };
    let RoutedSdlHostEvent::Window {
        window_id,
        event: SdlHostEvent::Input(gpui::PlatformInput::MouseMove(first)),
    } = &first[0]
    else {
        panic!("first event was not routed")
    };
    assert!(*window_id == first_id);
    let RoutedSdlHostEvent::Window {
        window_id,
        event: SdlHostEvent::Input(gpui::PlatformInput::MouseMove(second)),
    } = &second[0]
    else {
        panic!("second event was not routed")
    };
    assert!(*window_id == second_id);
    assert_eq!(first.position, gpui::point(gpui::px(12.0), gpui::px(18.0)));
    assert_eq!(second.position, gpui::point(gpui::px(1.0), gpui::px(-1.0)));

    let close = window_event(sdl::SDL_EVENT_WINDOW_CLOSE_REQUESTED, second_id);
    let close = unsafe { router.adapt(&close) };
    let [RoutedSdlHostEvent::CloseRequested { window_id }] = close.as_slice() else {
        panic!("close event was not routed")
    };
    assert!(*window_id == second_id);
}

fn motion(window_id: sdl::SDL_WindowID, x: f32, y: f32) -> sdl::SDL_Event {
    sdl::SDL_Event {
        motion: sdl::SDL_MouseMotionEvent {
            r#type: sdl::SDL_EVENT_MOUSE_MOTION,
            windowID: window_id,
            x,
            y,
            ..Default::default()
        },
    }
}

fn window_event(event_type: sdl::SDL_EventType, window_id: sdl::SDL_WindowID) -> sdl::SDL_Event {
    sdl::SDL_Event {
        window: sdl::SDL_WindowEvent {
            r#type: event_type,
            windowID: window_id,
            ..Default::default()
        },
    }
}
