use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Read as _, Write as _},
    rc::Rc,
    sync::Arc,
};

use mcre_anvil::{BiomeResolver, BlockResolver, ImportStats, RegionFile, Spawn};
use mcre_fs::{Fs as _, FsPath, web::OpfsFs};
use mcre_renderer::{
    Atlas, AtlasAnimator, CloudStatus, CloudTexture, DebugFlags, Fog, LightMode, MeshOptions,
    RenderChunk, Renderer, SectionMesh, build_atlas_square, demo, mesh_chunk, pack_chunk_sections,
    vanilla_fog,
};
use mcre_renderer::lightmap::DEFAULT_BRIGHTNESS;
use mcre_resource_pack::{Colormaps, ResourcePackData, ResourcePackSet, discover};
use mcre_world::{Chunk, ChunkPos, WORLD_HEIGHT, WORLD_MIN_Y, World};
use wasm_bindgen::{JsCast as _, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Event, HtmlButtonElement, HtmlInputElement};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, DeviceEvents, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    platform::web::{
        EventLoopExtWebSys as _, WindowAttributesExtWebSys as _, WindowExtWebSys as _,
    },
    window::{CursorGrabMode, Window, WindowAttributes},
};

const SAVE_CHUNK_RADIUS: i32 = 4;

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = "export function request_pointer_lock(canvas) { try { const pending = canvas.requestPointerLock(); if (pending && typeof pending.catch === 'function') pending.catch(() => document.dispatchEvent(new Event('pointerlockerror'))); } catch (_) { document.dispatchEvent(new Event('pointerlockerror')); } }")]
extern "C" {
    fn request_pointer_lock(canvas: &web_sys::HtmlCanvasElement);
}

struct WebState {
    data: Arc<ResourcePackData>,
    world: World,
    atlas: Arc<Atlas>,
    animator: AtlasAnimator,
    animation_ticks: f64,
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
    pointer_locked: bool,
    pointer_lock_pending: bool,
    last_cursor: Option<(f64, f64)>,
    last_frame_ms: f64,
    render_distance: u32,
    time_ticks: f32,
    brightness: f32,
    debug_visible: bool,
    fog_enabled: bool,
    unlit: bool,
    heatmap: bool,
    wireframe: bool,
    backface_cull: bool,
    chunk_borders: bool,
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

        let data = Arc::new(empty_resource_pack_data());
        let atlas = Arc::new(
            build_atlas_square(&*fs, &[], &data.texture_table)
                .await
                .expect("build placeholder atlas"),
        );
        let animator = AtlasAnimator::new(&atlas);
        let state = Rc::new(RefCell::new(WebState {
            data,
            world: demo::demo_world(),
            atlas,
            animator,
            animation_ticks: 0.0,
            sections: Vec::new(),
            renderer: None,
            window: None,
            cloud_texture: None,
            camera: mcre_renderer::FlyCamera::new(demo::DEMO_CAMERA_POSITION),
            generation: 0,
            initializing: false,
            keys: std::collections::HashSet::new(),
            dragging: false,
            pointer_locked: false,
            pointer_lock_pending: false,
            last_cursor: None,
            last_frame_ms: 0.0,
            render_distance: 1,
            time_ticks: 6000.0,
            brightness: DEFAULT_BRIGHTNESS,
            debug_visible: false,
            fog_enabled: true,
            unlit: false,
            heatmap: false,
            wireframe: false,
            backface_cull: true,
            chunk_borders: false,
        }));
        install_pack_input(fs.clone(), state.clone());
        install_save_input(fs.clone(), state.clone());
        install_save_download(fs);
        install_debug_controls(state.clone());

        let event_loop = EventLoop::new().expect("event loop");
        event_loop.spawn_app(WebViewer { state });
    });
}

fn main() {}

