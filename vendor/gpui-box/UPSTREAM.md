# Upstream snapshot

- Repository: https://github.com/fran0220/gpui-box
- Revision: `5c7e9eb6de8c8db3e7ff659934166218fb60f9f2`
- Scope: GPUI core, GPUI Box Kit, their path-dependency closure, and the upstream `block` compatibility fork needed to build and test them.

Local changes add the backend-neutral external-image API and scene primitive required by this repository's WGPU renderer. GPUI Box Kit is kept in this same workspace so every public API uses one canonical set of GPUI types.
