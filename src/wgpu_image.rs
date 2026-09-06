use std::sync::Arc;

pub(crate) struct WgpuImagePayload {
    pub(crate) view: wgpu::TextureView,
}

fn validate_texture_properties(
    size: wgpu::Extent3d,
    dimension: wgpu::TextureDimension,
    sample_count: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        size.width > 0 && size.height > 0,
        "WgpuImage dimensions must be non-zero"
    );
    anyhow::ensure!(
        dimension == wgpu::TextureDimension::D2,
        "WgpuImage requires a 2D texture"
    );
    anyhow::ensure!(
        size.depth_or_array_layers == 1,
        "WgpuImage requires exactly one layer"
    );
    anyhow::ensure!(
        sample_count == 1,
        "WgpuImage does not support multisampled textures"
    );
    anyhow::ensure!(
        usage.contains(wgpu::TextureUsages::TEXTURE_BINDING),
        "WgpuImage texture usage must contain TEXTURE_BINDING"
    );
    anyhow::ensure!(
        matches!(
            format.sample_type(None, None),
            Some(wgpu::TextureSampleType::Float { filterable: true })
        ),
        "WgpuImage format {format:?} must be filterable float"
    );
    Ok(())
}

/// A live, engine-owned WGPU texture view that can be passed to [`gpui::img`].
#[derive(Clone)]
pub struct WgpuImage(Arc<gpui::ExternalImageHandle>);

impl WgpuImage {
    /// Wraps a full-resource 2D texture view without copying its pixels.
    pub fn new(view: wgpu::TextureView) -> anyhow::Result<Self> {
        let texture = view.texture();
        let size = texture.size();
        validate_texture_properties(
            size,
            texture.dimension(),
            texture.sample_count(),
            texture.format(),
            texture.usage(),
        )?;
        Ok(Self(Arc::new(gpui::ExternalImageHandle::new(
            gpui::size(
                gpui::DevicePixels(size.width as i32),
                gpui::DevicePixels(size.height as i32),
            ),
            WgpuImagePayload { view },
        ))))
    }

    /// Returns the intrinsic physical size of the underlying texture.
    pub fn size(&self) -> gpui::Size<gpui::DevicePixels> {
        self.0.size()
    }
}

impl From<WgpuImage> for gpui::ImageSource {
    fn from(image: WgpuImage) -> Self {
        gpui::ImageSource::External(image.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_zero_dimensions_before_wgpu_view_creation() {
        let error = validate_texture_properties(
            wgpu::Extent3d {
                width: 0,
                height: 16,
                depth_or_array_layers: 1,
            },
            wgpu::TextureDimension::D2,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING,
        )
        .unwrap_err();

        assert!(error.to_string().contains("non-zero"));
    }
}
