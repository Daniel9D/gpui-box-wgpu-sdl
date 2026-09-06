# GPUI Box fork patches

Upstream: https://github.com/fran0220/gpui-box
Revision: ab8f37f6cbdee575f78cd4564597e9ae4d44e65c

## Allowed differences

- crates/gpui/src/elements/img.rs
- crates/gpui/src/external_image.rs
- crates/gpui/src/gpui.rs
- crates/gpui/src/scene.rs
- crates/gpui/src/window.rs
- crates/gpui_macos/src/metal_renderer.rs
- crates/gpui_windows/src/directx_renderer.rs
- crates/gpui_wgpu/src/wgpu_renderer.rs
- crates/gpui/src/app/headless_app_context.rs
- crates/gpui/src/platform/test/platform.rs
- FORK_PATCHES.md
- UPSTREAM.md

The first eight paths implement backend-neutral external images. The WGPU
renderer path also corrects the upstream uniform-layout test's stale 128-byte
header expectation after edge-mask fields expanded it to 144 bytes. The final
two Rust paths temporarily preserve current host clipboard/cursor behavior and
are removed when WgpuHost moves to EmbeddedPlatform. UPSTREAM.md and this
manifest record provenance only.
