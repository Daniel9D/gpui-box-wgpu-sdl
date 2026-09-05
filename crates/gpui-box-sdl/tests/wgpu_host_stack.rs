#![cfg(not(target_family = "wasm"))]

use std::{
    cell::{Cell, RefCell},
    ffi::CString,
    rc::Rc,
    sync::Arc,
};

use gpui::{App, Context, FocusHandle, IntoElement, Render, Window, div, prelude::*};
use gpui_sdl::{SdlHostEvent, SdlInputAdapter, Viewport};
use gpui_wgpu::{CosmicTextSystem, ExternalGpu, WgpuHost, wgpu};
use sdl3_sys::everything as sdl;

const EXTENT: wgpu::Extent3d = wgpu::Extent3d {
    width: 128,
    height: 64,
    depth_or_array_layers: 1,
};

#[derive(Clone, Default)]
struct ProbeState {
    mouse_moved: Rc<Cell<bool>>,
    input: Rc<RefCell<Vec<String>>>,
}

struct ProbeView {
    focus: FocusHandle,
    state: ProbeState,
}

impl Render for ProbeView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mouse_moved = Rc::clone(&self.state.mouse_moved);
        let input = Rc::clone(&self.state.input);
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_mouse_move(move |_, _, _| mouse_moved.set(true))
            .on_key_down(move |event, _, _| {
                let value = event
                    .keystroke
                    .key_char
                    .as_ref()
                    .unwrap_or(&event.keystroke.key);
                input.borrow_mut().push(value.clone());
            })
    }
}

#[test]
fn raw_sdl_input_reaches_real_gpui_listeners_through_wgpu_host() {
    let Some((instance, gpu_adapter, device, queue)) = gpu() else {
        return;
    };
    let device = Arc::new(device);
    let queue = Arc::new(queue);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpui_box_sdl_stack_target"),
        size: EXTENT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(wgpu::TextureFormat::Rgba8Unorm),
        ..Default::default()
    });
    let state = ProbeState::default();
    let root_state = state.clone();
    let mut host = WgpuHost::new(
        ExternalGpu {
            instance,
            adapter: gpu_adapter,
            device: Arc::clone(&device),
            queue,
        },
        wgpu::TextureFormat::Rgba8Unorm,
        gpui::size(gpui::px(128.0), gpui::px(64.0)),
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
    host.render_to_view(&view, EXTENT, 1.0).unwrap();

    let mut adapter = SdlInputAdapter::new(Viewport::default()).unwrap();
    let mouse = sdl::SDL_Event {
        motion: sdl::SDL_MouseMotionEvent {
            r#type: sdl::SDL_EVENT_MOUSE_MOTION,
            x: 12.0,
            y: 12.0,
            ..Default::default()
        },
    };
    let key = sdl::SDL_Event {
        key: sdl::SDL_KeyboardEvent {
            r#type: sdl::SDL_EVENT_KEY_DOWN,
            key: sdl::SDLK_X,
            down: true,
            ..Default::default()
        },
    };
    let committed = CString::new("olá").unwrap();
    let text = sdl::SDL_Event {
        text: sdl::SDL_TextInputEvent {
            r#type: sdl::SDL_EVENT_TEXT_INPUT,
            text: committed.as_ptr(),
            ..Default::default()
        },
    };

    for raw in [&mouse, &key, &text] {
        deliver(&mut host, unsafe { adapter.adapt(raw) }).unwrap();
    }

    assert!(state.mouse_moved.get());
    assert_eq!(&*state.input.borrow(), &["x", "olá"]);
}

fn deliver(host: &mut WgpuHost, events: Vec<SdlHostEvent>) -> anyhow::Result<()> {
    for event in events {
        match event {
            SdlHostEvent::Input(input) => {
                host.dispatch(input)?;
            }
            SdlHostEvent::TextInput(text) => host.dispatch_text(&text)?,
            SdlHostEvent::TextEditing(_)
            | SdlHostEvent::WindowResized { .. }
            | SdlHostEvent::FocusChanged(_)
            | SdlHostEvent::Quit => {}
        }
    }
    Ok(())
}

fn gpu() -> Option<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        flags: wgpu::InstanceFlags::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        backend_options: wgpu::BackendOptions::default(),
        display: None,
    });
    let gpu_adapter =
        match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        })) {
            Ok(adapter) => adapter,
            Err(error) => {
                eprintln!("skipping SDL/WgpuHost stack test: no compatible adapter: {error}");
                return None;
            }
        };
    let (device, queue) = pollster::block_on(gpu_adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("gpui_box_sdl_stack_device"),
        required_features: wgpu::Features::empty(),
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    }))
    .expect("request device after acquiring an adapter");
    Some((instance, gpu_adapter, device, queue))
}
