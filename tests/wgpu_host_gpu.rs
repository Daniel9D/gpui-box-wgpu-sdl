#![cfg(all(not(target_family = "wasm"), feature = "host"))]

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};

use gpui::{App, Context, FocusHandle, IntoElement, Render, Window, div, prelude::*, px, rgb};
use gpui_wgpu::{CosmicTextSystem, ExternalGpu, WgpuHost};

const EXTENT: wgpu::Extent3d = wgpu::Extent3d {
    width: 256,
    height: 128,
    depth_or_array_layers: 1,
};

#[derive(Clone, Default)]
struct ProbeState {
    mouse_moved: Rc<Cell<bool>>,
    text: Rc<RefCell<String>>,
}

struct ProbeView {
    focus: FocusHandle,
    state: ProbeState,
}

impl Render for ProbeView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mouse_moved = Rc::clone(&self.state.mouse_moved);
        let text = Rc::clone(&self.state.text);
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_mouse_move(move |_, _, _| mouse_moved.set(true))
            .on_key_down(move |event, _, _| {
                if let Some(value) = &event.keystroke.key_char {
                    text.borrow_mut().push_str(value);
                }
            })
            .flex()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .bg(rgb(0x172033))
            .child(div().size(px(32.0)).bg(rgb(0xef4444)))
            .child(div().size(px(32.0)).bg(rgb(0x3b82f6)))
    }
}

struct Fixture {
    host: WgpuHost,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    state: ProbeState,
}

#[test]
fn renders_same_device_pixels_at_scale_one() {
    let _gpu = gpu_test_guard();
    let Some(mut fixture) = fixture() else {
        return;
    };

    fixture
        .host
        .render_to_view(&fixture.view, EXTENT, 1.0)
        .unwrap();

    let rgba = read_texture(&fixture.device, &fixture.queue, &fixture.texture, EXTENT);
    assert_color_near(pixel(&rgba, EXTENT.width, 8, 8), [0x17, 0x20, 0x33, 0xff]);
    assert_color_near(
        pixel(&rgba, EXTENT.width, 100, 64),
        [0xef, 0x44, 0x44, 0xff],
    );
    assert_color_near(
        pixel(&rgba, EXTENT.width, 150, 64),
        [0x3b, 0x82, 0xf6, 0xff],
    );
}

#[test]
fn honors_hidpi_scale_in_scene_geometry() {
    let _gpu = gpu_test_guard();
    let Some(mut fixture) = fixture() else {
        return;
    };

    fixture
        .host
        .render_to_view(&fixture.view, EXTENT, 1.0)
        .unwrap();
    let scale_one = read_texture(&fixture.device, &fixture.queue, &fixture.texture, EXTENT);
    fixture
        .host
        .render_to_view(&fixture.view, EXTENT, 2.0)
        .unwrap();
    let scale_two = read_texture(&fixture.device, &fixture.queue, &fixture.texture, EXTENT);

    assert_color_near(
        pixel(&scale_one, EXTENT.width, 60, 64),
        [0x17, 0x20, 0x33, 0xff],
    );
    assert_color_near(
        pixel(&scale_two, EXTENT.width, 60, 64),
        [0xef, 0x44, 0x44, 0xff],
    );
    assert_color_near(
        pixel(&scale_two, EXTENT.width, 176, 64),
        [0x3b, 0x82, 0xf6, 0xff],
    );
}

#[test]
fn recovers_after_renderer_rejects_oversized_frame() {
    let _gpu = gpu_test_guard();
    let Some(mut fixture) = fixture() else {
        return;
    };
    let oversized = wgpu::Extent3d {
        width: fixture.device.limits().max_texture_dimension_2d + 1,
        ..EXTENT
    };

    assert!(
        fixture
            .host
            .render_to_view(&fixture.view, oversized, 1.0)
            .is_err()
    );
    fixture
        .host
        .render_to_view(&fixture.view, EXTENT, 1.0)
        .unwrap();

    let rgba = read_texture(&fixture.device, &fixture.queue, &fixture.texture, EXTENT);
    assert_color_near(
        pixel(&rgba, EXTENT.width, 100, 64),
        [0xef, 0x44, 0x44, 0xff],
    );
}

