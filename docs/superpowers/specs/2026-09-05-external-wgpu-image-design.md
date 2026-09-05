# External wgpu Image Design

## Goal

Allow a live, engine-owned `wgpu::TextureView` to be displayed through
`gpui::img` without CPU readback, an atlas upload, or ownership of the engine
render loop.

## Dependency topology

The public image contract belongs to `gpui-box`, while the concrete GPU
resource belongs to `gpui-box-wgpu`. Keep the core backend-neutral by adding
an opaque external-image interface to a local snapshot of GPUI Box revision
`5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`; do not add `wgpu` as a core
dependency.

Vendor only the GPUI Box workspace crates required to build and test the core.
The root workspace uses a Cargo source patch so its direct GPUI dependencies
and `gpui-box-kit` resolve through that same local core. Preserve upstream
license and revision provenance. The existing local renderer remains the only
`gpui-box-wgpu` implementation used by this workspace.

## Public API

`gpui-box` exposes a cloneable, type-erased `ExternalImage` handle containing
a stable identifier, physical pixel size, and an `Arc<dyn Any + Send + Sync>`
backend payload. `ImageSource`
accepts that handle and `img` treats it like any other image for intrinsic
size, `ObjectFit`, clipping, grayscale, opacity, and corner radii.

`gpui-box-wgpu` exposes:

```rust
let image = gpui_wgpu::WgpuImage::new(texture_view)?;

gpui::img(image.clone())
```

`WgpuImage` owns a clone of the view. The application retains and clones the
handle while the resource is displayed. Mutating the underlying texture updates
the next GPUI render automatically; recreating or resizing the texture requires
a new `WgpuImage`.

Passing raw `TextureView` directly to `img` is deliberately not supported:
the wrapper derives the physical size from its underlying texture and creates
the backend-neutral core handle without coupling GPUI Box to wgpu. The supplied
view must be the texture's default, full-resource 2D view; subset, array-layer
and nonzero-mip views are outside this first contract.

## Scene and rendering

During paint, GPUI computes the same fitted destination and cropped source
rectangle used by CPU images, then records an external-image primitive. The
scene owns the opaque handle until rendering completes.

The wgpu renderer recognizes `WgpuImage`, reuses the existing polychrome
sprite shader and sampler, creates a transient bind group for the supplied
`TextureView`, and draws the external-image primitive in normal scene order.
Consecutive primitives using the same handle may be batched, but no persistent
atlas entry or global registry is introduced.

Other renderers may skip an unsupported backend payload. The wgpu renderer
returns a descriptive error when a scene contains an external image of an
unknown payload type.

## Resource contract and validation

`WgpuImage::new` inspects `TextureView::texture()` and rejects:

- zero width or height;
- non-2D textures, multisampled textures, or textures with more than one
  depth/layer;
- textures lacking `TextureUsages::TEXTURE_BINDING`;
- underlying formats that cannot use the renderer's filterable floating-point
  texture binding.

wgpu does not expose the view descriptor, so the default/full-view condition
remains a documented caller contract. The texture must belong to the renderer's
device. Device identity and the final view-format compatibility are validated
when wgpu creates the bind group; validation failure must not trigger a CPU
fallback.

The engine submits writes to the shared queue before
`WgpuHost::render_to_view`; queue submission order makes those writes visible
to the UI draw.

## Testing

Follow TDD with two levels:

1. Core tests establish external source sizing, object-fit/source cropping,
   scene ordering, clipping and handle lifetime without wgpu.
2. A real GPU integration test creates a colored source texture on the engine
   device, renders it through `gpui::img(WgpuImage)` into a separate target,
   reads only the test target for assertions, updates the source texture, and
   proves the next frame changes without recreating the image or performing a
   runtime CPU readback.

Keep all existing default, host, kit, SDL, doctest and Clippy gates green.

## Non-goals

- Importing textures from another GPU device or graphics API.
- Sampling multisampled, array, cube, depth, storage-only, or non-filterable
  textures.
- Owning synchronization beyond the shared wgpu queue.
- Adding a CPU fallback or copying the external image into the GPUI atlas.
