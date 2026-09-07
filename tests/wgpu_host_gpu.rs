#![cfg(all(not(target_family = "wasm"), feature = "host"))]

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};

use gpui::{App, Context, FocusHandle, IntoElement, Render, Window, div, prelude::*, px, rgb};
use gpui_wgpu::{CosmicTextSystem, ExternalGpu, WgpuHost, WgpuImage};

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

struct ExternalImageView {
    image: gpui::ImageSource,
}

impl Render for ExternalImageView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::img(self.image.clone())
            .size_full()
            .object_fit(gpui::ObjectFit::Fill)
    }
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
fn wgpu_image_validation_accepts_sampled_2d_texture() {
    let _gpu = gpu_test_guard();
    let Some((_, _, device, _)) = gpu() else {
        return;
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("valid_external_image"),
        size: wgpu::Extent3d {
            width: 32,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    let image = WgpuImage::new(texture.create_view(&Default::default())).unwrap();
    assert_eq!(
        image.size(),
        gpui::size(gpui::DevicePixels(32), gpui::DevicePixels(16))
    );
    let _: gpui::ImageSource = image.into();
}

#[test]
fn wgpu_image_validation_rejects_unsupported_texture_properties() {
    let _gpu = gpu_test_guard();
    let Some((_, _, device, _)) = gpu() else {
        return;
    };
    let cases = [
        (
            "one-dimensional",
            wgpu::Extent3d {
                width: 32,
                height: 1,
                depth_or_array_layers: 1,
            },
            wgpu::TextureDimension::D1,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING,
        ),
        (
            "array",
            wgpu::Extent3d {
                width: 32,
                height: 16,
                depth_or_array_layers: 2,
            },
            wgpu::TextureDimension::D2,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING,
        ),
        (
            "multisampled",
            wgpu::Extent3d {
                width: 32,
                height: 16,
                depth_or_array_layers: 1,
            },
            wgpu::TextureDimension::D2,
            4,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        ),
        (
            "not-bindable",
            wgpu::Extent3d {
                width: 32,
                height: 16,
                depth_or_array_layers: 1,
            },
            wgpu::TextureDimension::D2,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::COPY_DST,
        ),
        (
            "integer-format",
            wgpu::Extent3d {
                width: 32,
                height: 16,
                depth_or_array_layers: 1,
            },
            wgpu::TextureDimension::D2,
            1,
            wgpu::TextureFormat::Rgba8Uint,
            wgpu::TextureUsages::TEXTURE_BINDING,
        ),
    ];

    for (label, size, dimension, sample_count, format, usage) in cases {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count,
            dimension,
            format,
            usage,
            view_formats: &[],
        });
        assert!(
            WgpuImage::new(texture.create_view(&Default::default())).is_err(),
            "{label} should be rejected"
        );
    }
}

