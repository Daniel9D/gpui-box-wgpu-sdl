# Upstream Resync and Compatibility Gates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resynchronize the vendored GPUI Box and standalone WGPU renderer to upstream commit `ab8f37f6cbdee575f78cd4564597e9ae4d44e65c`, preserve every fork feature, and freeze a verified base for the multi-window refactor.

**Architecture:** Replace the vendored upstream tree first, then reapply only the documented fork patch stack and refresh the standalone renderer from that same tree. Compatibility tests and a vendor-drift check prevent the resync from silently losing external images, the current `WgpuHost`, GPUI Box Kit, or SDL3 input/platform behavior.

**Tech Stack:** Rust 2024, GPUI Box, WGPU 30, SDL3, Cargo, GitHub Actions, Python 3 standard library.

**Spec:** `docs/superpowers/specs/2026-09-06-multi-window-wgpu-runtime-design.md`

## Global Constraints

- Work only in an isolated worktree created from commit `f710cbd`.
- Pin GPUI Box exactly to `ab8f37f6cbdee575f78cd4564597e9ae4d44e65c`; do not follow a moving branch during this refactor.
- Preserve all current `WgpuHost`, `WgpuImage`, GPUI Box Kit, and `gpui-box-sdl` public behavior.
- Keep SDL3 the first-class integration; do not introduce another native event/window dependency.
- Keep caller-owned `Instance`, `Adapter`, `Device`, `Queue`, `TextureView`, surfaces, and presentation.
- Preserve UTF-8 clipboard, complete keyboard mapping, IME/preedit events, cursor commands, and multi-file drag-and-drop.
- Runtime code must not add CPU image fallback/readback or synchronous waits to realtime rendering.
- Do not begin `EmbeddedPlatform` or `WgpuRuntime` implementation in this plan; the next plan is written against the resynchronized base.
- Every GPU-creating test takes the repository serialization guard and may skip only when no compatible adapter exists.

---

## File Structure

- `.github/workflows/ci.yml`: multiplatform format, feature, test, Clippy, docs, and WASM gates.
- `UPSTREAM.md`: authoritative standalone provenance, frozen SHA, and patch-stack summary.
- `vendor/gpui-box/FORK_PATCHES.md`: machine-readable pin and allowlist plus human-readable patch purpose.
- `scripts/check_vendor_drift.py`: stdlib-only comparison of vendored files against the frozen GitHub archive.
- `scripts/test_check_vendor_drift.py`: isolated unit test for allowlisted and unexpected drift.
- `vendor/gpui-box/**`: exact upstream snapshot plus the allowlisted GPUI external-image and temporary compatibility patches.
- `src/wgpu_context.rs`, `src/wgpu_renderer.rs`, `src/wgpu_atlas.rs`, shader files, and `src/cosmic_text_system.rs`: standalone renderer refreshed from the frozen vendored WGPU crate.
- `src/wgpu_host.rs`, `src/wgpu_image.rs`, and `src/gpui_wgpu.rs`: retained standalone-only facade and exports.
- `Cargo.toml`, `Cargo.lock`, `crates/gpui-box-sdl/Cargo.toml`: dependency and feature reconciliation after the resync.
- `tests/default_external_api.rs`, `tests/wgpu_host_gpu.rs`, and `crates/gpui-box-sdl/tests/**`: unchanged compatibility gates unless an upstream rename requires a mechanical adjustment with identical assertions.

---

### Task 1: Freeze the pre-resync compatibility baseline

**Files:**
- Create: `.github/workflows/ci.yml`
- Test: `tests/default_external_api.rs`
- Test: `tests/wgpu_host_gpu.rs`
- Test: `crates/gpui-box-sdl/tests/**`

**Interfaces:**
- Consumes: current `WgpuHost`, `WgpuImage`, `WgpuHeadlessRenderer`, GPUI Box Kit reexport, and SDL adapter/bridge APIs.
- Produces: one repeatable CI command matrix that must remain green after every later task.

