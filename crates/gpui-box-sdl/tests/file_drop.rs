use std::{ffi::CString, path::PathBuf};

use gpui_sdl::{SdlInputAdapter, Viewport};
use sdl3_sys::everything as sdl;

#[test]
fn complete_drop_delivers_all_files_at_the_transformed_position() {
    let mut adapter = SdlInputAdapter::new(Viewport {
        origin_x: 10.0,
        origin_y: 20.0,
        scale: 2.0,
    })
    .unwrap();
    let first = CString::new(r"C:\media\first.mp4").unwrap();
    let second = CString::new(r"C:\media\second.srt").unwrap();

    assert!(
        unsafe { adapter.adapt(&drop_event(sdl::SDL_EVENT_DROP_BEGIN, None, 0.0, 0.0)) }.is_empty()
    );
    assert!(
        unsafe {
            adapter.adapt(&drop_event(
                sdl::SDL_EVENT_DROP_FILE,
                Some(&first),
                50.0,
                80.0,
            ))
        }
        .is_empty()
    );
    assert!(
        unsafe {
            adapter.adapt(&drop_event(
                sdl::SDL_EVENT_DROP_FILE,
                Some(&second),
                50.0,
                80.0,
            ))
        }
        .is_empty()
    );

    let events =
        unsafe { adapter.adapt(&drop_event(sdl::SDL_EVENT_DROP_COMPLETE, None, 50.0, 80.0)) };
    assert_eq!(events.len(), 3);

    let gpui_sdl::SdlHostEvent::Input(gpui::PlatformInput::FileDrop(
        gpui::FileDropEvent::Entered { position, paths },
    )) = &events[0]
    else {
        panic!("expected file-drop enter")
    };
    assert_eq!(*position, gpui::point(gpui::px(20.0), gpui::px(30.0)));
    assert_eq!(
        paths.paths(),
        &[
            PathBuf::from(r"C:\media\first.mp4"),
            PathBuf::from(r"C:\media\second.srt"),
        ]
    );
    assert!(matches!(
        &events[1],
        gpui_sdl::SdlHostEvent::Input(gpui::PlatformInput::FileDrop(
            gpui::FileDropEvent::Submit { position }
        )) if *position == gpui::point(gpui::px(20.0), gpui::px(30.0))
    ));
    assert!(matches!(
        &events[2],
        gpui_sdl::SdlHostEvent::Input(gpui::PlatformInput::FileDrop(gpui::FileDropEvent::Ended))
    ));
}

#[test]
fn empty_or_cancelled_drop_does_not_emit_gpui_input() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let invalid = CString::new("").unwrap();

    assert!(
        unsafe {
            adapter.adapt(&drop_event(
                sdl::SDL_EVENT_DROP_FILE,
                Some(&invalid),
                0.0,
                0.0,
            ))
        }
        .is_empty()
    );
    assert!(
        unsafe { adapter.adapt(&drop_event(sdl::SDL_EVENT_DROP_COMPLETE, None, 0.0, 0.0,)) }
            .is_empty()
    );
}

fn drop_event(
    event_type: sdl::SDL_EventType,
    data: Option<&CString>,
    x: f32,
    y: f32,
) -> sdl::SDL_Event {
    sdl::SDL_Event {
        drop: sdl::SDL_DropEvent {
            r#type: event_type,
            data: data.map_or(std::ptr::null(), |value| value.as_ptr()),
            x,
            y,
            ..Default::default()
        },
    }
}