fn empty_resource_pack_data() -> ResourcePackData {
    let map = || vec![0; 256 * 256 * 3].into_boxed_slice();
    ResourcePackData::new(
        Vec::new(),
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
    input.set_id("pack-input");
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

fn install_save_input(fs: Rc<OpfsFs>, state: Rc<RefCell<WebState>>) {
    let document = web_sys::window()
        .expect("window")
        .document()
        .expect("document");
    let input: HtmlInputElement = document
        .create_element("input")
        .expect("create save input")
        .dyn_into()
        .expect("save input element");
    input.set_type("file");
    input.set_accept(".zip");
    input.set_id("world-input");
    input.set_title("Choose a Minecraft Java world zip");
    input.style().set_property("top", "48px").ok();
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
            if let Err(error) = load_uploaded_save(fs, state, file).await {
                web_sys::console::error_1(&JsValue::from_str(&error));
            }
        });
    });
    input
        .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
        .expect("listen for save selection");
    callback.forget();
}

fn install_save_download(fs: Rc<OpfsFs>) {
    let document = web_sys::window()
        .expect("window")
        .document()
        .expect("document");
    let button: HtmlButtonElement = document
        .create_element("button")
        .expect("create save download button")
        .dyn_into()
        .expect("save download button");
    button.set_id("download-save");
    button.set_text_content(Some("Download world zip"));
    let style = button.style();
    style.set_property("position", "fixed").ok();
    style.set_property("top", "84px").ok();
    style.set_property("left", "12px").ok();
    style.set_property("z-index", "2").ok();
    style.set_property("display", "none").ok();
    if let Some(body) = document.body() {
        let _ = body.append_child(&button);
    }

    let callback = Closure::<dyn FnMut(Event)>::new(move |_event: Event| {
        let fs = fs.clone();
        spawn_local(async move {
            if let Err(error) = download_save(fs).await {
                web_sys::console::error_1(&JsValue::from_str(&error));
            }
        });
    });
    button
        .add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())
        .expect("listen for save download");
    callback.forget();
}

#[derive(Clone, Copy)]
enum DebugToggle {
    Fog,
    Unlit,
    Heatmap,
    Wireframe,
    BackfaceCull,
    ChunkBorders,
}