- [ ] **Step 1: Run the complete current baseline**

Run:

```powershell
cargo fmt --all --check
cargo test --workspace --all-features -j 1
cargo clippy --workspace --all-targets --all-features -j 1 -- -D warnings
cargo doc --workspace --all-features --no-deps
cargo check --no-default-features
cargo check --features host
cargo check --features kit
cargo check -p gpui-box-sdl
```

Expected: every command exits zero. GPU tests may print an adapter skip only before a device is created.

- [ ] **Step 2: Add the CI workflow**

Create `.github/workflows/ci.yml` with four jobs:

```yaml
name: ci

on:
  push:
  pull_request:

jobs:
  rust:
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --all --check
      - run: cargo check --workspace --all-features
      - run: cargo test --workspace --all-features -j 1
      - run: cargo clippy --workspace --all-targets --all-features -j 1 -- -D warnings
      - run: cargo doc --workspace --all-features --no-deps

  features:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo check --no-default-features
      - run: cargo check --features host
      - run: cargo check --features kit
      - run: cargo check -p gpui-box-sdl

  wasm:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
      - run: cargo check --target wasm32-unknown-unknown --no-default-features

  vendor-drift:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.x"
      - run: python scripts/check_vendor_drift.py
```

- [ ] **Step 3: Validate the workflow syntax locally**

Run:

```powershell
git diff --check
rg -n "cargo (fmt|check|test|clippy|doc)|check_vendor_drift" .github/workflows/ci.yml
```

Expected: `git diff --check` exits zero and `rg` prints every required gate.

- [ ] **Step 4: Commit the baseline gate**

```powershell
git add .github/workflows/ci.yml
git commit -m "ci: freeze pre-refactor compatibility gates"
```

---

### Task 2: Replace the vendored tree with the frozen upstream snapshot

**Files:**
- Modify: `vendor/gpui-box/**`
- Create: `vendor/gpui-box/FORK_PATCHES.md`

**Interfaces:**
- Consumes: upstream repository `https://github.com/fran0220/gpui-box` and frozen commit `ab8f37f6cbdee575f78cd4564597e9ae4d44e65c`.
- Produces: a byte-for-byte upstream vendor tree before fork patches are reintroduced.

- [ ] **Step 1: Fetch only the two required upstream commits**

Run:

```powershell
git remote add gpui-box-upstream https://github.com/fran0220/gpui-box.git
git fetch gpui-box-upstream 5c7e9eb6de8c8db3e7ff659934166218fb60f9f2 ab8f37f6cbdee575f78cd4564597e9ae4d44e65c
git merge-base --is-ancestor 5c7e9eb6de8c8db3e7ff659934166218fb60f9f2 ab8f37f6cbdee575f78cd4564597e9ae4d44e65c
```

Expected: fetch succeeds and `merge-base` exits zero.

- [ ] **Step 2: Record the upstream change surface before replacement**

Run:

```powershell
git diff --name-status 5c7e9eb6de8c8db3e7ff659934166218fb60f9f2 ab8f37f6cbdee575f78cd4564597e9ae4d44e65c -- crates/gpui crates/gpui_wgpu crates/gpui-kit
git log --oneline 5c7e9eb6de8c8db3e7ff659934166218fb60f9f2..ab8f37f6cbdee575f78cd4564597e9ae4d44e65c -- crates/gpui crates/gpui_wgpu crates/gpui-kit
```

Expected: output identifies the GPUI, renderer, and kit files that changed; save it in the task log, not in the repository.

- [ ] **Step 3: Replace the vendor directory from a temporary Git checkout**

Run in the isolated worktree:

```powershell
$repositoryRoot = (git rev-parse --show-toplevel | Split-Path -Parent | Split-Path -Parent)
$snapshot = Join-Path $repositoryRoot '.worktrees/upstream-snapshot'
git worktree add --detach $snapshot ab8f37f6cbdee575f78cd4564597e9ae4d44e65c
git rm -r vendor/gpui-box
New-Item -ItemType Directory -Path vendor/gpui-box | Out-Null
Get-ChildItem -LiteralPath $snapshot -Force |
  Where-Object Name -ne '.git' |
  Copy-Item -Destination vendor/gpui-box -Recurse -Force
git worktree remove --force $snapshot
git add vendor/gpui-box
```

The temporary checkout is required on Windows because the upstream tree stores
license links as Git symlinks (`mode 120000`); with `core.symlinks=false`, Git
materializes them as ordinary files while `tar.exe` fails to create them.

Expected: every path from `git ls-tree -r --name-only
ab8f37f6cbdee575f78cd4564597e9ae4d44e65c` exists below
`vendor/gpui-box`, `git status --short vendor/gpui-box` shows the upstream
replacement, and no file outside `vendor/gpui-box` changed.

- [ ] **Step 4: Create the fork patch manifest**

Create `vendor/gpui-box/FORK_PATCHES.md` with this exact structure and allowlist:

```markdown
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
- FORK_PATCHES.md
- UPSTREAM.md

The first eight paths implement backend-neutral external images. The final two
Rust paths temporarily preserve current host clipboard/cursor behavior and are
removed when WgpuHost moves to EmbeddedPlatform. UPSTREAM.md and this manifest
record provenance only.
```

- [ ] **Step 5: Commit the clean upstream replacement**

```powershell
git add vendor/gpui-box
# Do not run `git diff --cached --check` for this snapshot commit. The exact
# upstream tree contains historical trailing whitespace in fixture/patch files;
# rewriting those bytes would itself create vendor drift.
git commit -m "build: resync vendored GPUI Box upstream"
```

---

### Task 3: Reapply the minimal vendored GPUI patch stack

**Files:**
- Modify: paths listed in `vendor/gpui-box/FORK_PATCHES.md`
- Test: vendored GPUI unit tests reached through the workspace dependency graph.

**Interfaces:**
- Consumes: fork commits `165878d`, `23aa4ac`, and `66cc310` from repository history.
- Produces: backend-neutral `ExternalImageHandle`, `ImageSource::External`, scene primitives, backend handling, and temporary legacy host clipboard/cursor hooks on the new upstream base.

- [ ] **Step 1: Apply the backend-neutral external-image patch**

Run:

```powershell
$gpuiPatch = @(
  'vendor/gpui-box/crates/gpui/src/elements/img.rs',
  'vendor/gpui-box/crates/gpui/src/external_image.rs',
  'vendor/gpui-box/crates/gpui/src/gpui.rs',
  'vendor/gpui-box/crates/gpui/src/scene.rs',
  'vendor/gpui-box/crates/gpui/src/window.rs',
  'vendor/gpui-box/crates/gpui_macos/src/metal_renderer.rs',
  'vendor/gpui-box/crates/gpui_windows/src/directx_renderer.rs',
  'vendor/gpui-box/crates/gpui_wgpu/src/wgpu_renderer.rs'
)
git show --format= 165878d -- $gpuiPatch | git apply --3way
```

Expected: patch applies cleanly. If Git reports a conflict, stop this task and review the corresponding upstream API before editing; do not discard either implementation.

- [ ] **Step 2: Apply the WGPU external-view refinement to GPUI scene files**

Run:

```powershell
$viewPatch = @(
  'vendor/gpui-box/crates/gpui/src/elements/img.rs',
  'vendor/gpui-box/crates/gpui/src/scene.rs',
  'vendor/gpui-box/crates/gpui/src/window.rs'
)
git show --format= 23aa4ac -- $viewPatch | git apply --3way
```

Expected: patch applies or stops at a concrete upstream conflict. After success, `rg -n "ExternalImageHandle|paint_external_image|PrimitiveBatch::ExternalImages" vendor/gpui-box/crates` finds the retained API and every renderer has an explicit external-image arm.

