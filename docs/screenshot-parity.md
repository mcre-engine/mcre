# Parity shots

A parity shot captures the **same frame** in vanilla Minecraft and in mcre —
same resolution, FOV, brightness, camera angle, world time, weather and render
distance — so the two PNGs can be diffed pixel-by-pixel while building the
1:1 vanilla renderer.

Both pipelines are driven by one JSON **shot spec** (`crates/mcre_shot`).

## Requirements

- `JAVA_HOME` pointing at a JDK (the embedded client JVM and the `mcje` build
  need it).
- A generated world, e.g. `just gen-world` (writes `target/gen_world/world`).
- A display — the vanilla client opens a real window (it can stay in the
  background; it must not be covered by a full-screen app on macOS or the
  captured frame may be swapped out).

## Quickstart

```sh
just gen-world                                    # once: build the shared world
just shot docs/shots/noon.json                    # both engines + diff
# -> target/shots/noon.vanilla.png
# -> target/shots/noon.mcre.png
# -> target/shots/noon.diff.png
```

The `just shot` recipe chains `shot-vanilla`, `shot-mcre`, and `shotcmp`. The
spec's `name` field (`"noon"` here) picks the output filenames.

## Spec format

`docs/shots/noon.json`:

```json
{
  "name": "noon",
  "world": "target/gen_world/world",
  "eye": [8.5, 80.0, 8.5],
  "yaw": 45.0,
  "pitch": 25.0,
  "fov": 70.0,
  "brightness": 0.5,
  "time_ticks": 6000,
  "width": 1280,
  "height": 720,
  "render_distance": 8,
  "clouds": "off",
  "settle_ms": 4000
}
```

| Field            | Meaning |
|------------------|---------|
| `name`           | Output filename prefix (`<name>.vanilla.png`, `<name>.mcre.png`). |
| `world`          | Path to the Anvil save dir used by both pipelines. |
| `eye`            | Camera position `[x, y, z]` in world blocks. |
| `yaw`, `pitch`   | Camera rotation, degrees. mcre's viewer applies a 180° yaw offset internally so the **vanilla** angle is the one you set here. |
| `fov`            | Vertical field of view in degrees (30–110). Both pipelines render exactly this value; the vanilla run zeroes `fovEffectScale` so flight/sprint modifiers cannot widen it. |
| `brightness`     | Minecraft `gamma` (0.0 – 1.0). |
| `time_ticks`     | Frozen world time (0 – 24000, 6000 = noon). |
| `width`,`height` | Output resolution. |
| `render_distance`| Chunk render radius (also patches `simulationDistance`). |
| `clouds`         | `"off"`, `"fast"`, or `"fancy"` (vanilla cloud setting). |
| `settle_ms`      | How long to wait after the world loads before verifying. |

## How the vanilla capture works

`cargo run -r -p vanilla_shot -- --spec <spec>` runs `tasks/vanilla_shot`
which:

1. **Prepares the world** (`tasks/vanilla_shot/src/prep.rs`):
   - symlinks the world into `target/shot/client` and writes a
     playerdata file for a fixed UUID (`00000000-0000-4000-8000-000000000001`,
     username `McreShot`) with the player in creative + flying at the spec's
     eye position;
   - patches `level.dat` to set `GameType: 1` (Creative) and
     `DataVersion`;
   - patches `data/minecraft/world_clocks.dat` to freeze the overworld
     clock (`paused: 1`, `total_ticks: <spec.time_ticks>`, `rate: 1.0`,
     `partial_tick: 0.0`) — the file shape is
     `{data: {<dim>: {…}}, DataVersion}` matching the SavedData codec;
   - patches `data/minecraft/weather.dat` to permanently clear weather;
   - writes `options.txt` with `onboardAccessibility: false` (skips the
     narrator welcome screen so `--quickPlaySingleplayer` runs without
     manual input), `fov`, `fovEffectScale: 0` (see below), `gamma`,
     `renderDistance`, `music: 0`, no bobbing, no fullscreen, etc.

   The freeze keeps the player in creative flight, and vanilla multiplies
   the FOV by 1.1 while flying (`AbstractClientPlayer#getFieldOfViewModifier`).
   `fovEffectScale: 0` makes `Camera#tickFov` lerp that modifier to exactly
   1.0, so the effective FOV equals the option and the spec's `fov` is the
   FOV of both images. Without it vanilla renders ~10% wider than mcre.

