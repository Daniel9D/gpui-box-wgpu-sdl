# Upstream provenance

- Repository: <https://github.com/fran0220/gpui-box>
- Revision: `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`
- Source path: `crates/gpui_wgpu`
- Initial engine copy: 2026-09-03
- Standalone extraction: 2026-09-04
- License: Apache-2.0, retained in `LICENSE-APACHE`.
- Additional notice: `HASH-PROSPECTOR-UNLICENSE.txt`.
- Bundled fonts: IBM Plex Sans and Lilex, with their licenses and source record
  retained in `assets/fonts/`.

This repository started from `rust-engine/vendor/gpui-box-wgpu`. The source,
shaders, and fonts were copied from that fork, with a standalone Cargo manifest.
The engine's vendor directory remains an unchanged earlier copy; this extraction
does not modify the engine or repoint its dependency.

The engine fork added `WgpuContext::from_external`,
`WgpuHeadlessRenderer::from_external`, and `render_scene_to_view` to retain the
host's device/queue and draw into its texture without a full-frame readback.
Those signatures remain compatible here and are available in the default
native build.

The standalone delta adds an optional, platform-neutral `WgpuHost` with caller
text/assets/root view, validated frame geometry, GPUI input dispatch, committed
text, and temporary external-target ownership. The `host` feature uses the
pinned upstream's `gpui/test-support` APIs; `test-support` remains an alias.
No native event library is introduced.

Standalone tests and documentation live in this repository. Full-output GPU
readback in integration tests is measurement only. Existing explicit screenshot
APIs remain feature-gated, and the runtime external-target path performs no
full-frame readback. The fork also includes small Clippy fixes and a denied
cognitive-complexity lint. These rendering hooks remain candidates for upstream
contribution; adopting an upstream replacement requires checking API parity.