fn install_debug_controls(state: Rc<RefCell<WebState>>) {
    let document = web_sys::window()
        .expect("window")
        .document()
        .expect("document");
    let button: HtmlButtonElement = document
        .get_element_by_id("debug-toggle")
        .expect("debug toggle")
        .dyn_into()
        .expect("debug toggle button");
    let toggle_state = state.clone();
    let callback = Closure::<dyn FnMut(Event)>::new(move |_| toggle_debug_menu(&toggle_state));
    button
        .add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())
        .expect("listen for debug toggle");
    callback.forget();

    let key_state = state.clone();
    let callback = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
        move |event: web_sys::KeyboardEvent| {
            if event.key() == "F1" && !event.repeat() {
                event.prevent_default();
                release_pointer_state(&key_state);
                toggle_debug_menu(&key_state);
            }
        },
    );
    document
        .add_event_listener_with_callback("keydown", callback.as_ref().unchecked_ref())
        .expect("listen for debug key");
    callback.forget();

    let distance: HtmlInputElement = document
        .get_element_by_id("render-distance")
        .expect("render distance control")
        .dyn_into()
        .expect("render distance input");
    let distance_for_event = distance.clone();
    let distance_state = state.clone();
    let callback = Closure::<dyn FnMut(Event)>::new(move |_| {
        let value = distance_for_event.value_as_number().round() as u32;
        let value = value.clamp(1, SAVE_CHUNK_RADIUS as u32);
        distance_state.borrow_mut().render_distance = value;
        set_render_distance_label(value);
    });
    distance
        .add_event_listener_with_callback("input", callback.as_ref().unchecked_ref())
        .expect("listen for render distance changes");
    callback.forget();

    let time: HtmlInputElement = document
        .get_element_by_id("time-of-day")
        .expect("time control")
        .dyn_into()
        .expect("time input");
    let time_for_event = time.clone();
    let time_state = state.clone();
    let callback = Closure::<dyn FnMut(Event)>::new(move |_| {
        let value = time_for_event.value_as_number().round().clamp(0.0, 24_000.0) as u32;
        time_state.borrow_mut().time_ticks = value as f32;
        let value_text = value.to_string();
        let label_text = format!("{value} ticks");
        set_slider_control("time-of-day", "time-value", &value_text, &label_text);
    });
    time.add_event_listener_with_callback("input", callback.as_ref().unchecked_ref())
        .expect("listen for time changes");
    callback.forget();

    let brightness: HtmlInputElement = document
        .get_element_by_id("brightness")
        .expect("brightness control")
        .dyn_into()
        .expect("brightness input");
    let brightness_for_event = brightness.clone();
    let brightness_state = state.clone();
    let callback = Closure::<dyn FnMut(Event)>::new(move |_| {
        let value = brightness_for_event.value_as_number().clamp(0.0, 1.0) as f32;
        brightness_state.borrow_mut().brightness = value;
        let value_label = format!("{value:.2}");
        set_slider_control("brightness", "brightness-value", &value_label, &value_label);
    });
    brightness
        .add_event_listener_with_callback("input", callback.as_ref().unchecked_ref())
        .expect("listen for brightness changes");
    callback.forget();

    for (id, toggle) in [
        ("fog-enabled", DebugToggle::Fog),
        ("unlit-enabled", DebugToggle::Unlit),
        ("heatmap-enabled", DebugToggle::Heatmap),
        ("wireframe-enabled", DebugToggle::Wireframe),
        ("backface-cull-enabled", DebugToggle::BackfaceCull),
        ("chunk-borders-enabled", DebugToggle::ChunkBorders),
    ] {
        let input: HtmlInputElement = document
            .get_element_by_id(id)
            .expect("debug option")
            .dyn_into()
            .expect("debug checkbox");
        let input_for_event = input.clone();
        let toggle_state = state.clone();
        let callback = Closure::<dyn FnMut(Event)>::new(move |_| {
            set_debug_toggle(&mut toggle_state.borrow_mut(), toggle, input_for_event.checked());
        });
        input
            .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
            .expect("listen for debug option");
        callback.forget();
    }
}

fn set_debug_toggle(state: &mut WebState, toggle: DebugToggle, enabled: bool) {
    match toggle {
        DebugToggle::Fog => state.fog_enabled = enabled,
        DebugToggle::Unlit => state.unlit = enabled,
        DebugToggle::Heatmap => state.heatmap = enabled,
        DebugToggle::Wireframe => state.wireframe = enabled,
        DebugToggle::BackfaceCull => state.backface_cull = enabled,
        DebugToggle::ChunkBorders => state.chunk_borders = enabled,
    }
}

fn toggle_debug_menu(state: &Rc<RefCell<WebState>>) {
    let visible = {
        let mut state = state.borrow_mut();
        state.debug_visible = !state.debug_visible;
        state.keys.clear();
        state.debug_visible
    };
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(menu) = document.get_element_by_id("debug-menu") {
        if visible {
            let _ = menu.remove_attribute("hidden");
        } else {
            let _ = menu.set_attribute("hidden", "");
        }
    }
    if let Some(button) = document.get_element_by_id("debug-toggle") {
        let _ = button.set_attribute("aria-expanded", &visible.to_string());
    }
}

fn release_pointer_state(state: &Rc<RefCell<WebState>>) {
    let mut state = state.borrow_mut();
    if !state.pointer_locked && !state.pointer_lock_pending && !state.dragging {
        return;
    }
    if let Some(window) = state.window.as_ref() {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
    }
    state.pointer_locked = false;
    state.pointer_lock_pending = false;
    state.dragging = false;
    state.last_cursor = None;
}

fn set_render_distance_label(distance: u32) {
    let value = distance.to_string();
    set_slider_control(
        "render-distance",
        "render-distance-value",
        &value,
        &format!("{distance} chunks"),
    );
}

