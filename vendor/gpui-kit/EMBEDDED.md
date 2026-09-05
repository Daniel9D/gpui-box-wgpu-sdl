# Embedded GPUI Kit fork

Source: https://github.com/longbridge/gpui-kit
Revision: b586fad393daf683301c634fa4e5d98b4393bbb3
License: Apache-2.0 (see LICENSE-APACHE). Bundled Lucide icons are ISC,
including Feather-derived icons under MIT (see LICENSE-LUCIDE); those terms
apply to `crates/assets/assets/icons`.

This snapshot contains kit, base, component, component-macros and assets. It is
adapted to GPUI Box revision 5c7e9eb6de8c8db3e7ff659934166218fb60f9f2.
It is an independent Cargo workspace so it can later be extracted into a fork.

The embedded facade exports GPUI Box, base, component, assets and init. It does
not export application/platform/web bootstrap: the embedding engine supplies
the host. Native menus and OS services require host integration.

Upstream README and examples describe the original native bootstrap, not this
embedded build. Follow the repository root integration instructions instead.

Spring primitives in crates/base/src/motion/spring_compat.rs are from gpui-pre 0.3.1, src/spring.rs (Zed, Apache-2.0), with imports redirected to GPUI Box. Original mathematical implementation and tests are retained.

Compatibility changes:
- GPUI, macros and sum-tree resolve to the pinned GPUI Box family.
- IntoPlot macro resolves renamed gpui-box dependencies.
- TextRun background_radius defaults to None; outward shadows use ShadowStyle::Drop.
- Loading shimmer uses per-element repeat; shared-phase repetition is unavailable.
- Button/Input accessibility_id builders are omitted because GPUI Box does not
  expose author IDs. The associated upstream-only author-ID test is omitted.
  Accessible roles, labels and input actions remain; OS accessibility transport
  still requires an embedding platform adapter.
- Tests are self-contained within the five-crate snapshot. The Markdown facade
  uses an inline fixture, and the omitted Aurora example remains covered by the
  inline gradient parsing regression.
- The native application/platform/web facade is omitted.