- [ ] **Step 3: Reapply temporary legacy host platform hooks**

Run:

```powershell
$legacyPatch = @(
  'vendor/gpui-box/crates/gpui/src/app/headless_app_context.rs',
  'vendor/gpui-box/crates/gpui/src/platform/test/platform.rs'
)
git show --format= 66cc310 -- $legacyPatch | git apply --3way
```

Expected: current `WgpuHost` clipboard and cursor methods remain available until the production platform replaces these test hooks.

- [ ] **Step 4: Run focused GPUI and API checks**

Run:

```powershell
cargo test --features host --test default_external_api
cargo test --features host --test wgpu_host_gpu external_image -- --nocapture
cargo check --features kit
```

Expected: external image API/render tests and GPUI Box Kit compile. Any upstream API break is fixed without weakening an assertion or broadening the manifest allowlist.

- [ ] **Step 5: Commit the vendored patch stack**

```powershell
git add vendor/gpui-box
git commit -m "feat: reapply GPUI external image patches"
```

---

### Task 4: Refresh the standalone renderer from the same upstream base

**Files:**
- Modify: `src/cosmic_text_system.rs`
- Modify: `src/wgpu_atlas.rs`
- Modify: `src/wgpu_context.rs`
- Modify: `src/wgpu_renderer.rs`
- Modify: `src/*.wgsl`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Preserve: `src/wgpu_host.rs`
- Preserve: `src/wgpu_image.rs`
- Preserve: host/kit exports in `src/gpui_wgpu.rs`

**Interfaces:**
- Consumes: `vendor/gpui-box/crates/gpui_wgpu` after Task 3, the complete
  standalone-versus-vendor delta at checkpoint `2c50cd0a`, and refinement
  commits `eccd5c5` and `23aa4ac`.
- Produces: current upstream renderer with `WgpuContext::from_external`, `WgpuHeadlessRenderer::from_external`, `render_scene_to_view`, and external `TextureView` images preserved.

- [ ] **Step 1: Copy upstream renderer-owned source files**

Copy every file from `vendor/gpui-box/crates/gpui_wgpu/src` except
`gpui_wgpu.rs` to `src`. Do not delete the standalone-only `wgpu_host.rs` or
`wgpu_image.rs`.

Run:

```powershell
$upstreamSrc = 'vendor/gpui-box/crates/gpui_wgpu/src'
Get-ChildItem -LiteralPath $upstreamSrc -File |
  Where-Object Name -ne 'gpui_wgpu.rs' |
  Copy-Item -Destination src -Force
```

- [ ] **Step 2: Reapply the complete standalone renderer delta**

Before applying individual refinements, compare the old standalone renderer
with its old vendored source. This is required because `WgpuContext::from_external`
originated in the initial standalone import (`ecc9306f`), not in `eccd5c5`:

```powershell
git diff 2c50cd0a:vendor/gpui-box/crates/gpui_wgpu/src/wgpu_context.rs 2c50cd0a:src/wgpu_context.rs
git diff 2c50cd0a:vendor/gpui-box/crates/gpui_wgpu/src/wgpu_renderer.rs 2c50cd0a:src/wgpu_renderer.rs
```

Port every still-applicable integration point onto the refreshed source,
including `WgpuContext::from_external`, the `host` feature gates,
`new_headless_with_format`, and the public `WgpuHeadlessRenderer` direct-view
API. Then apply the later renderer refinements:

Run:

```powershell
$hostRendererPatch = @('src/cosmic_text_system.rs', 'src/wgpu_atlas.rs', 'src/wgpu_context.rs', 'src/wgpu_renderer.rs')
git show --format= eccd5c5 -- $hostRendererPatch | git apply --3way
git show --format= 23aa4ac -- src/wgpu_renderer.rs | git apply --3way
```

Expected: `rg -n "from_external|render_scene_to_view|create_external_image_bind_groups" src` finds all three retained integration points.