fn set_slider_control(input_id: &str, label_id: &str, value: &str, label: &str) {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if let Some(input) = document
            .get_element_by_id(input_id)
            .and_then(|element| element.dyn_into::<HtmlInputElement>().ok())
        {
            input.set_value(value);
        }
        if let Some(label_element) = document.get_element_by_id(label_id) {
            label_element.set_text_content(Some(label));
        }
    }
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
    let sections = {
        let state = state.borrow();
        build_sections(&data, &atlas, &state.world)
    };
    let animator = AtlasAnimator::new(&atlas);

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
    state.data = data;
    state.atlas = atlas;
    state.animator = animator;
    state.animation_ticks = 0.0;
    state.sections = sections;
    state.cloud_texture = cloud_texture;
    if let Some(body) = web_sys::window().and_then(|window| window.document()?.body()) {
        let _ = body.set_attribute("data-pack-loaded", "true");
    }
    Ok(())
}

fn build_sections(
    data: &ResourcePackData,
    atlas: &Atlas,
    world: &World,
) -> Vec<(
    [i32; 2],
    [Option<SectionMesh>; mcre_world::SECTIONS_PER_CHUNK],
)> {
    world
        .iter_chunks()
        .map(|chunk| {
            let origin = [chunk.pos().x * 16, chunk.pos().z * 16];
            let mesh = mesh_chunk(
                data,
                atlas,
                chunk,
                world,
                origin,
                LightMode::Smooth,
                MeshOptions::default(),
            );
            (origin, pack_chunk_sections(&mesh, origin))
        })
        .collect()
}

async fn load_uploaded_save(
    fs: Rc<OpfsFs>,
    state: Rc<RefCell<WebState>>,
    file: web_sys::File,
) -> Result<(), String> {
    let filename = file.name();
    if !filename.ends_with(".zip") {
        return Err("Choose a zipped Java world (.zip)".into());
    }
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| format!("read selected world: {error:?}"))?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();

    let save_root = FsPath::from("saves").join("upload");
    if fs.exists(&save_root).await {
        fs.remove_dir_all(&save_root)
            .await
            .map_err(|error| format!("clear previous save: {error}"))?;
    }
    fs.create_dir_all(&save_root)
        .await
        .map_err(|error| format!("create save directory: {error}"))?;
    extract_world_zip(&fs, &save_root, &bytes).await?;
    let (world, spawn) = load_save_world(&fs, &save_root).await?;
    let (camera_position, camera_yaw, camera_pitch) = save_camera(&world, &spawn);
    let sections = {
        let state = state.borrow();
        build_sections(&state.data, &state.atlas, &world)
    };

    let mut state = state.borrow_mut();
    state.world = world;
    state.sections = sections;
    state.render_distance = SAVE_CHUNK_RADIUS as u32;
    state.camera.position = camera_position;
    state.camera.yaw = camera_yaw;
    state.camera.pitch = camera_pitch;
    state.generation += 1;
    drop(state);
    set_render_distance_label(SAVE_CHUNK_RADIUS as u32);
    if let Some(body) = web_sys::window().and_then(|window| window.document()?.body()) {
        let _ = body.set_attribute("data-save-loaded", "true");
    }
    if let Some(button) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("download-save"))
        .and_then(|element| element.dyn_into::<HtmlButtonElement>().ok())
    {
        button.style().set_property("display", "block").ok();
    }
    Ok(())
}

