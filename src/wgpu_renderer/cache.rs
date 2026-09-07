/// Renderer cache diagnostics available to validation suites. This type is
/// intentionally absent from normal production builds.
#[cfg(feature = "test-support")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WgpuRenderCacheStats {
    pub external_image_bind_group_creations: u64,
    pub external_image_bind_groups: usize,
    pub atlas_bind_group_creations: u64,
    pub atlas_bind_groups: usize,
}
