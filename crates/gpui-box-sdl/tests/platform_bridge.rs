use gpui::CursorStyle;
use gpui_sdl::{set_clipboard_text, system_cursor};
use sdl3_sys::everything as sdl;

#[test]
fn gpui_cursor_styles_use_the_closest_sdl_system_cursor() {
    let cases = [
        (CursorStyle::Arrow, sdl::SDL_SYSTEM_CURSOR_DEFAULT),
        (CursorStyle::IBeam, sdl::SDL_SYSTEM_CURSOR_TEXT),
        (CursorStyle::Crosshair, sdl::SDL_SYSTEM_CURSOR_CROSSHAIR),
        (CursorStyle::PointingHand, sdl::SDL_SYSTEM_CURSOR_POINTER),
        (CursorStyle::ResizeLeft, sdl::SDL_SYSTEM_CURSOR_W_RESIZE),
        (CursorStyle::ResizeRight, sdl::SDL_SYSTEM_CURSOR_E_RESIZE),
        (
            CursorStyle::ResizeLeftRight,
            sdl::SDL_SYSTEM_CURSOR_EW_RESIZE,
        ),
        (CursorStyle::ResizeUp, sdl::SDL_SYSTEM_CURSOR_N_RESIZE),
        (CursorStyle::ResizeDown, sdl::SDL_SYSTEM_CURSOR_S_RESIZE),
        (CursorStyle::ResizeUpDown, sdl::SDL_SYSTEM_CURSOR_NS_RESIZE),
        (
            CursorStyle::ResizeUpLeftDownRight,
            sdl::SDL_SYSTEM_CURSOR_NWSE_RESIZE,
        ),
        (
            CursorStyle::ResizeUpRightDownLeft,
            sdl::SDL_SYSTEM_CURSOR_NESW_RESIZE,
        ),
        (
            CursorStyle::OperationNotAllowed,
            sdl::SDL_SYSTEM_CURSOR_NOT_ALLOWED,
        ),
    ];

    for (style, expected) in cases {
        assert!(system_cursor(style) == expected);
    }
}

#[test]
fn clipboard_rejects_interior_nul_before_calling_sdl() {
    let error = set_clipboard_text("before\0after").unwrap_err();
    assert!(error.to_string().contains("NUL"));
}