async fn download_save(fs: Rc<OpfsFs>) -> Result<(), String> {
    let save_root = FsPath::from("saves").join("upload");
    let bytes = zip_save(&fs, &save_root).await?;
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes.as_slice()));
    let blob = web_sys::Blob::new_with_u8_array_sequence(parts.as_ref())
        .map_err(|error| format!("create save archive: {error:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|error| format!("create save download URL: {error:?}"))?;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "document unavailable".to_string())?;
    let anchor: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(|error| format!("create download link: {error:?}"))?
        .dyn_into()
        .map_err(|_| "download link element".to_string())?;
    web_sys::HtmlAnchorElement::set_href(&anchor, &url);
    web_sys::HtmlAnchorElement::set_download(&anchor, "world.zip");
    anchor.click();
    web_sys::Url::revoke_object_url(&url).ok();
    Ok(())
}

async fn zip_save(fs: &OpfsFs, save_root: &FsPath) -> Result<Vec<u8>, String> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let mut pending = vec![save_root.clone()];
    while let Some(dir) = pending.pop() {
        let entries = fs
            .read_dir(&dir)
            .await
            .map_err(|error| format!("list save directory: {error}"))?;
        for entry in entries {
            if entry.metadata.is_dir {
                pending.push(entry.path);
                continue;
            }
            let data = fs
                .read(&entry.path)
                .await
                .map_err(|error| format!("read save file: {error}"))?;
            let relative = entry.path.parts()[save_root.parts().len()..]
                .iter()
                .map(|part| part.as_ref())
                .collect::<Vec<_>>()
                .join("/");
            writer
                .start_file(
                    relative,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(|error| format!("write save archive: {error}"))?;
            writer
                .write_all(&data)
                .map_err(|error| format!("write save archive: {error}"))?;
        }
    }
    let cursor = writer
        .finish()
        .map_err(|error| format!("finish save archive: {error}"))?;
    Ok(cursor.into_inner())
}

async fn extract_world_zip(
    fs: &OpfsFs,
    save_root: &FsPath,
    bytes: &[u8],
) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("read world zip: {error}"))?;
    let mut best: Option<(usize, String)> = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("read world zip entry: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|error| format!("read world zip entry name: {error}"))?;
        if name == "level.dat" || name.ends_with("/level.dat") {
            let depth = name.matches('/').count();
            if best
                .as_ref()
                .is_none_or(|(best_depth, _)| depth < *best_depth)
            {
                let prefix = name
                    .rsplit_once('/')
                    .map(|(prefix, _)| prefix.to_string())
                    .unwrap_or_default();
                best = Some((depth, prefix));
            }
        }
    }
    let (_, prefix) = best.ok_or_else(|| "world zip has no level.dat".to_string())?;
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/")
    };

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("read world zip entry: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|error| format!("read world zip entry name: {error}"))?
            .into_owned();
        let relative = name.strip_prefix(&prefix).unwrap_or(&name);
        if relative.is_empty() || relative.starts_with('/') || relative.contains("..") {
            continue;
        }
        let path = join_relative(save_root, relative);
        if let Some(parent) = path.parent() {
            fs.create_dir_all(&parent)
                .await
                .map_err(|error| format!("create save directory: {error}"))?;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut data)
            .map_err(|error| format!("read world zip entry: {error}"))?;
        fs.write(&path, &data)
            .await
            .map_err(|error| format!("write save file: {error}"))?;
    }
    Ok(())
}

fn join_relative(base: &FsPath, relative: &str) -> FsPath {
    let mut path = base.clone();
    for part in relative.split('/').filter(|part| !part.is_empty()) {
        path.push(part);
    }
    path
}