- [ ] **Step 3: Reconcile the standalone manifest**

Do not copy the upstream manifest because its package and dependency entries
use workspace inheritance. The frozen renderer uses the same runtime
dependency set already present at the root: `anyhow`, `bytemuck`,
`collections`, `cosmic-text`, `etagere`, `font-kit`, `gpui`, `gpui_util`,
`image`, `itertools`, `js-sys`, `log`, `parking_lot`, `pollster`, `profiling`,
`raw-window-handle`, `smallvec`, `swash`, `unicode-bidi`,
`unicode-segmentation`, `wasm-bindgen`, `wasm-bindgen-futures`, `web-sys`, and
`wgpu`. Preserve root-only `gpui-kit`, `host`, `test-support`, and `kit`
entries.

Verify that assumption rather than rewriting versions:

```powershell
cargo check --no-default-features
cargo check --features host
cargo check --features kit
```

Expected: all three checks pass with no `Cargo.toml` edit. Cargo may refresh
`Cargo.lock` because the vendored packages changed; inspect that diff and keep
only the resolver output caused by the frozen upstream packages.

- [ ] **Step 4: Verify standalone compatibility**

Run:

```powershell
cargo test --features host --test default_external_api
cargo test --features host --test wgpu_host_gpu -j 1 -- --nocapture
cargo check --features kit
cargo check --target wasm32-unknown-unknown --no-default-features
```

Expected: external device/queue identity, direct target rendering, external images, WgpuHost, kit, and WASM all remain green.

- [ ] **Step 5: Commit the renderer refresh**

```powershell
git add Cargo.toml Cargo.lock src
git commit -m "build: refresh standalone WGPU renderer"
```

---

### Task 5: Add enforceable vendor-drift detection

**Files:**
- Create: `scripts/check_vendor_drift.py`
- Modify: `vendor/gpui-box/FORK_PATCHES.md`
- Test: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: frozen revision and allowed paths from `FORK_PATCHES.md`.
- Produces: `python scripts/check_vendor_drift.py`, which exits zero only when every non-allowlisted vendored file matches the frozen upstream archive.

- [ ] **Step 1: Write the failing drift check fixture test**

Implement the script around pure helpers `sha256(path)`, `relative_files(root)`, and `unexpected_drift(upstream_root, vendor_root, allowed)`. Add `scripts/test_check_vendor_drift.py` with:

```python
from pathlib import Path
from tempfile import TemporaryDirectory
from scripts.check_vendor_drift import unexpected_drift


def test_only_allowlisted_differences_are_accepted():
    with TemporaryDirectory() as directory:
        root = Path(directory)
        upstream = root / "upstream"
        vendor = root / "vendor"
        upstream.mkdir()
        vendor.mkdir()
        (upstream / "same.rs").write_text("same", encoding="utf-8")
        (vendor / "same.rs").write_text("same", encoding="utf-8")
        (upstream / "patched.rs").write_text("old", encoding="utf-8")
        (vendor / "patched.rs").write_text("new", encoding="utf-8")

        assert unexpected_drift(upstream, vendor, {"patched.rs"}) == []

        (vendor / "same.rs").write_text("drift", encoding="utf-8")
        assert unexpected_drift(upstream, vendor, {"patched.rs"}) == ["same.rs"]
```

- [ ] **Step 2: Run the fixture and verify RED**

Run:

```powershell
python -m unittest scripts/test_check_vendor_drift.py
```

Expected: import fails because `check_vendor_drift.py` does not exist.

- [ ] **Step 3: Implement the minimum checker**

The script must:

1. read the exact `Revision:` and allowlist bullets from `vendor/gpui-box/FORK_PATCHES.md`;
2. download `https://github.com/fran0220/gpui-box/archive/<revision>.tar.gz` with `urllib.request`;
3. read archive members directly instead of extracting them;
4. hash regular members and vendored files with `hashlib.sha256`, treating an
   archive symlink as the UTF-8 bytes of its link target because Git with
   `core.symlinks=false` materializes that target as an ordinary file on Windows;
