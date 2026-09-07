#![cfg(all(not(target_family = "wasm"), feature = "host"))]

use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard, OnceLock},
};

use gpui::{AppContext, Context, InteractiveElement, IntoElement, Render, Styled, Window, div, px};
use gpui_wgpu::{CosmicTextSystem, ExternalGpu, WgpuRuntime};

#[cfg(feature = "kit")]
use gpui::{Entity, Focusable as _, ParentElement as _};
#[cfg(feature = "kit")]
use gpui_wgpu::{TextPreedit, gpui_kit};

struct InputView {
    moved: Rc<Cell<bool>>,
}

impl Render for InputView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let moved = self.moved.clone();
        div()
            .size_full()
            .cursor_pointer()
            .on_mouse_move(move |_, _, _| moved.set(true))
    }
}

#[test]
fn runtime_routes_platform_state_and_redraw_per_window() {
    let _gpu_guard = gpu_test_guard();
    let Some(gpu) = gpu() else { return };
    let device = gpu.device.clone();
    let mut runtime = WgpuRuntime::new(
        gpu,
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new("Segoe UI")),
        Arc::new(()),
    )
    .unwrap();
    let first_moved = Rc::new(Cell::new(false));
    let second_moved = Rc::new(Cell::new(false));
    let first_state = first_moved.clone();
    let second_state = second_moved.clone();
    let (first, _) = runtime
        .open_window(gpui::size(px(64.0), px(64.0)), move |_, cx| {
            cx.new(|_| InputView { moved: first_state })
        })
        .unwrap();
    let (second, _) = runtime
        .open_window(gpui::size(px(48.0), px(32.0)), move |_, cx| {
            cx.new(|_| InputView {
                moved: second_state,
            })
        })
        .unwrap();

    assert_eq!(runtime.take_redraw_requests().len(), 2);
    assert!(runtime.take_redraw_requests().is_empty());
    runtime.set_window_hovered(first, true).unwrap();

    let extent = wgpu::Extent3d {
        width: 64,
        height: 64,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("runtime_input_target"),
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
            first,
            &texture.create_view(&Default::default()),
            extent,
            1.0,
        )
        .unwrap();
    runtime
        .dispatch(
            first,
            gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                position: gpui::point(px(8.0), px(8.0)),
                pressed_button: None,
                modifiers: gpui::Modifiers::default(),
            }),
        )
        .unwrap();

    assert!(first_moved.get());
    assert!(!second_moved.get());
    assert_eq!(runtime.take_redraw_requests(), vec![first]);
    runtime
        .render_window(
            first,
            &texture.create_view(&Default::default()),
            extent,
            1.0,
        )
        .unwrap();
    assert_eq!(
        runtime.window_state(first).unwrap().cursor_style,
        gpui::CursorStyle::PointingHand
    );

    runtime.set_window_focus(first, true).unwrap();
    runtime.set_clipboard_text("Olá, SDL UTF-8 👋".to_owned());
    assert_eq!(
        runtime.clipboard_text().as_deref(),
        Some("Olá, SDL UTF-8 👋")
    );
    assert!(runtime.dispatch_text(first, "").is_err());

    runtime.wake();
    let mut redraws = runtime.take_redraw_requests();
    redraws.sort_by_key(|window| format!("{window:?}"));
    assert_eq!(redraws.len(), 2);
    assert!(redraws.contains(&first));
    assert!(redraws.contains(&second));
}

#[cfg(feature = "kit")]
struct TextView {
    input: Entity<gpui_kit::controls::input::TextInput>,
}

#[cfg(feature = "kit")]
impl Render for TextView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.input.clone())
    }
}

#[cfg(feature = "kit")]
#[test]
fn runtime_commits_utf8_and_preedit_through_the_focused_input_handler() {
    let _gpu_guard = gpu_test_guard();
    let Some(gpu) = gpu() else { return };
    let device = gpu.device.clone();
    let input_slot = Rc::new(std::cell::RefCell::new(None));
    let root_input = input_slot.clone();
    let mut runtime = WgpuRuntime::new(
        gpu,
        wgpu::TextureFormat::Rgba8Unorm,
        Arc::new(CosmicTextSystem::new("Segoe UI")),
        Arc::new(gpui_kit::assets::Assets),
    )
    .unwrap();
    let (window, _) = runtime
        .open_window(gpui::size(px(256.0), px(64.0)), move |window, cx| {
            gpui_kit::install(cx);
            let input =
                cx.new(|cx| gpui_kit::controls::input::TextInput::new("runtime.input", window, cx));
            *root_input.borrow_mut() = Some(input.clone());
            cx.new(|_| TextView { input })
        })
        .unwrap();
    let input = input_slot.borrow().clone().unwrap();
    runtime
        .update_window(window, |window, cx| {
            input.read(cx).focus_handle(cx).focus(window, cx)
        })
        .unwrap();
    runtime.set_window_focus(window, true).unwrap();

    let extent = wgpu::Extent3d {
        width: 256,
        height: 64,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("runtime_text_input_target"),
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
    assert!(runtime.window_state(window).unwrap().text_input_active);

    runtime.dispatch_text(window, "olá 👋").unwrap();
    runtime
        .update_window(window, |_, cx| {
            assert_eq!(input.read(cx).value().as_str(), "olá 👋")
        })
        .unwrap();
    runtime
        .dispatch_text_editing(
            window,
            TextPreedit {
                text: "世界".to_owned(),
                selection_utf16: Some(0..2),
            },
        )
        .unwrap();
    assert!(runtime.window_state(window).unwrap().ime_area.is_some());
    runtime.close_window(window).unwrap();
    drop(input);
    drop(input_slot);
}

fn gpu_test_guard() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
        label: Some("runtime_input_test"),
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