#[test]
fn external_texture_view_updates_without_recreating_image() {
    let _gpu = gpu_test_guard();
    let Some((instance, adapter, device, queue)) = gpu() else {
        return;
    };
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let source_extent = wgpu::Extent3d {
        width: 32,
        height: 16,
        depth_or_array_layers: 1,
    };
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("engine_owned_external_image"),
        size: source_extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let image = WgpuImage::new(source.create_view(&Default::default())).unwrap();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("external_image_target"),
        size: EXTENT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    let root_image = image.clone();
    let root = Rc::new(RefCell::new(None));
    let built_root = Rc::clone(&root);
    let mut host = WgpuHost::new(
        ExternalGpu {
            instance,
            adapter,
            device: device.clone(),
            queue: queue.clone(),
        },
        wgpu::TextureFormat::Rgba8Unorm,
        gpui::size(px(256.), px(128.)),
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
        move |_, cx| {
            let entity = cx.new(|_| ExternalImageView {
                image: root_image.into(),
            });
            *built_root.borrow_mut() = Some(entity.clone());
            entity
        },
    )
    .unwrap();

    write_solid_texture(&queue, &source, source_extent, [255, 0, 0, 255]);
    host.render_to_view(&target_view, EXTENT, 1.).unwrap();
    let red = read_texture(&device, &queue, &target, EXTENT);
    assert_color_near(pixel(&red, EXTENT.width, 128, 64), [255, 0, 0, 255]);
    #[cfg(feature = "test-support")]
    assert_eq!(
        host.render_cache_stats()
            .external_image_bind_group_creations,
        1
    );

    write_solid_texture(&queue, &source, source_extent, [0, 255, 0, 255]);
    host.render_to_view(&target_view, EXTENT, 1.).unwrap();
    let green = read_texture(&device, &queue, &target, EXTENT);
    assert_color_near(pixel(&green, EXTENT.width, 128, 64), [0, 255, 0, 255]);
    #[cfg(feature = "test-support")]
    {
        let stats = host.render_cache_stats();
        assert_eq!(stats.external_image_bind_group_creations, 1);
        assert_eq!(stats.external_image_bind_groups, 1);

        let replacement = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("replacement_external_image"),
            size: source_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_solid_texture(&queue, &replacement, source_extent, [0, 0, 255, 255]);
        let replacement = WgpuImage::new(replacement.create_view(&Default::default())).unwrap();
        let root = root.borrow().as_ref().unwrap().clone();
        host.update(|_, cx| {
            root.update(cx, |view, cx| {
                view.image = replacement.into();
                cx.notify();
            });
        })
        .unwrap();
        host.render_to_view(&target_view, EXTENT, 1.).unwrap();
        let blue = read_texture(&device, &queue, &target, EXTENT);
        assert_color_near(pixel(&blue, EXTENT.width, 128, 64), [0, 0, 255, 255]);
        let stats = host.render_cache_stats();
        assert_eq!(stats.external_image_bind_group_creations, 2);
        assert_eq!(stats.external_image_bind_groups, 1);
    }
    drop(root.borrow_mut().take());
}

