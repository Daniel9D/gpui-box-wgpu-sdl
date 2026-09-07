#![cfg(all(not(target_family = "wasm"), feature = "host"))]

use std::sync::Arc;

use gpui::{AppContext, Context, IntoElement, Render, Window, div, px};
use gpui_wgpu::{CosmicTextSystem, ExternalGpu, WgpuRuntime};

struct EmptyView;

impl Render for EmptyView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[test]
fn closing_one_runtime_window_does_not_close_its_sibling() {
    let Some(gpu) = gpu() else { return };
    let mut runtime = WgpuRuntime::new(
        gpu,
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
    )
    .unwrap();
    let (first, _) = runtime
        .open_window(gpui::size(px(320.0), px(180.0)), |_, cx| {
            cx.new(|_| EmptyView)
        })
        .unwrap();
    let (second, _) = runtime
        .open_window(gpui::size(px(640.0), px(360.0)), |_, cx| {
            cx.new(|_| EmptyView)
        })
        .unwrap();

    runtime.close_window(first).unwrap();
    assert!(runtime.window_state(first).is_err());
    assert!(runtime.window_state(second).is_ok());
}

fn gpu() -> Option<ExternalGpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    }))
    .ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("runtime_lifecycle_test"),
        required_features: wgpu::Features::empty(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    }))
    .ok()?;
    Some(ExternalGpu {
        instance,
        adapter,
        device: Arc::new(device),
        queue: Arc::new(queue),
    })
}
