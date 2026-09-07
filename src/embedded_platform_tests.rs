use std::{borrow::Cow, rc::Rc, sync::Arc};

use gpui::{
    AnyWindowHandle, App, AppContext, Application, AtlasKey, AtlasTile, Bounds, Context,
    DevicePixels, DisplayId, IntoElement, NoopTextSystem, Pixels, PlatformAtlas, PlatformDisplay,
    PlatformWindow, QuitMode, Render, RequestFrameOptions, Size, ThreadedDispatcher, Window,
    WindowBounds, WindowOptions, div, point, px, size,
};

use crate::EmbeddedPlatform;

#[derive(Debug)]
struct EmptyDisplay;

impl PlatformDisplay for EmptyDisplay {
    fn id(&self) -> DisplayId {
        DisplayId::new(1)
    }

    fn uuid(&self) -> anyhow::Result<uuid::Uuid> {
        Ok(uuid::Uuid::nil())
    }

    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0)))
    }
}

#[test]
fn frame_requests_are_routed_only_to_the_selected_window() {
    let dispatcher = Arc::new(ThreadedDispatcher::new());
    let platform = EmbeddedPlatform::new(
        dispatcher,
        Arc::new(NoopTextSystem),
        Arc::new(EmptyAtlas),
        Rc::new(EmptyDisplay),
    );
    let app = Application::new_inaccessible(platform.clone()).run_embedded(|_: &mut App| {});
    let first = open_window(&app, size(px(100.0), px(100.0)));
    let second = open_window(&app, size(px(100.0), px(100.0)));
    let first_called = Rc::new(std::cell::Cell::new(false));
    let second_called = Rc::new(std::cell::Cell::new(false));

    platform.window(first).unwrap().on_request_frame(Box::new({
        let called = first_called.clone();
        move |_| called.set(true)
    }));
    platform.window(second).unwrap().on_request_frame(Box::new({
        let called = second_called.clone();
        move |_| called.set(true)
    }));

    platform
        .request_frame(
            second,
            RequestFrameOptions {
                require_presentation: true,
                force_render: false,
            },
        )
        .unwrap();
    assert!(!first_called.get());
    assert!(second_called.get());
}

#[test]
fn embedded_window_inherits_scale_and_appearance_defaults() {
    let platform = EmbeddedPlatform::new_with_defaults(
        Arc::new(ThreadedDispatcher::new()),
        Arc::new(NoopTextSystem),
        Arc::new(EmptyAtlas),
        Rc::new(EmptyDisplay),
        2.0,
        gpui::WindowAppearance::Dark,
    );
    let app = Application::new_inaccessible(platform.clone()).run_embedded(|_: &mut App| {});
    let window = open_window(&app, size(px(100.0), px(80.0)));

    assert_eq!(platform.window(window).unwrap().scale_factor(), 2.0);
    assert_eq!(
        platform.window(window).unwrap().appearance(),
        gpui::WindowAppearance::Dark
    );
}

#[test]
fn embedded_window_forwards_completed_luminance_probes() {
    let platform = EmbeddedPlatform::new(
        Arc::new(ThreadedDispatcher::new()),
        Arc::new(NoopTextSystem),
        Arc::new(EmptyAtlas),
        Rc::new(EmptyDisplay),
    );
    let app = Application::new_inaccessible(platform.clone()).run_embedded(|_: &mut App| {});
    let window = open_window(&app, size(px(100.0), px(80.0)));
    let calls = Rc::new(std::cell::Cell::new(0));
    platform
        .window(window)
        .unwrap()
        .set_luminance_callback(Box::new({
            let calls = calls.clone();
            move |slot| {
                calls.set(calls.get() + 1);
                (slot == 3).then_some(0.75)
            }
        }));

    assert_eq!(
        platform.window(window).unwrap().backdrop_luminance(3),
        Some(0.75)
    );
    assert_eq!(platform.window(window).unwrap().backdrop_luminance(2), None);
    assert_eq!(calls.get(), 2, "the callback is restored after every query");
}

struct EmptyAtlas;

impl PlatformAtlas for EmptyAtlas {
    fn get_or_insert_with<'a>(
        &self,
        _key: &AtlasKey,
        _build: &mut dyn FnMut() -> anyhow::Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> anyhow::Result<Option<AtlasTile>> {
        Ok(None)
    }

    fn remove(&self, _key: &AtlasKey) {}

    fn contains(&self, _key: &AtlasKey) -> bool {
        false
    }
}

struct EmptyView;

impl Render for EmptyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

fn open_window(app: &gpui::ApplicationHandle, size: Size<Pixels>) -> AnyWindowHandle {
    app.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.0), px(0.0)),
                    size,
                ))),
                ..Default::default()
            },
            |_, cx| cx.new(|_| EmptyView),
        )
        .unwrap()
        .into()
    })
}

#[test]
fn embedded_windows_keep_size_focus_and_close_state_isolated() {
    let dispatcher = Arc::new(ThreadedDispatcher::new());
    let platform = EmbeddedPlatform::new(
        dispatcher.clone(),
        Arc::new(NoopTextSystem),
        Arc::new(EmptyAtlas),
        Rc::new(EmptyDisplay),
    );
    let app = Application::new_inaccessible(platform.clone())
        .with_quit_mode(QuitMode::Explicit)
        .run_embedded(|_: &mut App| {});

    let first = open_window(&app, size(px(320.0), px(180.0)));
    let second = open_window(&app, size(px(640.0), px(360.0)));

    platform
        .resize(first, size(px(400.0), px(225.0)), 1.5)
        .unwrap();
    platform.set_active(first, true).unwrap();
    assert_eq!(
        platform.window(first).unwrap().content_size(),
        size(px(400.0), px(225.0))
    );
    assert_eq!(
        platform.window(second).unwrap().content_size(),
        size(px(640.0), px(360.0))
    );
    assert!(platform.window(first).unwrap().is_active());
    assert!(!platform.window(second).unwrap().is_active());

    platform.set_active(second, true).unwrap();
    assert!(!platform.window(first).unwrap().is_active());
    assert!(platform.window(second).unwrap().is_active());

    platform.request_close(first).unwrap();
    dispatcher.run_ready_main_tasks();
    assert!(platform.window(first).is_none());
    assert!(platform.window(second).is_some());
}
