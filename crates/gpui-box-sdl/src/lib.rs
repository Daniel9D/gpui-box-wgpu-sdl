#![doc = include_str!("../README.md")]

mod adapter;
mod keyboard;
mod platform;

pub use adapter::{SdlHostEvent, SdlInputAdapter, TextEditing, Viewport};
pub use platform::{
    SdlCursor, SdlPlatformBridge, clipboard_text, set_clipboard_text, system_cursor,
};
