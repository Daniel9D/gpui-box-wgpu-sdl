# External WGPU Image Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make `gpui::img(gpui_wgpu::WgpuImage::new(texture_view)?)` render a live, engine-owned `wgpu::TextureView` without CPU readback or atlas upload.

**Architecture:** Vendor GPUI Box core and GPUI Box Kit together at upstream revision `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`, point every consumer at that local workspace, and keep the core backend-neutral with an `ExternalImageHandle` carrying a type-erased payload. Add an ordered external-image primitive to the GPUI scene. The local WGPU renderer supplies the typed `WgpuImage` wrapper, validates the underlying texture, downcasts the scene payload, binds the external view directly, and reuses the polychrome sprite pipeline.

**Tech Stack:** Rust 2024, Cargo workspaces/source patches, GPUI Box 0.1.2, wgpu 30.0.1, anyhow, existing WGPU shaders and GPU test harness.

---

## Guardrails

- Preserve all current uncommitted `gpui-kit` to `gpui-box-kit` migration changes; stage only the files named by each task.
- Do not add `wgpu` to the GPUI core crate.
- Do not copy a `TextureView` into the GPUI atlas and do not add a CPU readback path.
- Use the existing polychrome sprite shader, sampler, blend modes, clipping, opacity, grayscale, and rounded-corner behavior.
- Treat a resized/recreated engine texture as a new `WgpuImage`; ordinary queue writes to the same texture must appear on the next render without recreating the handle.

### Task 1: Vendor and pin the GPUI Box core and kit workspace

**Files:**
- Create: `vendor/gpui-box/Cargo.toml`
- Create: `vendor/gpui-box/Cargo.lock`
- Create: `vendor/gpui-box/LICENSE`
- Create: `vendor/gpui-box/LICENSE-APACHE`
- Create: `vendor/gpui-box/THIRD_PARTY_NOTICES`
- Create: `vendor/gpui-box/UPSTREAM.md`
- Create: `vendor/gpui-box/crates/{collections,gpui,gpui_linux,gpui_macos,gpui_macros,gpui_platform,gpui_shared_string,gpui_util,gpui_web,gpui_wgpu,gpui_windows,http_client,media,refineable,scheduler,sum_tree,util_macros}/**`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

- [ ] **Step 1: Record the failing dependency-source assertion**

Run:

```powershell
cargo tree -p gpui-box-wgpu -i gpui-box --prefix none
```

Expected: the `gpui-box` package still resolves from the pinned Git checkout, so the tree does not contain `vendor/gpui-box/crates/gpui`.

- [ ] **Step 2: Copy the upstream core closure without its Git metadata**

Copy the root manifests/notices and the listed 17 crate directories from the cached checkout for commit `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`. Include `crates/refineable/derive_refineable`, which is nested under `refineable`. Do not copy examples, kit crates, tools, snapshots, generated validation output, or upstream `.git`.

Write `vendor/gpui-box/UPSTREAM.md` with:

```markdown
# Upstream snapshot

- Repository: https://github.com/fran0220/gpui-box
- Revision: `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`
- Scope: GPUI core and the path-dependency closure needed to build and test it.

Local changes are intentionally limited to the backend-neutral external-image API and scene primitive required by this repository's WGPU renderer.
```

- [ ] **Step 3: Prune the nested workspace membership**

In `vendor/gpui-box/Cargo.toml`, retain upstream workspace package/lint/dependency declarations, but replace `members` and `default-members` with exactly:

```toml
members = [
    "crates/collections",
    "crates/gpui",
    "crates/gpui_linux",
    "crates/gpui_macos",
    "crates/gpui_macros",
    "crates/gpui_platform",
    "crates/gpui_shared_string",
    "crates/gpui_util",
    "crates/gpui_web",
    "crates/gpui_wgpu",
    "crates/gpui_windows",
    "crates/http_client",
    "crates/media",
    "crates/refineable",
    "crates/refineable/derive_refineable",
    "crates/scheduler",
    "crates/sum_tree",
    "crates/util_macros",
]
default-members = ["crates/gpui"]
```

In the outer `Cargo.toml`, add `"vendor/gpui-box"` to `[workspace].exclude` and add:

```toml
gpui = { package = "gpui-box", path = "vendor/gpui-box/crates/gpui" }
gpui-kit = { package = "gpui-box-kit", path = "vendor/gpui-box/crates/gpui-kit" }
```

Keep `gpui-box-kit` on the pinned Git revision; Cargo must patch its transitive `gpui-box` dependency to the local core.