5. ignore only the exact allowlist paths;
6. print sorted missing, extra, or changed paths and exit one when drift exists.

`unexpected_drift` compares the union of relative file paths and returns sorted POSIX-style names whose presence or digest differs and which are not allowlisted. Do not add PyPI dependencies or wildcard exclusions.

- [ ] **Step 4: Verify the fixture and real tree**

Run:

```powershell
python -m unittest scripts/test_check_vendor_drift.py
python scripts/check_vendor_drift.py
```

Expected: both exit zero. If the real check reports a path, either restore it to upstream or document that exact path and purpose in the allowlist; never add a directory wildcard.

- [ ] **Step 5: Commit the drift gate**

```powershell
git add scripts/check_vendor_drift.py scripts/test_check_vendor_drift.py vendor/gpui-box/FORK_PATCHES.md
git commit -m "ci: detect unreviewed GPUI vendor drift"
```

---

### Task 6: Freeze provenance and verify the resynchronized base

**Files:**
- Modify: `UPSTREAM.md`
- Modify: `vendor/gpui-box/UPSTREAM.md`
- Modify: `Cargo.lock` only if Cargo generated a legitimate lockfile update.

**Interfaces:**
- Consumes: green outputs from Tasks 1-5.
- Produces: the frozen, documented base used by the next multi-window implementation plan.

- [ ] **Step 1: Update both provenance documents**

Set the revision in both files to `ab8f37f6cbdee575f78cd4564597e9ae4d44e65c`. In root `UPSTREAM.md`, replace the statement that `host` intentionally uses `gpui/test-support` with: production compatibility still temporarily uses it at this checkpoint, and the multi-window runtime phase removes it only after `WgpuHost` delegates to `EmbeddedPlatform`.

Record these retained fork capabilities explicitly:

- external caller GPU construction;
- direct `TextureView` rendering;
- backend-neutral external images and `WgpuImage`;
- GPUI Box Kit reexport;
- SDL3 keyboard, pointer, UTF-8 text/preedit, file drop, clipboard, and cursor bridge.

- [ ] **Step 2: Run the full frozen-base verification**

Run:

```powershell
cargo fmt --all --check
cargo test --workspace --all-features -j 1
cargo clippy --workspace --all-targets --all-features -j 1 -- -D warnings
cargo doc --workspace --all-features --no-deps
cargo check --no-default-features
cargo check --features host
cargo check --features kit
cargo check -p gpui-box-sdl
cargo check --target wasm32-unknown-unknown --no-default-features
python -m unittest scripts/test_check_vendor_drift.py
python scripts/check_vendor_drift.py
git diff --check
```

Expected: every command exits zero. Do not accept ignored tests, weakened assertions, post-device GPU skips, or a broadened drift allowlist as substitutes.

- [ ] **Step 3: Confirm forbidden regressions are absent**

Run:

```powershell
rg -n "pub struct WgpuHost|pub struct WgpuImage|from_external|render_scene_to_view" src
rg -n "SdlHostEvent|SdlInputAdapter|SdlPlatformBridge|clipboard_text|set_clipboard_text" crates/gpui-box-sdl/src
git status --short
```

Expected: every public integration point is present and only the two provenance files or a legitimate lockfile refresh remain uncommitted.

- [ ] **Step 4: Commit the frozen base**

```powershell
git add UPSTREAM.md vendor/gpui-box/UPSTREAM.md Cargo.lock
git commit -m "docs: freeze resynchronized GPUI base"
```

- [ ] **Step 5: Stop for the architectural checkpoint**

Write the next implementation plan against this exact tree. Its first tasks must close raw-handle, generational window ownership, request-close, routed SDL lifecycle, and wake/redraw contracts before splitting renderer ownership. Do not reuse the deleted HeadlessAppContext-based plan.
