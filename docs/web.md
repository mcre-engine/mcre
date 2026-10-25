# Web viewer

MCRE builds to `wasm32-unknown-unknown` and runs in the browser with `winit`'s
web backend and wgpu's WebGPU backend. The entry point is
`crates/mcre_renderer/examples/web/viewer.rs`; the native viewer is
`examples/native/viewer_native.rs` (selected by `examples/viewer.rs`).

```sh
just web        # build and serve http://localhost:8000
just web-build  # build target/web without serving
just web-smoke  # headless Chromium smoke test
```

`wasm-bindgen-cli` 0.2.129 and the `wasm32-unknown-unknown` target are
required. The first build downloads the Minecraft client jar (through the
`mcre_assets` build script) so `web/make_smoke_pack.py` can build its fixture.

## Resource packs

The browser build does **not** fetch anything from Mojang. It starts with an
empty placeholder atlas and a resource-pack file picker. Select a zipped pack
with `pack.mcmeta` at the archive root or under one enclosing folder; the zip
is copied into OPFS, extracted into `pack_cache/`, and its
blockstates/models/textures are baked into the atlas.
The demo scene is a 3×3 stone/grass plane. Animated block textures
(`.png.mcmeta`) play once the pack loads; the atlas updates at 20 Hz, or 40 Hz
for sprites with `interpolate: true`.

Everything the viewer writes is under the OPFS root for the page's origin
(`resourcepacks/`, `pack_cache/`, `saves/`). Browsers clear it with site data,
and other origins cannot read it.

## Saves

After loading a pack, select a zipped Java world with the second file picker.
The zip is extracted to `saves/upload/` in OPFS, then the viewer loads a 9×9
chunk area around the spawn point (modern
`dimensions/<namespace>/<path>/region/` layout or legacy `region/`). Chunks
missing from the zip are air. The current pack supplies the blockstates,
models, and textures, so load a pack first.

"Download world zip" re-zips `saves/upload/` and downloads it as `world.zip`.
Region write-back (saving edits) is still out of scope; `mcre_anvil` only
parses region files.

## Rendering notes

- The demo camera matches the native demo shot: eye `[8, 7, 26]`, FOV 70,
  noon (`time_ticks = 6000`), render distance 1, clouds off.
- `build_atlas_square` pads small packs to a square atlas. Non-square atlases
  make the RGSS shader's mip selection sample too high a level (the native
  parity path keeps its packed layout for the existing shot baseline).
- Wireframe pipelines fall back to `Fill` on wasm; the renderer does not
  request `POLYGON_MODE_LINE`.
- All meshing and `compute_light()` work runs on the main thread. The demo
  scene is deliberately small; a larger world needs worker-based meshing or
  a time-sliced budget.
- Only WebGPU is enabled. There is no WebGL fallback.

## Controls

Click the canvas to capture the mouse; press `Escape` to release it. If pointer
capture is unavailable, drag with the left mouse button to look. `W`/`A`/`S`/`D`
move, `Space`/`Shift` move up/down, and `Ctrl` boosts speed. Press `F1` or use
the Debug button for render distance, time, brightness, fog, lighting, geometry,
and chunk-border controls. Save worlds currently load a fixed 9×9 area; render
distance only changes visibility within that area, and camera-centered streaming
is not implemented yet.

## Smoke test

`just web-smoke` builds the bundle, installs the Playwright dependencies, and
runs `web/smoke.mjs` in headless Chromium with SwiftShader. The test:

1. captures the empty viewer,
2. uploads `target/web/smoke-pack.zip` through the file input,
3. waits for the atlas/pack load and the first animated-texture tick,
4. asserts the render changed, the canvas is 960×540, and the frame matches
   `web/demo-reference.png` (native demo render) within MAE 1.2 and 2.5 % of
   pixels over 16,
5. uploads `target/web/smoke-save.zip`, waits for the save load, and asserts
   the frame changed,
6. clicks "Download world zip" and checks the download,
7. exercises the debug panel and render-distance control, then checks pointer
   capture/release when supported by headless Chromium.

Regenerate the native reference after intentional visual changes:

```sh
just web-reference
```

The reference is a native render of the same demo scene. It needs the default
pack cache (the native viewer downloads it on first use). Regenerate it
whenever the renderer, demo scene, or Minecraft assets change (including
`mc-version` bumps).

## OPFS conformance

The shared `mcre_fs/tests/conformance.rs` suite runs against the native backend
with `cargo test -p mcre_fs`. To run it against OPFS, install a ChromeDriver
that matches your Chromium and point the wasm test runner at it:

```sh
CHROMEDRIVER=/path/to/chromedriver \
WASM_BINDGEN_USE_BROWSER=1 \
WASM_BINDGEN_TEST_WEBDRIVER_JSON=web/webdriver.json \
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
cargo test -p mcre_fs --target wasm32-unknown-unknown --test conformance
```

`web/webdriver.json` launches headless Chrome. Add
`goog:chromeOptions.binary` to use a specific Chromium (for example
Playwright's). This is not wired into CI yet because the driver and browser
versions must match.

## Not yet implemented

- Region write-back / saving edits. Saves can be imported and exported as
  zips, but modified chunks are not written back into the region files
  (`mcre_anvil` write support is a Phase 3 candidate).
- Keyboard focus/pointer-lock polish and touch input.
