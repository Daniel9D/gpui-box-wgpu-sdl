use std::{cell::RefCell, rc::Rc, sync::Arc};

use crate::WgpuHeadlessRenderer;
use anyhow::Context as _;

pub struct ExternalGpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
}

#[derive(Clone, Copy, Debug)]
struct HostFrame {
    device_size: gpui::Size<gpui::DevicePixels>,
    logical_size: gpui::Size<gpui::Pixels>,
}

impl HostFrame {
    fn new(extent: wgpu::Extent3d, scale_factor: f32) -> anyhow::Result<Self> {
        anyhow::ensure!(
            extent.width > 0 && extent.height > 0,
            "target dimensions must be non-zero"
        );
        anyhow::ensure!(
            extent.depth_or_array_layers == 1,
            "target must have exactly one layer"
        );
        anyhow::ensure!(
            scale_factor.is_finite() && scale_factor > 0.0,
            "scale factor must be positive and finite"
        );

        let width = i32::try_from(extent.width).context("target width exceeds i32")?;
        let height = i32::try_from(extent.height).context("target height exceeds i32")?;
        let logical_width = extent.width as f32 / scale_factor;
        let logical_height = extent.height as f32 / scale_factor;
        anyhow::ensure!(
            logical_width.is_finite() && logical_height.is_finite(),
            "logical target dimensions exceed GPUI's pixel range"
        );

        Ok(Self {
            device_size: gpui::size(gpui::DevicePixels(width), gpui::DevicePixels(height)),
            logical_size: gpui::size(gpui::px(logical_width), gpui::px(logical_height)),
        })
    }
}

fn validate_logical_size(size: gpui::Size<gpui::Pixels>) -> anyhow::Result<()> {
    let width = size.width.as_f32();
    let height = size.height.as_f32();
    anyhow::ensure!(
        width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0,
        "logical window dimensions must be positive and finite"
    );
    Ok(())
}

struct CurrentTarget {
    view: wgpu::TextureView,
    size: gpui::Size<gpui::DevicePixels>,
}

struct DirectRendererState {
    renderer: WgpuHeadlessRenderer,
    target: Option<CurrentTarget>,
    last_error: Option<String>,
}

impl DirectRendererState {
    fn begin(&mut self, view: &wgpu::TextureView, size: gpui::Size<gpui::DevicePixels>) {
        self.last_error = None;
        self.target = Some(CurrentTarget {
            view: view.clone(),
            size,
        });
    }

    fn finish(&mut self) -> Option<String> {
        self.target = None;
        self.last_error.take()
    }
}

struct DirectRenderer {
    state: Rc<RefCell<DirectRendererState>>,
}

impl DirectRenderer {
    fn render_direct(
        &mut self,
        scene: &gpui::Scene,
        _platform_size: gpui::Size<gpui::DevicePixels>,
    ) -> anyhow::Result<()> {
        let mut state = self.state.borrow_mut();
        let Some(target) = &state.target else {
            let error = "GPUI attempted to draw without a host texture target".to_owned();
            state.last_error = Some(error.clone());
            anyhow::bail!(error);
        };
        let view = target.view.clone();
        let size = target.size;
        let result = state.renderer.render_scene_to_view(scene, size, &view);
        if let Err(error) = &result {
            state.last_error = Some(format!("{error:#}"));
        }
        result
    }
}

impl gpui::PlatformHeadlessRenderer for DirectRenderer {
    fn render_scene_to_image(
        &mut self,
        scene: &gpui::Scene,
        size: gpui::Size<gpui::DevicePixels>,
    ) -> anyhow::Result<image::RgbaImage> {
        self.render_direct(scene, size)?;
        Ok(image::RgbaImage::new(0, 0))
    }

    fn render_scene(
        &mut self,
        scene: &gpui::Scene,
        size: gpui::Size<gpui::DevicePixels>,
    ) -> anyhow::Result<()> {
        self.render_direct(scene, size)
    }

    fn sprite_atlas(&self) -> Arc<dyn gpui::PlatformAtlas> {
        self.state.borrow().renderer.sprite_atlas()
    }

    fn backdrop_luminance(&mut self, slot: u32) -> Option<f32> {
        self.state.borrow_mut().renderer.backdrop_luminance(slot)
    }
}

pub struct WgpuHost {
    context: gpui::HeadlessAppContext,
    window: gpui::AnyWindowHandle,
    renderer: Rc<RefCell<DirectRendererState>>,
    logical_size: gpui::Size<gpui::Pixels>,
    scale_factor: Option<f32>,
}