async fn load_save_world(fs: &OpfsFs, save_root: &FsPath) -> Result<(World, Spawn), String> {
    let level_bytes = fs
        .read(&save_root.join("level.dat"))
        .await
        .map_err(|error| format!("read level.dat: {error}"))?;
    let level = mcre_anvil::read_gzip_nbt_bytes(&level_bytes)
        .map_err(|error| format!("parse level.dat: {error}"))?;
    let spawn =
        mcre_anvil::read_spawn_from_root(&level).map_err(|error| format!("read spawn: {error}"))?;
    let region_dir = resolve_region_dir(fs, save_root, &spawn.dimension).await;

    let blocks = BlockResolver::new();
    let biomes = BiomeResolver::new();
    let mut stats = ImportStats::default();
    let mut world = World::new(SAVE_CHUNK_RADIUS as u32);
    let mut regions: HashMap<(i32, i32), Option<RegionFile>> = HashMap::new();
    let spawn_chunk = (spawn.pos.0.div_euclid(16), spawn.pos.2.div_euclid(16));
    for dz in -SAVE_CHUNK_RADIUS..=SAVE_CHUNK_RADIUS {
        for dx in -SAVE_CHUNK_RADIUS..=SAVE_CHUNK_RADIUS {
            let chunk_x = spawn_chunk.0 + dx;
            let chunk_z = spawn_chunk.1 + dz;
            let region_x = chunk_x.div_euclid(32);
            let region_z = chunk_z.div_euclid(32);
            let region = region_at(fs, &region_dir, &mut regions, region_x, region_z).await;
            let chunk = match region {
                Some(region) => match region.chunk_nbt(chunk_x, chunk_z) {
                    Ok(Some(root)) => {
                        match mcre_anvil::load_chunk_from_nbt(
                            &root, chunk_x, chunk_z, &blocks, &biomes, &mut stats,
                        ) {
                            Ok(chunk) => chunk,
                            Err(error) => {
                                web_sys::console::warn_1(&JsValue::from_str(&format!(
                                    "skipping corrupt chunk {chunk_x},{chunk_z}: {error}"
                                )));
                                None
                            }
                        }
                    }
                    Ok(None) => None,
                    Err(error) => {
                        web_sys::console::warn_1(&JsValue::from_str(&format!(
                            "skipping unreadable chunk {chunk_x},{chunk_z}: {error}"
                        )));
                        None
                    }
                },
                None => None,
            };
            world.replace_chunk(
                chunk.unwrap_or_else(|| Chunk::new_at(ChunkPos::new(chunk_x, chunk_z))),
            );
        }
    }
    world.adopt_stored_light();
    world.compute_light();
    web_sys::console::log_1(&JsValue::from_str(&format!(
        "loaded save: {} chunks, {} sections, {} blocks ({} unknown blocks)",
        stats.chunks,
        stats.sections,
        stats.blocks_set,
        stats.unknown_blocks.len()
    )));
    Ok((world, spawn))
}

async fn region_at<'a>(
    fs: &OpfsFs,
    region_dir: &FsPath,
    regions: &'a mut HashMap<(i32, i32), Option<RegionFile>>,
    region_x: i32,
    region_z: i32,
) -> Option<&'a RegionFile> {
    if let std::collections::hash_map::Entry::Vacant(entry) =
        regions.entry((region_x, region_z))
    {
        let path = region_dir.join(format!("r.{region_x}.{region_z}.mca"));
        let region = match fs.read(&path).await {
            Ok(bytes) => RegionFile::from_bytes(bytes).ok(),
            Err(_) => None,
        };
        entry.insert(region);
    }
    regions
        .get(&(region_x, region_z))
        .and_then(Option::as_ref)
}

async fn resolve_region_dir(fs: &OpfsFs, save_root: &FsPath, dimension: &str) -> FsPath {
    let (namespace, path) = dimension
        .split_once(':')
        .unwrap_or(("minecraft", dimension));
    let mut modern = save_root.join("dimensions").join(namespace);
    for part in path.split('/').filter(|part| !part.is_empty()) {
        modern.push(part);
    }
    let modern = modern.join("region");
    if fs.exists(&modern).await {
        modern
    } else {
        save_root.join("region")
    }
}

fn save_camera(world: &World, spawn: &Spawn) -> ([f32; 3], f32, f32) {
    let column = (spawn.pos.0 + 6, spawn.pos.2 + 10);
    let mut top = spawn.pos.1;
    for y in (WORLD_MIN_Y..WORLD_MIN_Y + WORLD_HEIGHT).rev() {
        if let Some(state) = world.get_block((column.0, y, column.1))
            && !state.is_air()
        {
            top = y;
            break;
        }
    }
    let position = [column.0 as f32, top as f32 + 6.0, column.1 as f32];
    let dx = spawn.pos.0 as f32 - position[0];
    let dz = spawn.pos.2 as f32 - position[2];
    let yaw = dx.atan2(-dz);
    let target_y = spawn.pos.1 as f32 + 1.0;
    let pitch = (target_y - position[1])
        .atan2(dx.hypot(dz))
        .clamp(-0.8, -0.45);
    (position, yaw, pitch)
}

