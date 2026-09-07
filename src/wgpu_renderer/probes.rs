pub(super) struct ProbeInflight {
    pub buffer: wgpu::Buffer,
    pub requests: Vec<u32>,
    pub bgra: bool,
    pub map_result: std::sync::Arc<std::sync::Mutex<Option<Result<(), String>>>>,
}

/// One probe sample's stride in the readback buffer. A texel is 4 bytes; the
/// rest is the copy offset alignment the downlevel backends ask for.
pub(super) const PROBE_SAMPLE_STRIDE: usize = 256;
