//! Embedded GPUI Kit for GPUI Box. The engine supplies the platform and renderer.
//! Call init with the host App before constructing component::Root.
/// Defines unit actions without requiring consumers to depend on GPUI under the
/// crate name `gpui`.
///
/// GPUI's original macro spells its derive as `gpui::Action`, which does not
/// resolve when GPUI is consumed solely through this facade.
#[macro_export]
macro_rules! actions {
    ($namespace:path, [ $( $(#[$attr:meta])* $name:ident),* $(,)? ]) => {
        $(
            #[derive(
                ::std::clone::Clone,
                ::std::cmp::PartialEq,
                ::std::default::Default,
                ::std::fmt::Debug,
                $crate::Action
            )]
            #[action(namespace = $namespace)]
            $(#[$attr])*
            pub struct $name;
        )*
    };
    ([ $( $(#[$attr:meta])* $name:ident),* $(,)? ]) => {
        $(
            #[derive(
                ::std::clone::Clone,
                ::std::cmp::PartialEq,
                ::std::default::Default,
                ::std::fmt::Debug,
                $crate::Action
            )]
            $(#[$attr])*
            pub struct $name;
        )*
    };
}

// Everything in GPUI itself, so `use gpui_kit::*;` is enough to get started.
// With the `test-support` feature the glob also carries GPUI's `test`
// attribute, so a test module imports explicitly (or adds
// `use core::prelude::v1::test;`) to keep the built-in `#[test]`.
pub use ::gpui::*;

// The crate name, so code that keeps `gpui::…` paths still resolves after
// `use gpui_kit::*;`. `gpui_kit::*` is the documented way.
#[doc(hidden)]
pub use ::gpui;

pub use ::gpui_base as base;

/// The styled component library.
#[cfg(feature = "component")]
pub use ::gpui_component as component;
#[cfg(feature = "assets")]
pub use ::gpui_kit_assets as assets;

/// Initializes every enabled layer. Call it once, before using anything else.
///
/// With the `component` feature (on by default) this is
/// `gpui_component::init`, which also initializes `gpui-base`; otherwise it
/// is `gpui_base::init`.
pub fn init(cx: &mut App) {
    #[cfg(feature = "component")]
    gpui_component::init(cx);
    #[cfg(not(feature = "component"))]
    gpui_base::init(cx);
}
