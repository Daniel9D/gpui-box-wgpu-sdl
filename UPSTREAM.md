# Upstream provenance

- Repository: <https://github.com/fran0220/gpui-box>
- Revision: `ab8f37f6cbdee575f78cd4564597e9ae4d44e65c`
- Source path: `crates/gpui_wgpu`
- Initial engine copy: 2026-09-03
- Standalone extraction: 2026-09-04
- License: Apache-2.0, retained in `LICENSE-APACHE`.
- Additional notice: `HASH-PROSPECTOR-UNLICENSE.txt`.
- Bundled fonts: IBM Plex Sans and Lilex, with their licenses and source record
  retained in `assets/fonts/`.

This repository started from `rust-engine/vendor/gpui-box-wgpu`. The renderer,
shaders, and fonts were extracted with a standalone Cargo manifest. The complete
GPUI Box tree is vendored at the revision above; intentional differences are
listed exactly in `vendor/gpui-box/FORK_PATCHES.md` and enforced in CI.

The engine fork added `WgpuContext::from_external`,
`WgpuHeadlessRenderer::from_external`, and `render_scene_to_view` to retain the
host's device/queue and draw into its texture without a full-frame readback.
Those signatures remain compatible here and are available in the default
native build.

The standalone delta adds an optional, platform-neutral `WgpuHost` with caller
text/assets/root view, validated frame geometry, GPUI input dispatch, committed
text, and temporary external-target ownership. Production compatibility still
temporarily uses the pinned upstream's `gpui/test-support` APIs; `test-support`
remains an alias. The multi-window runtime phase removes that dependency only
after `WgpuHost` delegates to `EmbeddedPlatform`.

The retained fork capabilities at this checkpoint are:

- construction from a caller-owned WGPU instance, adapter, device, and queue;
- direct rendering into a caller-owned `wgpu::TextureView`;
- backend-neutral GPUI external images and the WGPU `WgpuImage` adapter;
- `gpui-box-kit` through the root crate's `kit` feature and re-export;
- an SDL3 bridge for keyboard, pointer, UTF-8 text/preedit, file drop, UTF-8
  clipboard synchronization, and native cursor synchronization.

Standalone tests and documentation live in this repository. Full-output GPU
readback in integration tests is measurement only. Existing explicit screenshot
APIs remain feature-gated, and the runtime external-target path performs no
full-frame readback. The fork also includes small Clippy fixes and a denied
cognitive-complexity lint. These rendering hooks remain candidates for upstream
contribution; adopting an upstream replacement requires checking API parity.
