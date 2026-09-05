/// Embed application assets for GPUI Component.
///
/// This assets provides icons svg files for [IconName](https://docs.rs/gpui-component/latest/gpui_component/enum.IconName.html).
///
/// Pass [`Assets`] as the asset source when constructing the embedded host.
///
/// ## Platform Differences
///
/// - **Native (Desktop)**: Icons are embedded in the binary using RustEmbed
/// - **WASM (Web)**: Icons are downloaded from CDN using web_sys::Request
///   - This significantly reduces WASM bundle size
///   - Icons are downloaded on-demand when first used
///   - Downloaded icons are cached in memory
#[cfg(not(target_family = "wasm"))]
mod native_assets;

#[cfg(target_family = "wasm")]
mod wasm_assets;

#[cfg(not(target_family = "wasm"))]
pub use native_assets::Assets;

#[cfg(target_family = "wasm")]
pub use wasm_assets::Assets;