2. **Boots the client** via JNI in `mcje` with
   `--quickPlaySingleplayer <worldName>`, which fires once the title-screen
   resources finish loading.

3. **Drives the client** through `crates/mcje/java/mcre/ShotDriver.java`
   executed on the render thread:

   | step   | effect |
   |--------|--------|
   | `wait_for_world` | polls until the client's `player` is non-null and the render thread is alive. |
   | `verify_camera`  | reads the player's eye, rotation and effective FOV; if out of spec (0.001 m, 0.05° and 0.05° epsilons), teleports via `ServerGamePacketListenerImpl#teleport`. The feet are placed so `feet + getEyeHeight()` lands exactly on `spec.eye` (`mcre_shot::EYE_HEIGHT` = 1.62, the standing player height). A wrong FOV fails the run instead of silently producing a mismatched image. |
   | `verify_framebuffer` | reads the window size; if it doesn't match `spec.width × height`, calls `Window#setWindowed(logical_w, logical_h)` and retries (handles macOS Retina 2×). |
   | `verify_clock`   | reads `ClientLevel.getOverworldClockTime()` (inherited from `Level`); must equal `spec.time_ticks`. |
   | `hide`           | `Minecraft.gui.hud.toggle()` if not already hidden. |
   | `grab`           | `Screenshot.grab(gameDirectory, name, mainRenderTarget, 1, cb)`. The callback is empty; the actual write happens on `Util.ioPool()` once the GPU readback fence drains. |
   | `stop`           | `Minecraft.stop()` to clean up. |

4. **Waits for `screenshots/<name>.png`** to appear and have a stable size
   for ≥ 400 ms (the GPU readback is async; the file is written on the IO
   pool after the readback fence fires).

5. **Copies** the PNG to `target/shots/<name>.vanilla.png` and validates the
   dimensions match the spec.

## How the mcre capture works

`cargo run -r -p mcre_renderer --example viewer -- --shot <spec>` runs the
WGPU viewer in offscreen mode (`crates/mcre_renderer/examples/viewer.rs`):

1. Loads the spec, applies fog/time/clouds/render-distance.
2. Imports chunks around the camera (radius 8) from the Anvil save.
3. Streams further chunks in the background while the camera frames settle.
4. Once the mesh has been quiet for `SHOT_QUIET_FRAMES` consecutive frames,
   captures the current render target to `target/shots/<name>.mcre.png` via
   `write_png`.

## How the diff works

`cargo run -r -p shotcmp -- <a.png> <b.png> <diff.png> [--amplify N] [--max-mae X]`
prints mean absolute error and the share of pixels whose worst channel
differs by more than 4 / 16, and writes an amplified grayscale PNG to
`diff.png`. With `--max-mae X`, the exit code is 1 when the MAE exceeds X
(for scripted regression checks).

## Rendering parity notes

The capture only lines up if the two renderers agree on these details. Each
one was verified against the 26.4 sources; changing any of them will move the
MAE by a lot.

- **Colour space.** The vanilla terrain pass renders into an `RGBA8_UNORM`
  target, so all shader math happens in gamma space and the stored bytes are
  the shader outputs. mcre therefore uses an `Rgba8Unorm` atlas, a non-sRGB
  surface and a non-sRGB capture target. Any sRGB conversion adds ~10 MAE.
- **Camera.** `mcre_yaw = vanilla_yaw + 180`, `mcre_pitch = -vanilla_pitch`,
  eye height `1.62` (`Avatar.DEFAULT_EYE_HEIGHT`). `vanilla_shot` reads
  `Camera#getFov()` after settling and fails the run if it is more than
  `0.05°` from the spec.
- **FOV.** Creative flight multiplies the vanilla FOV by `1.1`. The prep
  writes `fovEffectScale:0.0`, which pins the modifier to `1.0`, so the
  spec's `fov` is the rendered FOV.
- **Graphics preset.** `Minecraft` applies the saved graphics preset *after*
  loading `options.txt`, and the `CUSTOM` preset is a no-op. The prep writes
  `graphicsPreset:"custom"` plus every quality key it cares about, otherwise
  the preset would overwrite render distance, clouds and friends.