- [ ] **Step 4: Prove the snapshot and source unification resolve**

Run:

```powershell
cargo metadata --manifest-path vendor/gpui-box/Cargo.toml --format-version 1 --no-deps
cargo tree -p gpui-box-wgpu -i gpui-box --prefix none
cargo tree -p gpui-box-wgpu --features kit -d
```

Expected: metadata succeeds; every `gpui-box` occurrence points to `vendor/gpui-box/crates/gpui`; duplicate output contains no second `gpui-box` version/source.

- [ ] **Step 5: Commit only the vendoring/configuration slice**

```powershell
git add Cargo.toml Cargo.lock vendor/gpui-box
git commit -m "build: vendor gpui core for renderer extensions"
```

### Task 2: Add a backend-neutral external image to GPUI layout and scene recording

**Files:**
- Create: `vendor/gpui-box/crates/gpui/src/external_image.rs`
- Modify: `vendor/gpui-box/crates/gpui/src/gpui.rs`
- Modify: `vendor/gpui-box/crates/gpui/src/elements/img.rs`
- Modify: `vendor/gpui-box/crates/gpui/src/window.rs`
- Modify: `vendor/gpui-box/crates/gpui/src/scene.rs`
- Test: `vendor/gpui-box/crates/gpui/src/elements/img.rs`
- Test: `vendor/gpui-box/crates/gpui/src/scene.rs`

- [ ] **Step 1: Write failing tests for the public handle and `img` behavior**

Add tests that construct a backend-neutral image with a `String` payload:

```rust
let image = Arc::new(ExternalImageHandle::new(size(DevicePixels(200), DevicePixels(100)), "gpu".to_owned()));
assert_eq!(image.size(), size(DevicePixels(200), DevicePixels(100)));
assert_eq!(image.downcast_ref::<String>().map(String::as_str), Some("gpu"));
assert_ne!(image.id(), Arc::new(ExternalImageHandle::new(image.size(), ())).id());
```

Using the existing `TestAppContext` image tests, render `img(image.clone())` and assert:

- automatic layout is `200 x 100` device pixels at scale factor 1;
- `ObjectFit::Contain` preserves the 2:1 aspect ratio;
- `ObjectFit::Cover` records the cropped source rectangle;
- the scene record retains the same `Arc` payload after the element value is dropped;
- clipping, grayscale, opacity, and corner radii are copied into the recorded sprite.

Run:

```powershell
cargo test --manifest-path vendor/gpui-box/Cargo.toml -p gpui-box external_image --no-default-features
```

Expected: FAIL because `ExternalImageHandle`, the `ImageSource` conversion, and the scene primitive do not exist.

- [ ] **Step 2: Implement the type-erased handle**

Create `external_image.rs` with a process-local atomic ID and this backend-neutral API:

```rust
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ExternalImageId(pub usize);

#[derive(Clone)]
pub struct ExternalImageHandle {
    id: ExternalImageId,
    size: Size<DevicePixels>,
    payload: Arc<dyn Any + Send + Sync>,
}

impl ExternalImageHandle {
    pub fn new<T: Any + Send + Sync>(size: Size<DevicePixels>, payload: T) -> Self;
    pub fn id(&self) -> ExternalImageId;
    pub fn size(&self) -> Size<DevicePixels>;
    pub fn downcast_ref<T: Any>(&self) -> Option<&T>;
}
```

Implement `Debug` manually so it reports only `id` and `size`. Declare and publicly re-export the module from `gpui.rs`.

- [ ] **Step 3: Make `img` treat external images as immediately available**

Add `ImageSource::External(Arc<ExternalImageHandle>)` plus `From<Arc<ExternalImageHandle>>`. Refactor the existing intrinsic-size calculation into a small helper shared by CPU and external branches. In `request_layout`, bypass asset loading for `External` and use `external.size()` for aspect ratio and automatic width/height. In `paint`, calculate `object_fit` from the same physical size and call `Window::paint_external_image`. Update `use_data`, `get_data`, `remove_asset`, and `is_asset_cached` exhaustively so external images never enter the asset loader or loading/fallback state.

- [ ] **Step 4: Record an ordered scene primitive**

Add:

```rust
#[derive(Clone, Debug)]
pub struct PaintExternalImage {
    pub image: Arc<ExternalImageHandle>,
    pub sprite: PolychromeSprite,
}
```

Add `external_images: Vec<PaintExternalImage>` to `Scene`, a `Primitive::ExternalImage` variant, and:

