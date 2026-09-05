# Embedded GPUI Kit

This package is the compatibility facade used by the embedded GPUI Box host in
the repository root. It is not the crates.io `gpui-kit` release and is not
publishable.

| Path                  | Crate             | Feature          |
| --------------------- | ----------------- | ---------------- |
| `gpui_kit::*`         | `gpui-box`        | always           |
| `gpui_kit::base`      | `gpui-base`       | always           |
| `gpui_kit::component` | `gpui-component`  | `component` (on) |
| `gpui_kit::assets`    | `gpui-kit-assets` | `assets` (on)    |

Call `gpui_kit::init(cx)` once inside the root builder supplied to
`WgpuHost::new`, then wrap the application view in
`gpui_kit::component::Root`. The embedding engine owns the platform, window,
GPU resources, frame loop and presentation; this facade intentionally does not
export `application()` or a platform bootstrap.

The component features and `test-support` feature remain available under their
upstream names. See the repository root README and `../../EMBEDDED.md` for the
integration contract, provenance and compatibility differences.