- **Light levels.** `sample_lightmap.glsl` uses `uv / 256.0 + 0.5 / 16.0` and
  `LightCoordsUtil.smoothBlend` averages the four samples in quarter-level
  units, so the 16×16 lightmap is sampled at a *fractional* coordinate and
  bilinearly blends between texels. mcre carries the level as `f32` and
  encodes `level * 16 + 8` (`8..248`); truncating to an integer level costs
  ~0.26 MAE on the noon shot.
- **Vertex colour.** Vanilla multiplies the lightmap sample into the vertex
  colour in `terrain.vsh` and interpolates the product. mcre does the same;
  interpolating AO and light separately and multiplying per fragment is
  visibly different on large faces.
- **Fog.** Two independent linear fogs (environmental `0..1024` and
  render-distance `115.2..128` at distance 8) are combined with `max`, then
  mixed with `FogColor.rgb` lerped `0.1866` toward `#78a7ff`.
- **Texture filtering.** `TextureAtlas` uses a `NEAREST` sampler with four
  mip levels. Vanilla's `sampleNearest` only diverges from plain nearest when
  minified, where `sampleRGSS` takes over. mcre samples nearest at mip 0, so
  the far field is slightly sharper than vanilla.

Known residual on `docs/shots/noon.json` (1280×720): MAE `1.6486`
(RGB `2.198`), 23.4 % of pixels over 4, 5.3 % over 16. The remaining error is
concentrated in per-block AO corner attachment and in minified texture
sampling; swapping AO on/off on either side moves the MAE to ~5.5, so the
overall AO strength is right.

## Adding a new shot

1. Copy `docs/shots/noon.json` and tweak the fields (keep `world` and
   `time_ticks` matching a chunk that actually exists in the world, or the
   camera will render the void / unloaded terrain).
2. `just shot docs/shots/<name>.json`.

## Common issues

- **"Welcome to Minecraft!" screen blocks quickplay**: the prep writes
  `onboardAccessibility: false` so this should never appear. If it does,
  delete `target/shot/client/options.txt` and re-run.
- **Camera mismatch every attempt**: the `verify_camera` loop has 3
  attempts with `FIX_SETTLE` (1.5 s). If the server-side teleport never
  converges, the most common cause is the world being empty at that
  position (player is unloaded).
- **`client fov is X, want Y (a camera modifier is active)`**: something
  modified the effective FOV, so the vanilla image would not line up with
  mcre. Check that `options.txt` (as written by `prep.rs`, not the copy the
  client rewrites on exit) still has `fovEffectScale: 0.0` and that the
  player is not using a bow/spyglass.
- **`world_clocks.dat` not parsed**: the file must use the
  `{data: {…}, DataVersion}` shape, not the bare per-dimension map — the
  vanilla SavedData codec for `SavedDataType(world_clocks)` wraps the map
  under `data`.
- **Framebuffer never reaches the target size**: the loop in
  `verify_framebuffer` re-calls `Window#setWindowed` with the logical size
  (`spec_w / scale_x`, `spec_h / scale_y`) where the scale is detected
  from the first reading. On unusual DPI setups this may not converge; the
  loop gives up after 5 attempts.
- **Screenshot never written**: the async GPU readback (`queueFencedTask`)
  fires every frame in `Minecraft.run`; the file appears within ~1 s under
  normal conditions. If `wait_for_screenshot` times out, check that the
  client window isn't occluded on macOS and that the IO pool isn't
  blocked.
- **Music plays during capture**: `options.txt` writes `music: 0` to mute
  it. If you hear music, the options file is being regenerated by a
  previous run that loaded it with the default — re-run from a clean
  `target/shot/client`.

## Layout

```
crates/mcre_shot/                 # shared spec types (ShotSpec, Clouds, etc.)
crates/mcje/                      # embedded JVM + JNI client control
  java/mcre/ShotDriver.java       # render-thread step implementation
  src/client.rs                   # JNI handles (player state, window, etc.)
tasks/vanilla_shot/               # vanilla pipeline orchestration
  src/main.rs                     # drive() — wait/verify/hide/grab/copy
  src/prep.rs                     # world + options + world_clocks/weather patches
  src/assets.rs                   # runtime asset-object download
tasks/shotcmp/                    # PNG diff tool
crates/mcre_renderer/examples/viewer.rs   # mcre offscreen shot mode
```