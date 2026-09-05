#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub origin_x: f32,
    pub origin_y: f32,
    pub scale: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            scale: 1.0,
        }
    }
}

pub struct SdlInputAdapter {
    viewport: Viewport,
}

impl SdlInputAdapter {
    pub fn new(viewport: Viewport) -> anyhow::Result<Self> {
        validate_viewport(viewport)?;
        Ok(Self { viewport })
    }

    pub fn set_viewport(&mut self, viewport: Viewport) -> anyhow::Result<()> {
        validate_viewport(viewport)?;
        self.viewport = viewport;
        Ok(())
    }

    pub fn map_position(&self, x: f32, y: f32) -> gpui::Point<gpui::Pixels> {
        gpui::point(
            gpui::px((x - self.viewport.origin_x) / self.viewport.scale),
            gpui::px((y - self.viewport.origin_y) / self.viewport.scale),
        )
    }
}

fn validate_viewport(viewport: Viewport) -> anyhow::Result<()> {
    anyhow::ensure!(
        viewport.origin_x.is_finite() && viewport.origin_y.is_finite(),
        "viewport origin must be finite"
    );
    anyhow::ensure!(
        viewport.scale.is_finite() && viewport.scale > 0.0,
        "viewport scale must be positive and finite"
    );
    Ok(())
}
