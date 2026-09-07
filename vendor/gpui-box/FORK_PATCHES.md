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
- crates/gpui/src/platform.rs
- crates/gpui/src/platform/threaded_dispatcher.rs
- CHANGELOG.md
- FORK_PATCHES.md
- UPSTREAM.md

The first eight paths implement backend-neutral external images. The WGPU
renderer path also corrects the upstream uniform-layout test's stale 128-byte
header expectation after edge-mask fields expanded it to 144 bytes. The test
platform paths preserve compatibility tests. `platform.rs`,
`platform/threaded_dispatcher.rs`, and `CHANGELOG.md` expose and document the
host-driven realtime dispatcher used by `EmbeddedPlatform` without enabling
GPUI's test-support feature. UPSTREAM.md and this manifest record provenance.