impl WgpuHost {
    pub fn new<V: gpui::Render + 'static>(
        gpu: ExternalGpu,
        target_format: wgpu::TextureFormat,
        initial_size: gpui::Size<gpui::Pixels>,
        text_system: Arc<dyn gpui::PlatformTextSystem>,
        asset_source: Arc<dyn gpui::AssetSource>,
        build_root: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> gpui::Entity<V>,
    ) -> anyhow::Result<Self> {
        validate_logical_size(initial_size)?;
        let renderer = WgpuHeadlessRenderer::from_external(
            gpu.instance,
            gpu.adapter,
            gpu.device,
            gpu.queue,
            target_format,
        )?;
        let renderer = Rc::new(RefCell::new(DirectRendererState {
            renderer,
            target: None,
            last_error: None,
        }));
        let factory_renderer = Rc::clone(&renderer);
        let mut context =
            gpui::HeadlessAppContext::with_platform(text_system, asset_source, move || {
                Some(Box::new(DirectRenderer {
                    state: Rc::clone(&factory_renderer),
                }))
            });
        let window = context.open_window(initial_size, build_root)?.into();

        Ok(Self {
            context,
            window,
            renderer,
            logical_size: initial_size,
            scale_factor: None,
        })
    }

    pub fn dispatch(
        &mut self,
        input: gpui::PlatformInput,
    ) -> anyhow::Result<gpui::DispatchEventResult> {
        let result = self.context.update_window(self.window, |_, window, cx| {
            window.dispatch_event(input, cx)
        })?;
        self.context.run_until_parked();
        Ok(result)
    }

    pub fn dispatch_text(&mut self, text: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!text.is_empty(), "committed text must not be empty");
        let keystroke = gpui::Keystroke {
            modifiers: gpui::Modifiers::default(),
            key: String::new(),
            key_char: Some(text.to_owned()),
        };
        self.context.update_window(self.window, |_, window, cx| {
            window.dispatch_keystroke(keystroke, cx);
        })?;
        self.context.run_until_parked();
        Ok(())
    }

    pub fn render_to_view(
        &mut self,
        target: &wgpu::TextureView,
        physical_size: wgpu::Extent3d,
        scale_factor: f32,
    ) -> anyhow::Result<()> {
        let frame = HostFrame::new(physical_size, scale_factor)?;
        self.renderer.borrow_mut().begin(target, frame.device_size);
        let draw_result = self.draw_frame(frame.logical_size, scale_factor);
        let render_error = self.renderer.borrow_mut().finish();
        draw_result?;
        if let Some(error) = render_error {
            anyhow::bail!(error);
        }
        Ok(())
    }

    fn draw_frame(
        &mut self,
        logical_size: gpui::Size<gpui::Pixels>,
        scale_factor: f32,
    ) -> anyhow::Result<()> {
        let logical_changed = self.logical_size != logical_size;
        let scale_changed = self.scale_factor != Some(scale_factor);
        let result = self.context.update_window(self.window, |_, window, cx| {
            if logical_changed {
                window.resize(logical_size);
                window.bounds_changed(cx);
            }
            if logical_changed || scale_changed {
                window.set_scale_factor(scale_factor);
            }
            window.draw(cx).clear(cx);
            window.render_to_image().map(drop)
        })?;
        self.logical_size = logical_size;
        self.scale_factor = Some(scale_factor);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_physical_extent_to_logical_size() {
        let frame = HostFrame::new(
            wgpu::Extent3d {
                width: 300,
                height: 150,
                depth_or_array_layers: 1,
            },
            1.5,
        )
        .unwrap();

        assert_eq!(
            frame.device_size,
            gpui::size(gpui::DevicePixels(300), gpui::DevicePixels(150))
        );
        assert_eq!(
            frame.logical_size,
            gpui::size(gpui::px(200.0), gpui::px(100.0))
        );
    }

    #[test]
    fn rejects_invalid_frame_geometry() {
        let extent = wgpu::Extent3d {
            width: 300,
            height: 150,
            depth_or_array_layers: 2,
        };
        assert!(HostFrame::new(extent, 1.0).is_err());

        let flat = wgpu::Extent3d {
            depth_or_array_layers: 1,
            ..extent
        };
        assert!(HostFrame::new(wgpu::Extent3d { width: 0, ..flat }, 1.0).is_err());
        assert!(HostFrame::new(flat, 0.0).is_err());
        assert!(HostFrame::new(flat, f32::NAN).is_err());
    }

    #[test]
    fn rejects_device_and_logical_size_overflow() {
        let too_wide = wgpu::Extent3d {
            width: i32::MAX as u32 + 1,
            height: 1,
            depth_or_array_layers: 1,
        };
        assert!(HostFrame::new(too_wide, 1.0).is_err());

        let logical_overflow = wgpu::Extent3d {
            width: i32::MAX as u32,
            height: 1,
            depth_or_array_layers: 1,
        };
        assert!(HostFrame::new(logical_overflow, f32::MIN_POSITIVE).is_err());
    }

    #[test]
    fn rejects_invalid_initial_logical_size() {
        assert!(validate_logical_size(gpui::size(gpui::px(0.0), gpui::px(1.0))).is_err());
        assert!(validate_logical_size(gpui::size(gpui::px(f32::INFINITY), gpui::px(1.0))).is_err());
    }
}