#[test]
fn dispatches_pointer_keyboard_and_committed_text() {
    let _gpu = gpu_test_guard();
    let Some(mut fixture) = fixture() else {
        return;
    };
    fixture
        .host
        .render_to_view(&fixture.view, EXTENT, 1.0)
        .unwrap();

    let result = fixture
        .host
        .dispatch(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
            position: gpui::point(gpui::px(12.0), gpui::px(12.0)),
            pressed_button: None,
            modifiers: gpui::Modifiers::default(),
        }))
        .unwrap();
    assert!(result.propagate);
    assert!(fixture.state.mouse_moved.get());

    fixture
        .host
        .dispatch(gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers: gpui::Modifiers::default(),
                key: "x".into(),
                key_char: Some("x".into()),
            },
            is_held: false,
            prefer_character_input: false,
        }))
        .unwrap();
    fixture.host.dispatch_text("olá").unwrap();

    assert_eq!(&*fixture.state.text.borrow(), "xolá");
    assert!(fixture.host.dispatch_text("").is_err());
}

fn fixture() -> Option<Fixture> {
    let _ = env_logger::builder().is_test(true).try_init();
    let (instance, adapter, device, queue) = gpu()?;
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let texture_format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let view_format = texture_format.remove_srgb_suffix();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("wgpu_host_integration_target"),
        size: EXTENT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[view_format],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(view_format),
        ..Default::default()
    });
    let state = ProbeState::default();
    let root_state = state.clone();
    let host = WgpuHost::new(
        ExternalGpu {
            instance,
            adapter,
            device: Arc::clone(&device),
            queue: Arc::clone(&queue),
        },
        view_format,
        gpui::size(gpui::px(256.0), gpui::px(128.0)),
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
        move |window, cx: &mut App| {
            let view = cx.new(|cx| ProbeView {
                focus: cx.focus_handle(),
                state: root_state,
            });
            let focus = view.read(cx).focus.clone();
            focus.focus(window, cx);
            view
        },
    )
    .unwrap();

    Some(Fixture {
        host,
        device,
        queue,
        texture,
        view,
        state,
    })
}

fn gpu() -> Option<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("skipping WgpuHost GPU test: no compatible adapter: {error}");
            return None;
        }
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("wgpu_host_test_device"),
        required_features: wgpu::Features::empty(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    }))
    .expect("request WgpuHost test device after acquiring an adapter");
    Some((instance, adapter, device, queue))
}

fn gpu_test_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn assert_color_near(actual: [u8; 4], expected: [u8; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!(actual.abs_diff(expected) <= 1, "{actual} != {expected}");
    }
}

fn pixel(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y as usize * width as usize) + x as usize) * 4;
    rgba[offset..offset + 4].try_into().unwrap()
}

fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    extent: wgpu::Extent3d,
) -> Vec<u8> {
    let unpadded_bytes_per_row = extent.width * 4;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("wgpu_host_test_readback"),
        size: u64::from(padded_bytes_per_row) * u64::from(extent.height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(extent.height),
            },
        },
        extent,
    );
    queue.submit([encoder.finish()]);

    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .clone()
        .map_async(wgpu::MapMode::Read, .., move |result| {
            sender.send(result).unwrap();
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();

    let mapped = buffer.get_mapped_range(..).unwrap();
    let mut result = Vec::with_capacity((unpadded_bytes_per_row * extent.height) as usize);
    for row in mapped.chunks_exact(padded_bytes_per_row as usize) {
        result.extend_from_slice(&row[..unpadded_bytes_per_row as usize]);
    }
    drop(mapped);
    buffer.unmap();
    result
}