```rust
PrimitiveBatch::ExternalImages {
    image_id: ExternalImageId,
    blend_mode: SpriteBlendMode,
    range: Range<usize>,
}
```

Implement the same order/bounds/occlusion plumbing as `PolychromeSprite`. Coalesce only consecutive external primitives having the same `image_id` and `blend_mode`; never reorder across quads, paths, text, atlas sprites, or other external images.

- [ ] **Step 5: Implement `Window::paint_external_image` with the shared crop math**

Extract the visible-bounds/source-rectangle calculation currently embedded in `paint_image` into a private helper that accepts `bounds`, `image_bounds`, and physical source size. Keep `paint_image` behavior unchanged. Build a `PolychromeSprite` for the external image with the calculated source rectangle encoded as an atlas tile, white tint, normal blending, current content mask, snapped bounds, requested grayscale, current opacity, and clamped corner radii; insert it as `PaintExternalImage` instead of entering the atlas.

- [ ] **Step 6: Run the focused and regression tests**

```powershell
cargo test --manifest-path vendor/gpui-box/Cargo.toml -p gpui-box external_image --no-default-features
cargo test --manifest-path vendor/gpui-box/Cargo.toml -p gpui-box elements::img --no-default-features
```

Expected: PASS, including existing zero-frame and object-fit tests.

- [ ] **Step 7: Commit the core contract**

```powershell
git add vendor/gpui-box/crates/gpui/src
git commit -m "feat(gpui): add backend-neutral external images"
```

### Task 3: Add the public `WgpuImage` wrapper and validation

**Files:**
- Create: `src/wgpu_image.rs`
- Modify: `src/gpui_wgpu.rs`
- Test: `tests/wgpu_host_gpu.rs`

- [ ] **Step 1: Write failing validation and conversion tests**

In the GPU test fixture, create textures and assert:

```rust
let image = WgpuImage::new(valid_texture.create_view(&Default::default())).unwrap();
let _: gpui::ImageSource = image.clone().into();
assert_eq!(image.size(), gpui::size(DevicePixels(32), DevicePixels(16)));
```

Also verify descriptive errors for non-2D textures, multisampled textures, depth/array count other than one, missing `TEXTURE_BINDING`, and a non-filterable/depth/integer format. Test zero width/height through the private pure validation helper because wgpu rejects zero-sized textures before it can create a `TextureView`.

Run:

```powershell
cargo test --features host --test wgpu_host_gpu wgpu_image_validation -- --nocapture
```

Expected: FAIL because `WgpuImage` does not exist.

- [ ] **Step 2: Implement the smallest typed wrapper**

Create a private payload and public cloneable wrapper:

```rust
pub(crate) struct WgpuImagePayload {
    pub(crate) view: wgpu::TextureView,
    pub(crate) format: wgpu::TextureFormat,
}

#[derive(Clone)]
pub struct WgpuImage(Arc<gpui::ExternalImageHandle>);

impl WgpuImage {
    pub fn new(view: wgpu::TextureView) -> anyhow::Result<Self>;
    pub fn size(&self) -> gpui::Size<gpui::DevicePixels>;
}

impl From<WgpuImage> for gpui::ImageSource;
```

Use `view.texture()` to inspect size, dimension, sample count, usage, and base format. Put those scalar properties behind a private pure validation helper so impossible-to-construct zero dimensions remain testable. Accept only non-zero single-layer D2 textures with one sample, `TEXTURE_BINDING`, and `TextureFormat::sample_type(None, None) == Some(TextureSampleType::Float { filterable: true })`; conservatively reject formats such as `R32Float` whose filterability depends on optional device features. Store the cloned view in `WgpuImagePayload`, wrap it in `gpui::ExternalImageHandle`, and export `WgpuImage` from `gpui_wgpu.rs`. Document that callers must pass the default full-resource view; a different view format is rejected later by WGPU bind validation because wgpu does not expose the view descriptor.

- [ ] **Step 3: Run the focused test**

```powershell
cargo test --features host --test wgpu_host_gpu wgpu_image_validation -- --nocapture
```

Expected: PASS on an available adapter; retain the suite's existing graceful skip when no adapter is present.

- [ ] **Step 4: Commit the public wrapper**

```powershell
git add src/wgpu_image.rs src/gpui_wgpu.rs tests/wgpu_host_gpu.rs
git commit -m "feat: add validated wgpu image handle"
```

### Task 4: Bind external views directly in the WGPU renderer

**Files:**
- Modify: `src/wgpu_renderer.rs`
- Test: `tests/wgpu_host_gpu.rs`

- [ ] **Step 1: Write the failing zero-copy live-update test**

