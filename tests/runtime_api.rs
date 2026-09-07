#![cfg(all(not(target_family = "wasm"), feature = "host"))]

use std::sync::Arc;

use gpui_wgpu::{
    CloseOutcome, ExternalGpu, TextPreedit, WgpuRuntime, WgpuRuntimeBuilder, WgpuWindow,
    WgpuWindowState,
};

type RuntimeConstructor = fn(
    ExternalGpu,
    wgpu::TextureFormat,
    Arc<dyn gpui::PlatformTextSystem>,
    Arc<dyn gpui::AssetSource>,
) -> anyhow::Result<WgpuRuntime>;
type RuntimeBuilderConstructor = fn(
    ExternalGpu,
    wgpu::TextureFormat,
    Arc<dyn gpui::PlatformTextSystem>,
    Arc<dyn gpui::AssetSource>,
) -> WgpuRuntimeBuilder;

#[test]
fn multi_window_runtime_public_contract_is_available() {
    let _new: RuntimeConstructor = WgpuRuntime::new;
    let _builder: RuntimeBuilderConstructor = WgpuRuntime::builder;

    fn assert_window_traits<T: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash>() {}
    assert_window_traits::<WgpuWindow>();

    let _state = WgpuWindowState {
        cursor_style: gpui::CursorStyle::Arrow,
        cursor_visible: true,
        text_input_active: false,
        ime_area: None,
    };
    let _preedit = TextPreedit {
        text: "composição".to_owned(),
        selection_utf16: Some(0..3),
    };
    let _outcomes = [CloseOutcome::KeptOpen, CloseOutcome::Closed];
}

#[test]
fn render_window_accepts_a_borrowed_texture_view() {
    fn accepts_contract(
        runtime: &mut WgpuRuntime,
        window: WgpuWindow,
        target: &wgpu::TextureView,
    ) -> anyhow::Result<()> {
        runtime.render_window(
            window,
            target,
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            1.0,
        )
    }

    let _ = accepts_contract;
}
