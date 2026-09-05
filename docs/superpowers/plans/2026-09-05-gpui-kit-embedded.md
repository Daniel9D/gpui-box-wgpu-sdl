# GPUI Kit Embedded Implementation Plan

**Goal:** Run Longbridge components on the pinned GPUI Box with engine-owned GPU resources.
**Architecture:** Independent vendored compatibility workspace plus optional renderer integration. Engine owns scheduling and presentation.
**Tech Stack:** Rust, GPUI Box 0.1.2, GPUI Kit 0.6 snapshot, wgpu 30, SDL3 adapter.
**Spec:** ../specs/2026-09-05-gpui-kit-embedded-design.md

## Constraints
- GPUI Box revision 5c7e9eb6de8c8db3e7ff659934166218fb60f9f2 throughout.
- Preserve upstream license and snapshot provenance.
- No native platform bootstrap in the embedded facade.
- Default renderer dependency graph stays independent of kit.

## Tasks
- [x] Vendor required crates and run cargo check for gpui-kit to expose actual API errors; adapt each compiler-reported incompatibility, preserving semantics.
- [x] Add optional kit dependency and a real component host integration test. Test initialization, rendering, input and elapsed-time tasks before adding host APIs.
- [x] Run kit compilation and focused host tests, then workspace format and relevant regression checks.
- [x] Document dependency usage, fork changes, engine composition contract, and tested platform-service boundaries.
