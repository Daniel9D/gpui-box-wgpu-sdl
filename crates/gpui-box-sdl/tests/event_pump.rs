#![cfg(feature = "build-from-source")]

use std::{mem::MaybeUninit, sync::Mutex};

use gpui_sdl::{SdlHostEvent, SdlInputAdapter, Viewport};
use sdl3_sys::everything as sdl;

static SDL_LOCK: Mutex<()> = Mutex::new(());

struct SdlGuard;

impl Drop for SdlGuard {
    fn drop(&mut self) {
        unsafe { sdl::SDL_Quit() };
    }
}

#[test]
fn pushed_sdl_event_reaches_the_adapter_through_the_real_event_pump() {
    let _lock = SDL_LOCK.lock().unwrap();
    assert!(unsafe { sdl::SDL_Init(sdl::SDL_INIT_EVENTS) });
    let _guard = SdlGuard;
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let mut pushed = sdl::SDL_Event {
        motion: sdl::SDL_MouseMotionEvent {
            r#type: sdl::SDL_EVENT_MOUSE_MOTION,
            x: 42.0,
            y: 24.0,
            ..Default::default()
        },
    };

    assert!(unsafe { sdl::SDL_PushEvent(&mut pushed) });
    let mut polled = MaybeUninit::<sdl::SDL_Event>::uninit();
    assert!(unsafe { sdl::SDL_PollEvent(polled.as_mut_ptr()) });
    let polled = unsafe { polled.assume_init() };
    let output = unsafe { adapter.adapt(&polled) };

    let SdlHostEvent::Input(gpui::PlatformInput::MouseMove(event)) = &output[0] else {
        panic!("expected mouse move")
    };
    assert_eq!(event.position, gpui::point(gpui::px(42.0), gpui::px(24.0)));
}