#[test]
fn external_image_from_another_device_returns_an_error() {
    let _gpu = gpu_test_guard();
    let Some((instance, adapter, device, queue)) = gpu() else {
        return;
    };
    let (other_device, _) = pollster::block_on(adapter.request_device(&Default::default()))
        .expect("second device should be available");
    let source = other_device.create_texture(&wgpu::TextureDescriptor {
        label: Some("external_image_from_another_device"),
        size: wgpu::Extent3d {
            width: 32,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let image = WgpuImage::new(source.create_view(&Default::default())).unwrap();
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cross_device_external_image_target"),
        size: EXTENT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    let mut host = WgpuHost::new(
        ExternalGpu {
            instance,
            adapter,
            device,
            queue,
        },
        wgpu::TextureFormat::Rgba8Unorm,
        gpui::size(px(256.), px(128.)),
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
        move |_, cx| {
            cx.new(|_| ExternalImageView {
                image: image.into(),
            })
        },
    )
    .unwrap();

    let error = host
        .render_to_view(&target_view, EXTENT, 1.)
        .expect_err("an external image from another device must be rejected");
    assert!(error.to_string().contains("external image"));
}

#[test]
fn external_image_reports_an_unsupported_renderer_payload() {
    let _gpu = gpu_test_guard();
    let Some((instance, adapter, device, queue)) = gpu() else {
        return;
    };
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("unsupported_external_image_target"),
        size: EXTENT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target_view = target.create_view(&Default::default());
    let image = Arc::new(gpui::ExternalImageHandle::new(
        gpui::size(gpui::DevicePixels(32), gpui::DevicePixels(16)),
        (),
    ));
    let mut host = WgpuHost::new(
        ExternalGpu {
            instance,
            adapter,
            device,
            queue,
        },
        wgpu::TextureFormat::Rgba8Unorm,
        gpui::size(px(256.), px(128.)),
        Arc::new(CosmicTextSystem::new_without_system_fonts("sans-serif")),
        Arc::new(()),
        move |_, cx| {
            cx.new(|_| ExternalImageView {
                image: image.into(),
            })
        },
    )
    .unwrap();

    let error = host
        .render_to_view(&target_view, EXTENT, 1.)
        .expect_err("unknown external payload must be rejected");
    assert!(
        error
            .to_string()
            .contains("unsupported external image payload")
    );
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

#[test]
fn engine_tick_advances_tasks_without_input_events() {
    let _gpu = gpu_test_guard();
    let Some(mut fixture) = fixture() else { return };
    let completed = Rc::new(Cell::new(false));
    let task_completed = completed.clone();
    fixture
        .host
        .update(move |_, cx| {
            let timer = cx
                .background_executor()
                .timer(std::time::Duration::from_millis(50));
            cx.spawn(async move |_| {
                timer.await;
                task_completed.set(true);
            })
            .detach();
        })
        .unwrap();
    fixture.host.tick(std::time::Duration::from_millis(49));
    assert!(!completed.get());
    fixture.host.tick(std::time::Duration::from_millis(1));
    assert!(completed.get());
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

fn write_solid_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    extent: wgpu::Extent3d,
    color: [u8; 4],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &color.repeat((extent.width * extent.height) as usize),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(extent.width * 4),
            rows_per_image: Some(extent.height),
        },
        extent,
    );
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

#[cfg(feature = "kit")]
#[test]
fn kit_input_and_button_use_the_engine_device() {
    use gpui::Focusable;
    use gpui_kit::{controls::input::TextInput, prelude::Button};
    struct KitView {
        input: gpui::Entity<TextInput>,
        clicks: Rc<Cell<u32>>,
    }
    impl Render for KitView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let clicks = self.clicks.clone();
            div()
                .size_full()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    Button::new("button")
                        .label("Apply")
                        .on_click(move |_, _| clicks.set(clicks.get() + 1)),
                )
                .child(self.input.clone())
        }
    }
    let _gpu = gpu_test_guard();
    let Some((instance, adapter, device, queue)) = gpu() else {
        return;
    };
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("engine_owned_kit_target"),
        size: EXTENT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let target = texture.create_view(&Default::default());
    let clicks = Rc::new(Cell::new(0));
    let root_clicks = clicks.clone();
    let input_slot = Rc::new(RefCell::new(None));
    let root_input = input_slot.clone();
    let mut host = WgpuHost::new(
        ExternalGpu {
            instance,
            adapter,
            device: device.clone(),
            queue: queue.clone(),
        },
        wgpu::TextureFormat::Rgba8Unorm,
        gpui::size(px(256.), px(128.)),
        Arc::new(CosmicTextSystem::new("Segoe UI")),
        Arc::new(gpui_kit::assets::Assets),
        move |window, cx| {
            gpui_kit::install(cx);
            let input = cx.new(|cx| TextInput::new("engine.input", window, cx));
            *root_input.borrow_mut() = Some(input.clone());
            cx.new(|_| KitView {
                input,
                clicks: root_clicks,
            })
        },
    )
    .unwrap();
    host.render_to_view(&target, EXTENT, 1.).unwrap();
    host.dispatch(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
        position: gpui::point(px(20.), px(16.)),
        pressed_button: None,
        modifiers: Default::default(),
    }))
    .unwrap();
    host.render_to_view(&target, EXTENT, 1.).unwrap();
    for down in [true, false] {
        let position = gpui::point(px(20.), px(16.));
        let event = if down {
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            })
        } else {
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                button: gpui::MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
            })
        };
        host.dispatch(event).unwrap();
    }
    assert_eq!(clicks.get(), 1);
    let input = input_slot.borrow().clone().unwrap();
    host.update(|window, cx| input.read(cx).focus_handle(cx).focus(window, cx))
        .unwrap();
    host.render_to_view(&target, EXTENT, 1.).unwrap();
    host.dispatch_text("olá engine").unwrap();
    host.update(|_, cx| assert_eq!(input.read(cx).value().as_str(), "olá engine"))
        .unwrap();
    host.update(|window, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(" + paste".into()));
        window.dispatch_action(Box::new(gpui_kit::controls::input::Paste), cx);
    })
    .unwrap();
    host.tick(std::time::Duration::from_millis(16));
    host.update(|_, cx| assert_eq!(input.read(cx).value().as_str(), "olá engine + paste"))
        .unwrap();
    host.render_to_view(&target, EXTENT, 1.).unwrap();
    drop(input);
    drop(input_slot);
}
