# Upstream provenance

- Repository: <https://github.com/fran0220/gpui-box>
- Revision: `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`
- Source path: `crates/gpui_wgpu`
- Copied on: 2026-09-03
- License: Apache-2.0; retained in `LICENSE-APACHE`.

The initial local copy contained the upstream `src` tree, shaders, and bundled
font assets unchanged. The workspace-inherited manifest was replaced by the
standalone manifest in this directory.

The local compatibility delta is intentionally limited to:

1. accepting an existing wgpu instance, adapter, device, and queue;
2. rendering a GPUI scene into a caller-provided texture view.

These hooks are intended for upstream contribution. Once an equivalent public
API ships in GPUI Box, this directory should be removed and the root dependency
returned to the upstream package.

## Implemented compatibility API

- `WgpuContext::from_external` retains host-owned `Arc<Device>` and
  `Arc<Queue>` handles without requesting another device.
- `WgpuHeadlessRenderer::from_external` validates the engine target format and
  creates renderer pipelines for it.
- `WgpuHeadlessRenderer::render_scene_to_view` draws into a caller-owned
  `TextureView` without allocating or reading back an output framebuffer.

Engine integration and behavioral tests live outside this directory. The only
readback is test instrumentation in `tests/gpui_direct_gpu.rs`.
