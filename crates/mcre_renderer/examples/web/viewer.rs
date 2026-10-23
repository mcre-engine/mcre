use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use mcre_fs::{Fs as _, FsPath, web::OpfsFs};
use mcre_renderer::{
    Atlas, CloudStatus, CloudTexture, LightMode, MeshOptions, RenderChunk, Renderer, SectionMesh,
    build_atlas_square, demo, mesh_chunk, pack_chunk_sections, vanilla_fog,
};
use mcre_renderer::lightmap::DEFAULT_BRIGHTNESS;
use mcre_resource_pack::{Colormaps, ResourcePackData, ResourcePackSet, discover};
use wasm_bindgen::{JsCast as _, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Event, HtmlInputElement};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    platform::web::{EventLoopExtWebSys as _, WindowAttributesExtWebSys as _},
    window::{Window, WindowAttributes},
};

struct WebState {
    atlas: Arc<Atlas>,
    sections: Vec<(
        [i32; 2],
        [Option<SectionMesh>; mcre_world::SECTIONS_PER_CHUNK],
    )>,
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    cloud_texture: Option<CloudTexture>,
    camera: mcre_renderer::FlyCamera,
    generation: u64,
    initializing: bool,
    keys: std::collections::HashSet<winit::keyboard::KeyCode>,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    last_frame_ms: f64,
}

struct WebViewer {
    state: Rc<RefCell<WebState>>,
}

#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    spawn_local(async {
        let fs = Rc::new(OpfsFs::new().await.expect("initialize OPFS"));
        fs.create_dir_all(&FsPath::from("resourcepacks"))
            .await
            .expect("create resource pack directory");
        fs.create_dir_all(&FsPath::from("pack_cache"))
            .await
            .expect("create resource pack cache");

        let empty_data = empty_resource_pack_data();
        let atlas = Arc::new(
            build_atlas_square(&*fs, &[], &empty_data.texture_table)
                .await
                .expect("build placeholder atlas"),
        );
        let state = Rc::new(RefCell::new(WebState {
            atlas,
            sections: Vec::new(),
            renderer: None,
            window: None,
            cloud_texture: None,
            camera: mcre_renderer::FlyCamera::new(demo::DEMO_CAMERA_POSITION),
            generation: 0,
            initializing: false,
            keys: std::collections::HashSet::new(),
            dragging: false,
            last_cursor: None,
            last_frame_ms: 0.0,
        }));
        install_pack_input(fs.clone(), state.clone());

        let event_loop = EventLoop::new().expect("event loop");
        event_loop.spawn_app(WebViewer { state });
    });
}

fn main() {}

fn empty_resource_pack_data() -> ResourcePackData {
    let map = || vec![0; 256 * 256 * 3].into_boxed_slice();
    ResourcePackData::new(
        Vec::new(),
        mcre_resource_pack::interner::StringInterner::new(),
        HashMap::new(),
        Colormaps {
            grass: map(),
            foliage: map(),
            dry_foliage: map(),
        },
    )
}

fn install_pack_input(fs: Rc<OpfsFs>, state: Rc<RefCell<WebState>>) {
    let document = web_sys::window()
        .expect("window")
        .document()
        .expect("document");
    let input: HtmlInputElement = document
        .create_element("input")
        .expect("create pack input")
        .dyn_into()
        .expect("pack input element");
    input.set_type("file");
    input.set_accept(".zip");
    input.set_title("Choose a Minecraft Java resource pack zip");
    if let Some(body) = document.body() {
        let _ = body.append_child(&input);
    }

    let callback = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        let Some(input) = event
            .target()
            .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(file) = input.files().and_then(|files| files.item(0)) else {
            return;
        };
        let fs = fs.clone();
        let state = state.clone();
        spawn_local(async move {
            if let Err(error) = load_uploaded_pack(fs, state, file).await {
                web_sys::console::error_1(&JsValue::from_str(&error));
            }
        });
    });
    input
        .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
        .expect("listen for pack selection");
    callback.forget();
}

async fn load_uploaded_pack(
    fs: Rc<OpfsFs>,
    state: Rc<RefCell<WebState>>,
    file: web_sys::File,
) -> Result<(), String> {
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| format!("read selected pack: {error:?}"))?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    let filename = file.name();
    if !filename.ends_with(".zip") {
        return Err("Choose a zipped Java resource pack (.zip)".into());
    }
    let pack_path = FsPath::from("resourcepacks").join(filename.as_str());
    fs.write(&pack_path, &bytes)
        .await
        .map_err(|error| format!("save resource pack in OPFS: {error}"))?;

    let discovered = discover::discover_packs(&*fs, &FsPath::from("resourcepacks")).await;
    let pack = discovered
        .iter()
        .find(|pack| pack.id == filename)
        .ok_or_else(|| "selected file is not a readable resource pack".to_string())?;
    if let Some(error) = &pack.error {
        return Err(format!("resource pack error: {error}"));
    }

    let extracted = discover::ensure_extracted(&*fs, pack, &FsPath::from("pack_cache"))
        .await
        .map_err(|error| error.to_string())?;
    let mut packs = ResourcePackSet::new();
    packs
        .add_pack(&*fs, &extracted)
        .await
        .map_err(|error| error.to_string())?;
    let roots = packs.texture_roots();
    let data = Arc::new(packs.build(&*fs).await.map_err(|error| error.to_string())?);
    let atlas = Arc::new(
        build_atlas_square(&*fs, &roots, &data.texture_table)
            .await
            .map_err(|error| error.to_string())?,
    );
    let cloud_texture = CloudTexture::load(&*fs, &roots).await;
    let sections = build_demo_sections(&data, &atlas);

    let mut state = state.borrow_mut();
    state.generation += 1;
    if let Some(renderer) = state.renderer.as_mut() {
        renderer.set_atlas(&atlas);
        renderer.set_cloud_texture(cloud_texture.clone());
        renderer.set_cloud_status(if cloud_texture.is_some() {
            CloudStatus::Fancy
        } else {
            CloudStatus::Off
        });
    }
    state.atlas = atlas;
    state.sections = sections;
    state.cloud_texture = cloud_texture;
    if let Some(body) = web_sys::window().and_then(|window| window.document()?.body()) {
        let _ = body.set_attribute("data-pack-loaded", "true");
    }
    Ok(())
}