impl ApplicationHandler for WebViewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.borrow().initializing {
            return;
        }
        event_loop.listen_device_events(DeviceEvents::Always);
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
        window.set_prevent_default(true);
        let atlas = self.state.borrow().atlas.clone();
        {
            let mut state = self.state.borrow_mut();
            state.window = Some(window.clone());
            state.initializing = true;
        }
        let canvas = window.canvas().expect("browser canvas");
        let state_for_pointer = self.state.clone();
        let canvas_for_pointer = canvas.clone();
        let pointer_down =
            Closure::<dyn FnMut(web_sys::PointerEvent)>::new(move |event: web_sys::PointerEvent| {
            if event.button() != 0 {
                return;
            }
            let mut state = state_for_pointer.borrow_mut();
            if state.pointer_locked || state.pointer_lock_pending {
                return;
            }
            state.pointer_lock_pending = true;
            state.dragging = true;
            state.last_cursor = None;
            drop(state);
            request_pointer_lock(&canvas_for_pointer);
            });
        canvas
            .add_event_listener_with_callback("pointerdown", pointer_down.as_ref().unchecked_ref())
            .expect("listen for pointer capture");
        pointer_down.forget();

        let document = web_sys::window()
            .expect("window")
            .document()
            .expect("document");
        let state_for_change = self.state.clone();
        let document_for_change = document.clone();
        let pointer_lock_change = Closure::<dyn FnMut(Event)>::new(move |_| {
            let locked = document_for_change.pointer_lock_element().is_some();
            let mut state = state_for_change.borrow_mut();
            state.pointer_locked = locked;
            state.pointer_lock_pending = false;
            state.dragging = false;
            if let Some(window) = state.window.as_ref() {
                window.set_cursor_visible(!locked);
            }
        });
        document
            .add_event_listener_with_callback(
                "pointerlockchange",
                pointer_lock_change.as_ref().unchecked_ref(),
            )
            .expect("listen for pointer lock changes");
        pointer_lock_change.forget();

        let document = web_sys::window()
            .expect("window")
            .document()
            .expect("document");
        let state_for_error = self.state.clone();
        let pointer_lock_error = Closure::<dyn FnMut(Event)>::new(move |_| {
            let mut state = state_for_error.borrow_mut();
            state.pointer_locked = false;
            state.pointer_lock_pending = false;
            if let Some(window) = state.window.as_ref() {
                window.set_cursor_visible(true);
            }
        });
        document
            .add_event_listener_with_callback(
                "pointerlockerror",
                pointer_lock_error.as_ref().unchecked_ref(),
            )
            .expect("listen for pointer lock errors");
        pointer_lock_error.forget();
        let state = self.state.clone();
        let window_for_renderer = window.clone();
        spawn_local(async move {
            let mut renderer = Renderer::new(window_for_renderer.clone(), &atlas)
                .await
                .expect("initialize WebGPU renderer");
            let size = window_for_renderer.inner_size();
            renderer.resize(size.width, size.height);
            renderer.set_fog(vanilla_fog(1, 6000.0, None, 0.0));
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
                button,
                ..
            } => {
                if button == MouseButton::Left && state == ElementState::Pressed {
                    let mut state = self.state.borrow_mut();
                    if !state.pointer_locked && !state.pointer_lock_pending {
                        state.dragging = true;
                        state.last_cursor = None;
                    }
                } else if button == MouseButton::Left && state == ElementState::Released {
                    let mut state = self.state.borrow_mut();
                    if !state.pointer_locked {
                        state.dragging = false;
                        state.last_cursor = None;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let mut state = self.state.borrow_mut();
                if state.dragging && !state.pointer_locked {
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
                    if code == KeyCode::F1 {
                        return;
                    }
                    if event.state == ElementState::Pressed && code == KeyCode::Escape {
                        self.release_pointer();
                        return;
                    }
                    let mut state = self.state.borrow_mut();
                    if event.state == ElementState::Pressed {
                        state.keys.insert(code);
                    } else {
                        state.keys.remove(&code);
                    }
                }
            }
            WindowEvent::Focused(false) => self.release_pointer(),
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            let mut state = self.state.borrow_mut();
            if state.pointer_locked {
                let sensitivity = state.camera.sensitivity;
                state.camera.rotate(
                    delta.0 as f32 * sensitivity,
                    -(delta.1 as f32) * sensitivity,
                );
            }
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
            state.animation_ticks += f64::from(dt) * 20.0;
            if !state.debug_visible {
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
        }
        if let Some(window) = state.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl WebViewer {
    fn release_pointer(&mut self) {
        release_pointer_state(&self.state);
    }

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
        let atlas = state.atlas.clone();
        let ticks = state.animation_ticks;
        if state.animator.update(&atlas, ticks) {
            renderer.update_atlas(&atlas, state.animator.pixels(), state.animator.mips());
            if let Some(body) = web_sys::window().and_then(|window| window.document()?.body()) {
                let _ = body.set_attribute("data-animation-tick", &ticks.to_string());
            }
        }
        renderer.set_time(state.time_ticks);
        renderer.set_brightness(state.brightness);
        renderer.set_fog(if state.fog_enabled {
            vanilla_fog(state.render_distance, state.time_ticks, None, 0.0)
        } else {
            Fog::none()
        });
        renderer.set_debug_flags(DebugFlags {
            unlit: state.unlit,
            heatmap: state.heatmap,
            fog_off: !state.fog_enabled,
        });
        renderer.set_wireframe(state.wireframe);
        renderer.set_backface_culling(state.backface_cull);
        renderer.set_chunk_borders(state.chunk_borders);

        let max_distance = state.render_distance as f32 * 16.0;
        let chunks: Vec<_> = state
            .sections
            .iter()
            .filter(|(origin, _)| {
                let min_x = origin[0] as f32;
                let min_z = origin[1] as f32;
                let dx = if camera_position[0] < min_x {
                    min_x - camera_position[0]
                } else if camera_position[0] > min_x + 16.0 {
                    camera_position[0] - min_x - 16.0
                } else {
                    0.0
                };
                let dz = if camera_position[2] < min_z {
                    min_z - camera_position[2]
                } else if camera_position[2] > min_z + 16.0 {
                    camera_position[2] - min_z - 16.0
                } else {
                    0.0
                };
                dx * dx + dz * dz <= max_distance * max_distance
            })
            .map(|(origin, sections)| RenderChunk {
                origin: *origin,
                version: state.generation,
                sections,
                visible_sections: (1 << mcre_world::SECTIONS_PER_CHUNK) - 1,
            })
            .collect();
        renderer.render(view_projection, camera_position, &chunks, None);
        if state.debug_visible {
            let stats = renderer.frame_stats();
            let chunk_x = (camera_position[0].floor() as i32).div_euclid(16);
            let chunk_z = (camera_position[2].floor() as i32).div_euclid(16);
            let text = format!(
                "Camera: {:.1}, {:.1}, {:.1}\nChunk: {}, {}\nTime: {:.0}\nChunks: {} in range / {} loaded\nSections: {}\nDraws: {} opaque + {} translucent",
                camera_position[0],
                camera_position[1],
                camera_position[2],
                chunk_x,
                chunk_z,
                state.time_ticks,
                chunks.len(),
                state.sections.len(),
                stats.sections,
                stats.opaque_draws,
                stats.translucent_draws,
            );
            if let Some(info) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id("debug-info"))
            {
                info.set_text_content(Some(&text));
            }
        }
        state.renderer = Some(renderer);
    }
}
