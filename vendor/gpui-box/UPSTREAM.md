# Vendored GPUI Box provenance

- Repository: <https://github.com/fran0220/gpui-box>
- Revision: `ab8f37f6cbdee575f78cd4564597e9ae4d44e65c`
- Snapshot date: 2026-09-06

Every path matches that immutable revision except the exact files documented in
`FORK_PATCHES.md`. The fork patches preserve backend-neutral external images
and temporary compatibility hooks used by the standalone WGPU/SDL host.

Run `python scripts/check_vendor_drift.py` from the repository root to verify
the snapshot and reject undocumented differences.
