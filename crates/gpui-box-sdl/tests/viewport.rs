use gpui_sdl::{SdlInputAdapter, Viewport};

#[test]
fn viewport_converts_window_coordinates_to_gpui_coordinates() {
    let adapter = SdlInputAdapter::new(Viewport {
        origin_x: 20.0,
        origin_y: 10.0,
        scale: 2.0,
    })
    .unwrap();

    assert_eq!(
        adapter.map_position(60.0, 34.0),
        gpui::point(gpui::px(20.0), gpui::px(12.0))
    );
}

#[test]
fn invalid_viewport_does_not_replace_the_current_transform() {
    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();

    assert!(
        adapter
            .set_viewport(Viewport {
                scale: 0.0,
                ..Viewport::default()
            })
            .is_err()
    );
    assert!(
        adapter
            .set_viewport(Viewport {
                origin_x: f32::NAN,
                ..Viewport::default()
            })
            .is_err()
    );
    assert_eq!(
        adapter.map_position(12.0, 8.0),
        gpui::point(gpui::px(12.0), gpui::px(8.0))
    );
}
