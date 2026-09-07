#![cfg(all(not(target_family = "wasm"), feature = "host"))]

use std::sync::Arc;

use gpui::{AppContext, Context, IntoElement, Render, Styled, Window, div, px};
use gpui_wgpu::{CosmicTextSystem, ExternalGpu, WgpuRuntime, WgpuWindowError};

struct DetachableView;

impl Render for DetachableView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full()
    }
}

#[test]
fn entity_identity_survives_repeated_detach_and_reattach_cycles() {
    let Some(gpu) = gpu() else { return };
    let mut runtime = WgpuRuntime::new(
        gpu,
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
    )
    .unwrap();
    let size = gpui::size(px(320.0), px(180.0));
    let (first_window, first_entity) = runtime
        .open_window(size, |_, cx| cx.new(|_| DetachableView))
        .unwrap();
    let (second_window, second_entity) = runtime
        .open_window(size, |_, cx| cx.new(|_| DetachableView))
        .unwrap();
    let first_id = first_entity.entity_id();

    let second_entity = runtime
        .detach_window(second_window, &second_entity)
        .unwrap();
    assert!(matches!(
        runtime
            .detach_window(first_window, &second_entity)
            .unwrap_err()
            .downcast_ref(),
        Some(WgpuWindowError::WrongEntity)
    ));

    let entity = runtime.detach_window(first_window, &first_entity).unwrap();
    assert_eq!(entity.entity_id(), first_id);
    assert!(matches!(
        runtime
            .window_state(first_window)
            .unwrap_err()
            .downcast_ref(),
        Some(WgpuWindowError::Closed)
    ));

    let mut window = runtime
        .open_window_with_entity(size, entity.clone())
        .unwrap();
    assert!(matches!(
        runtime
            .open_window_with_entity(size, entity.clone())
            .unwrap_err()
            .downcast_ref(),
        Some(WgpuWindowError::EntityAlreadyAttached)
    ));

    for _ in 0..3 {
        let returned = runtime.detach_window(window, &entity).unwrap();
        assert_eq!(returned.entity_id(), first_id);
        window = runtime
            .open_window_with_entity(size, returned.clone())
            .unwrap();
    }

    runtime.detach_window(window, &entity).unwrap();
    drop(second_entity);
    drop(entity);
    drop(first_entity);
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
        label: Some("runtime_detach_test"),
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