fn build_demo_sections(
    data: &ResourcePackData,
    atlas: &Atlas,
) -> Vec<(
    [i32; 2],
    [Option<SectionMesh>; mcre_world::SECTIONS_PER_CHUNK],
)> {
    let world = demo::demo_world();

    world
        .iter_chunks()
        .map(|chunk| {
            let origin = [chunk.pos().x * 16, chunk.pos().z * 16];
            let mesh = mesh_chunk(
                data,
                atlas,
                chunk,
                &world,
                origin,
                LightMode::Smooth,
                MeshOptions::default(),
            );
            (origin, pack_chunk_sections(&mesh, origin))
        })
        .collect()
}

impl ApplicationHandler for WebViewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.borrow().initializing {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("MCRE")
                        .with_inner_size(PhysicalSize::new(1280, 720))
                        .with_append(true),
                )
                .expect("create browser canvas"),
        );
        let atlas = self.state.borrow().atlas.clone();
        {
            let mut state = self.state.borrow_mut();
            state.window = Some(window.clone());
            state.initializing = true;
        }
        let state = self.state.clone();
        let window_for_renderer = window.clone();
        spawn_local(async move {
            let mut renderer = Renderer::new(window_for_renderer.clone(), &atlas)
                .await
                .expect("initialize WebGPU renderer");
            let size = window_for_renderer.inner_size();
            renderer.resize(size.width, size.height);
            renderer.set_fog(vanilla_fog(1, 6000.0, None));
            renderer.set_time(6000.0);
            renderer.set_brightness(DEFAULT_BRIGHTNESS);
            let mut state = state.borrow_mut();
            renderer.set_atlas(&state.atlas);
            renderer.set_cloud_texture(state.cloud_texture.clone());
            renderer.set_cloud_status(if state.cloud_texture.is_some() {
                CloudStatus::Fancy
            } else {
                CloudStatus::Off
            });
            state.renderer = Some(renderer);
            state.window = Some(window_for_renderer);
            state.initializing = false;
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.state.borrow_mut().renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let pressed = state == ElementState::Pressed;
                let mut state = self.state.borrow_mut();
                state.dragging = pressed;
                state.last_cursor = None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let mut state = self.state.borrow_mut();
                if state.dragging {
                    if let Some((last_x, last_y)) = state.last_cursor {
                        let dx = position.x - last_x;
                        let dy = position.y - last_y;
                        let sensitivity = state.camera.sensitivity;
                        state
                            .camera
                            .rotate(dx as f32 * sensitivity, -(dy as f32) * sensitivity);
                    }
                    state.last_cursor = Some((position.x, position.y));
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    let mut state = self.state.borrow_mut();
                    if event.state == ElementState::Pressed {
                        state.keys.insert(code);
                    } else {
                        state.keys.remove(&code);
                    }
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let now = js_sys::Date::now();
        let mut state = self.state.borrow_mut();
        let dt = if state.last_frame_ms > 0.0 {
            ((now - state.last_frame_ms) / 1000.0).clamp(0.0, 0.05) as f32
        } else {
            0.0
        };
        state.last_frame_ms = now;
        if dt > 0.0 {
            let mut forward = 0.0;
            if state.keys.contains(&KeyCode::KeyW) {
                forward += 1.0;
            }
            if state.keys.contains(&KeyCode::KeyS) {
                forward -= 1.0;
            }
            let mut strafe = 0.0;
            if state.keys.contains(&KeyCode::KeyD) {
                strafe += 1.0;
            }
            if state.keys.contains(&KeyCode::KeyA) {
                strafe -= 1.0;
            }
            let mut vertical = 0.0;
            if state.keys.contains(&KeyCode::Space) {
                vertical += 1.0;
            }
            if state.keys.contains(&KeyCode::ShiftLeft) {
                vertical -= 1.0;
            }
            let boost = if state.keys.contains(&KeyCode::ControlLeft) {
                3.0
            } else {
                1.0
            };
            let speed = state.camera.speed * boost * dt;
            let forward_vec = state.camera.forward();
            let right_vec = state.camera.right();
            for axis in 0..3 {
                state.camera.position[axis] +=
                    (forward_vec[axis] * forward + right_vec[axis] * strafe) * speed;
            }
            state.camera.position[1] += vertical * speed;
        }
        if let Some(window) = state.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl WebViewer {
    fn render(&mut self) {
        let mut state = self.state.borrow_mut();
        let Some(window) = state.window.as_ref() else {
            return;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let view_projection = state
            .camera
            .view_projection(size.width as f32 / size.height as f32)
            .0;
        let camera_position = state.camera.position;
        let Some(mut renderer) = state.renderer.take() else {
            return;
        };
        let chunks: Vec<_> = state
            .sections
            .iter()
            .map(|(origin, sections)| RenderChunk {
                origin: *origin,
                version: state.generation,
                sections,
                visible_sections: (1 << mcre_world::SECTIONS_PER_CHUNK) - 1,
            })
            .collect();
        renderer.render(view_projection, camera_position, &chunks, None);
        state.renderer = Some(renderer);
    }
}
