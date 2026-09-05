#![cfg(not(target_family = "wasm"))]

use gpui_wgpu::WgpuHeadlessRenderer;

#[test]
fn external_renderer_api_is_available_without_optional_features() {
    let _constructor = WgpuHeadlessRenderer::from_external;
    let _render = WgpuHeadlessRenderer::render_scene_to_view;
}