Add a view whose root renders `gpui::img(image.clone()).size_full().object_fit(ObjectFit::Fill)`. Create a `32 x 16` engine texture with `TEXTURE_BINDING | COPY_DST`, write solid red bytes with `queue.write_texture`, render the host into the existing readable target, and assert a center pixel is red. Write solid green bytes to the same engine texture, render again without replacing `WgpuImage`, and assert the center pixel is green.

The only `COPY_SRC`/map/readback belongs to the test's render target. The external source texture must not have `COPY_SRC`, which makes any implementation-time source readback invalid.

Run:

```powershell
cargo test --features host --test wgpu_host_gpu external_texture_view_updates_without_recreating_image -- --nocapture
```

Expected: FAIL because `PrimitiveBatch::ExternalImages` is not handled by the WGPU renderer.

- [ ] **Step 2: Upload external sprite instances**

Extend the per-frame instance bindings with one buffer for the `PolychromeSprite` values contained in `scene.external_images`. Reuse the existing projected-sprite byte layout and instance-range accounting; do not create a second shader or a texture registry.

- [ ] **Step 3: Render each external batch with a transient texture bind group**

Add a renderer helper that:

1. takes the first primitive in the batch;
2. downcasts `primitive.image` to `WgpuImagePayload` and returns an `anyhow` error containing the external ID when the type is unsupported;
3. creates the same texture/sampler bind group used by polychrome atlas sprites, but binds `payload.view` directly;
4. selects the existing normal/additive/screen polychrome pipeline from the batch's blend mode;
5. issues the external instance range in the current render pass.

Handle `PrimitiveBatch::ExternalImages` in the main ordered batch match immediately alongside polychrome sprites. Allow WGPU device/view-format incompatibility to return through the renderer's existing error path; do not substitute a CPU image.

- [ ] **Step 4: Add unsupported-payload and ordering coverage**

Render a raw `gpui::ExternalImageHandle` carrying `()` and assert `render_to_view` returns a descriptive unsupported-payload error. Add a pixel test with an external image between two translucent quads so the final color proves scene order was preserved.

- [ ] **Step 5: Run GPU and host regressions**

```powershell
cargo test --features host --test wgpu_host_gpu external_ -- --nocapture
cargo test --features host --test wgpu_host_gpu -- --nocapture
```

Expected: PASS or the existing adapter-unavailable skip; the live-update assertion changes red to green while retaining the same `WgpuImage` ID.

- [ ] **Step 6: Commit the renderer integration**

```powershell
git add src/wgpu_renderer.rs tests/wgpu_host_gpu.rs
git commit -m "feat: render external wgpu texture views"
```

### Task 5: Document the API and verify the complete migration

**Files:**
- Modify: `README.md`
- Modify: `Cargo.lock`

- [ ] **Step 1: Add the engine integration example**

Document:

```rust
let image = gpui_wgpu::WgpuImage::new(texture.create_view(&Default::default()))?;

// Queue writes submitted before WgpuHost::render_to_view are visible this frame.
gpui::img(image.clone())
```

State the accepted texture constraints and that resize/recreation requires constructing a new `WgpuImage`.

- [ ] **Step 2: Format and run the full verification matrix**

```powershell
cargo fmt --all
cargo fmt --all -- --check
cargo test --all-targets
cargo test --all-targets --features host
cargo test --all-targets --features kit
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps --all-features
cargo test --manifest-path vendor/gpui-box/Cargo.toml -p gpui-box --no-default-features
cargo tree -p gpui-box-wgpu --features kit -d
```

Expected: every command passes; there is one effective local `gpui-box` core source; `gpui-box-kit` resolves from the same vendored workspace; no standalone `vendor/gpui-kit` source remains.

- [ ] **Step 3: Inspect the final diff for scope and readback regressions**

```powershell
git diff --check
rg -n "copy_texture_to_buffer|map_async|ImageData|TextureView" src vendor/gpui-box/crates/gpui/src README.md
git status --short
```

Expected: source-texture rendering uses only direct view binding; readback calls appear only in test-target verification or pre-existing unrelated code; current migration changes remain intact.

- [ ] **Step 4: Commit documentation and lockfile**

```powershell
git add README.md Cargo.lock
git commit -m "docs: explain external wgpu images"
```

- [ ] **Step 5: Request a correctness review before integration**

Use `superpowers:requesting-code-review` against the complete diff, apply only verified findings, rerun the affected focused test plus the full verification matrix, and then use `superpowers:finishing-a-development-branch` to present the integration options.
