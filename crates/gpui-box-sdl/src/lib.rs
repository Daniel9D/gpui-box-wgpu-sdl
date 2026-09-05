#![doc = include_str!("../README.md")]

mod adapter;
mod keyboard;

pub use adapter::{SdlHostEvent, SdlInputAdapter, TextEditing, Viewport};
