#![cfg(all(not(target_family = "wasm"), feature = "host"))]

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use gpui::{
    AppContext, Context, Corners, GlassMaterial, IntoElement, ParentElement, Render, Styled,
    Window, canvas, div, px,
};
use gpui_wgpu::{CosmicTextSystem, ExternalGpu, WgpuRuntime, WgpuWindowError};

struct EmptyView;

impl Render for EmptyView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(gpui::rgb(0xff0000))
    }
}

struct GreenView;

impl Render for GreenView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(gpui::rgb(0x00ff00))
    }
}

struct LuminanceProbeView;

impl Render for LuminanceProbeView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(gpui::rgb(0xffffff)).child(
            canvas(
                |_, _, _| (),
                |bounds, (), window, _| {
                    let mut material = GlassMaterial::clear();
                    material.probe = 0;
                    window.paint_backdrop_glass(bounds, Corners::all(px(0.0)), material, &[]);
                },
            )
            .size_full(),
        )
    }
}

#[test]
fn runtime_renders_two_windows_into_independent_caller_texture_views() {
    let _gpu_guard = gpu_test_guard();
    let Some(gpu) = gpu() else { return };
    let device = gpu.device.clone();
    let queue = gpu.queue.clone();
    let mut runtime = WgpuRuntime::new(
        gpu,
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
    )
    .unwrap();
    let (window, _) = runtime
        .open_window(gpui::size(px(64.0), px(64.0)), |_, cx| {
            cx.new(|_| EmptyView)
        })
        .unwrap();
    let (green_window, _) = runtime
        .open_window(gpui::size(px(32.0), px(48.0)), |_, cx| {
            cx.new(|_| GreenView)
        })
        .unwrap();
    #[cfg(feature = "test-support")]
    assert!(
        runtime
            .windows_share_renderer_resources(window, green_window)
            .unwrap(),
        "compatible runtime windows should share pipelines and layouts"
    );
    let extent = wgpu::Extent3d {
        width: 64,
        height: 64,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("runtime_direct_target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });

    runtime
        .render_window(
            window,
            &texture.create_view(&Default::default()),
            extent,
            1.0,
        )
        .unwrap();

    let green_extent = wgpu::Extent3d {
        width: 32,
        height: 48,
        depth_or_array_layers: 1,
    };
    let green_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("runtime_direct_green_target"),
        size: green_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    runtime
        .render_window(
            green_window,
            &green_texture.create_view(&Default::default()),
            green_extent,
            1.0,
        )
        .unwrap();

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("runtime_direct_readback"),
        size: 256 * 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        extent,
    );
    let green_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("runtime_direct_green_readback"),
        size: 256 * 48,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        green_texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &green_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(48),
            },
        },
        green_extent,
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    let (green_tx, green_rx) = std::sync::mpsc::channel();
    green_buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            green_tx.send(result).unwrap()
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    rx.recv().unwrap().unwrap();
    green_rx.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap();
    assert!(bytes[0] > 240 && bytes[1] < 10 && bytes[2] < 10 && bytes[3] > 240);
    let green_bytes = green_buffer.slice(..).get_mapped_range().unwrap();
    assert!(
        green_bytes[0] < 10 && green_bytes[1] > 240 && green_bytes[2] < 10 && green_bytes[3] > 240
    );
}

#[test]
fn closing_one_runtime_window_does_not_close_its_sibling() {
    let _gpu_guard = gpu_test_guard();
    let Some(gpu) = gpu() else { return };
    let sibling_runtime_gpu = ExternalGpu {
        instance: gpu.instance.clone(),
        adapter: gpu.adapter.clone(),
        device: gpu.device.clone(),
        queue: gpu.queue.clone(),
    };
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
    assert!(matches!(
        runtime.window_state(first).unwrap_err().downcast_ref(),
        Some(WgpuWindowError::Closed)
    ));
    assert!(runtime.window_state(second).is_ok());
    let size = runtime
        .update_window(second, |window, _| window.viewport_size())
        .unwrap();
    assert_eq!(size, gpui::size(px(640.0), px(360.0)));

    let (replacement, _) = runtime
        .open_window(gpui::size(px(160.0), px(90.0)), |_, cx| {
            cx.new(|_| EmptyView)
        })
        .unwrap();
    assert_ne!(first, replacement);

    let other_runtime = WgpuRuntime::new(
        sibling_runtime_gpu,
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
    )
    .unwrap();
    assert!(matches!(
        other_runtime
            .window_state(second)
            .unwrap_err()
            .downcast_ref(),
        Some(WgpuWindowError::ForeignRuntime)
    ));
}

#[test]
fn realtime_runtime_completes_luminance_probes_through_nonblocking_pump() {
    let _gpu_guard = gpu_test_guard();
    let Some(gpu) = gpu() else { return };
    let device = gpu.device.clone();
    let mut runtime = WgpuRuntime::new(
        gpu,
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
    )
    .unwrap();
    let (window, _) = runtime
        .open_window(gpui::size(px(32.0), px(32.0)), |_, cx| {
            cx.new(|_| LuminanceProbeView)
        })
        .unwrap();
    let extent = wgpu::Extent3d {
        width: 32,
        height: 32,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("runtime_realtime_probe_target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    runtime
        .render_window(
            window,
            &texture.create_view(&Default::default()),
            extent,
            1.0,
        )
        .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let luminance = loop {
        if let Some(value) = runtime
            .update_window(window, |window, _| window.backdrop_luminance(0))
            .unwrap()
        {
            break value;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "realtime luminance probe did not complete"
        );
        runtime.pump();
        std::thread::yield_now();
    };
    assert!(luminance > 0.95, "unexpected luminance: {luminance}");
}

fn gpu_test_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
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
