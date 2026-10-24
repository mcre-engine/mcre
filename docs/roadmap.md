# MCRE Roadmap

Full-project writeup: where the project stands, what is finished, and what
comes next. Crate-level detail for the renderer track lives in
`crates/mcre_renderer/TODO.md`; this file is the cross-cutting plan.

## 1. Context

MCRE is an experimental Minecraft Java Edition client in Rust, targeting the
version in `mc-version` (currently 26.4-snapshot-4). Ground truth for vanilla
behavior is the decompiled client in `target/mc-src/<version>/` (`just src`).

**Guiding principles**
- Port vanilla algorithms exactly where feasible (verified against decompiled
  sources, not memory). Known intentional deviations are documented below.
- Data-driven over hardcoded: culling rules, tags, collision shapes, light
  values come from `data_gen` extraction, never name matching.
- `just ready` (ck + tests + lint + fmt + typos + docs) is green before
  anything lands; viewer smoke (25s, expect exit 124) confirms visuals.

**Workspace layout**
```
crates/
  mcre_core/        # primitives (no_std): directions, math types
  mcre_data/        # extracted JSON + typed schema (mcre_data/src/*.rs are
                    #   hand-written schema; the JSON files are generated)
  mcre_static_data_gen/ # build-time accessor generator (bins + getters)
  mcre_world/       # world model: sections, chunks, ring-buffer manager,
                    #   nibble light storage, flood-fill lighting
  mcre_assets/      # asset extraction: blockstate/model JSON → baked quads
  mcre_resource_pack/ # pack loading, model resolution, baked lookup table,
                    #   texture atlas input, per-block culling overrides
  mcre_renderer/    # wgpu renderer: atlas, chunk meshing, viewer example
  mcre_fs/          # platform-agnostic filesystem (native + web backends)
  mcje/, mcje_macros/, mcje_downloader/  # JVM bridge + game jar download
tasks/data_gen/     # JNI extraction: block/state JSON from the MC jar
```

**Known intentional vanilla deviations**
- AO corner attachment is keyed to world-space vertex position, not vertex
  slot (`AmbientVertexRemap`). Vanilla permutes AO with model-variant
  rotation; we keep AO fixed so only the texture rotates.
- Face culling uses extracted occlusion boxes with exact voxel-interval face
  slabs (f32, 16th-aligned content assumed); empty own slabs fall through to
  the coverage test per live-JVM probing.
- Leaves render fancy-style by default (fast toggle in debug menu).
- Depth compare is `LessEqual` (vanilla terrain equivalent) so coplanar
  overlay faces (grass/mycelium sides) resolve.
- Weighted AO on non-cubic quads clamps light channels to 15; smooth-face
  collision checks use empty-context extracted shapes.

## 2. Finished work

**M1 — Chunk storage** (`mcre_world`): dense sections
(`Homogeneous`/`Paletted`/`NonPaletted` + `U256`), lazy chunk sections,
radius ring-buffer manager, fuzz-tested across `optimize()`.

**M2 — Face culling tier 1**: baked rotated `cullface`, occluder rule via
`solid_render`, data-driven `skipRendering` (same-block self-skip bits,
liquid-fluid match, `face_occluding_ice_blocks` tag overlap), pack-level
`mcre/culling.json` overrides (last-pack-wins).

**M3 — Light + shading**: nibble light storage per section, whole-ring
flood fill (`compute_light`), per-vertex smooth AO + blending (vanilla
`BlockModelLighter` port), flat-mode toggle, emissive fullbright, fog,
`CardinalLighting` directional shade.

**Extraction pipeline** (`data_gen` + `mcre_static_data_gen`): per-state
`self_skip`, `collision_full`, `fluid`; per-block `liquid_fluid`,
`skip_against_tags`, `culling_tags` (tag membership read from the game jar —
runtime tag binding is absent in headless bootstrap); generated accessors
(`BlockState::{self_skip, collision_full, fluid}`,
`Block::{liquid_fluid, skip_against_tags, culling_tags}`,
`Direction::index`).

**Correctness fixes since**: atlas UV epsilon mapping (edge texels no longer
clipped), back-face culling + all-faces winding test, element-rotation
degrees + `CuboidRotation` rescale port (cross models span corner-to-corner),
per-position weighted variant selection (`Mth.getSeed` + `LegacyRandom`
ports, Java-cross-checked, multipart reseed semantics).

**Light API**: per-block values exposed (`ChunkLight::level`,
`World::light_level`); propagation stays whole-ring recompute (incremental
relighting deferred).

## 3. Phase 1 — Finish the current roadmap

Order-independent items first (tooling), then M4 → M5-core → M6.

### 1A. Viewer debug menu (egui) — done
Rationale: manual-verification harness for everything below; UI foundation
that Phase 2 pack UI reuses. Toolkit: `egui` overlay (native + wasm
compatible — no Dear ImGui).
- [x] Deps `egui`, `egui-winit`, `egui-wgpu` (pin to our `wgpu`); init in
      `resumed()`, event forwarding in `window_event`, draw pass after world
- [x] `F1` toggles menu (Esc stays exit); keybinds for common toggles
- [x] Lighting panel: AO Smooth/Flat (+ existing `L`), unlit-fullbright
      uniform flag, fog on/off + range sliders (`set_fog`), day/night slider
      (`set_sky_factor`), block/sky light heatmap overlay (vertex `light`
      already carries it)
- [x] Geometry panel: wireframe pipeline toggle (cached second pipeline),
      backface-cull toggle, chunk-border overlay
- [x] Info panel: live quad/vert counts, frame ms, camera pos; block
      inspector (existing raycast: name, state id, skip/fluid/tags, stored
      light at crosshair); force-remesh + recompute-light buttons
- [x] Renderer: pipeline cache keyed by `{wireframe, cull}`; debug uniform
      bitmask (`unlit`, `heatmap`, `fog_off`) — no shader permutations
- [x] Verify: suite green + per-toggle smoke checklist (heatmap vs inspector
      values must agree)

### 1B. M4 — Biome rendering — done
- [x] `data_gen`: `data/minecraft/worldgen/biome/*.json` → typed `Biome`
      in `mcre_data` (68 biomes, jar-direct read, generated accessors)
- [x] Colormap loader (`grass`/`foliage`/`dry_foliage`, pack-stack last-wins,
      vanilla nearest-index sampling) + per-block tint registry (port
      `BlockColors`), replacing `GRASS_TINT`
- [x] `mcre_world`: 4×4×4 biome cells per section + `biome_at()`; demo biome
      (default plains; `World::biome_at/set_biome`, `BlockAccess::biome_at`)
- [ ] Defer: swamp grass noise, snowy-skip → M6/Phase 2 (badlands/pale-garden
      overrides + dark-forest modifier work)

### 1C. M5 core — Pack stack (no UI; UI is Phase 2) — done
- [x] `build_atlas` resolves textures through the pack stack, last-wins
- [x] `pack_format` gating + overlay application in `ResourcePackSet::build`
      (current 100.1, jar-verified; warn-and-load with `compatible` flag)
- [x] Pack-order config: `--pack <path>` (repeatable, bottom-to-top)

### 1D. M6 — Polish + deferred backlog — done
- [x] VoxelShape extraction → tier-2 culling; per-chunk frustum culling (AABB)
      (304 deduped occlusion shapes; exact face-slab + coverage port verified
      against live-JVM probes; frustum per chunk in viewer)
- [x] Incremental relighting; non-cubic AO weighting; `emissiveRendering`
      (region-superset relight, differential-tested; full weighted-blend port;
      per-state emissive (magma + active sculk) in dispatch and sampling)
- [x] Culling nuances: cross iron↔copper bars, double-sided-liquid option,
      fast-leaves toggle, level-context collision shapes
      (`MeshOptions` + debug-menu toggles; occlusion proven level-independent)
- [x] Fog/day-night source wiring (time source drives `sky_factor` + fog)

## 4. Phase 2 — New systems

### 2A. `mcre_anvil` — load-only world loading (approved scope) — done
- [x] New crate: owned-tree NBT parser, `.mca` region reader (sector table,
      gzip/zlib/uncompressed payloads); native path helpers use `std::fs`,
      the browser uses `mcre_fs` + byte-level APIs
- [x] Palette → `BlockState` via `Block::state()` + `PropKey/PropVal`
      (`id`/`properties` new form, `Name`/`Properties` legacy, `{"":name}`,
      bare-string defaults); biome name → `Biome`, per-block→cell majority
- [x] Bit storage: vanilla `SimpleBitStorage` layout (no straddle,
      `ceil(size/vpl)` longs) + block/biome `Strategy` bit rules from mc-src
- [x] Lighting via existing `compute_light()` post-load
- [x] Viewer `--world <save-dir>` (spawn-area ring, camera at spawn surface);
      unloaded ring positions stay air
- [x] Tests: 7 unit (synthetic NBT/region, resolvers) + real-save validation
      (9 chunks, 313887 blocks, 0 unknown, bedrock/-64 + sky-15 spot checks)
- [x] Version tolerance: older saves load best-effort (26.3 biome cells are
      4x4x4, not per-block — disambiguated by data length; unknown states
      become air with counts); corrupt chunks are skipped with a warning,
      fatal save errors print cleanly instead of panicking
- (Saving/region write-back explicitly out of scope — Phase 3 candidate;
  `take_dirty` already tracks edits.)

### 2A follow-ups — streaming + light/edit performance (landed)
- [x] Viewer ring streamer: `--world` starts with instant 3×3, fills
      `--radius N` (default 4) in background nearest-first, rebases on
      camera-chunk move; torus slots (no unload hitches), 1 chunk/job so
      edits never wait behind loads; far meshes + GPU buffers pruned
- [x] Targeted single-edit relight (vanilla-style removal wave + reseed,
      early-out on no-change): ~550ms → ~0–2ms per edit in debug;
      worker snapshots `pending_light_edits`, viewer acknowledges on apply
      so every steady-state edit stays on the fast path
- [x] Per-chunk emitter lists (`Chunk::emitters`, maintained in `set_block`)
      seed full + region paths — no more whole-ring block scans;
      differential-tested region ≡ full
- [x] Renderer GPU buffer cache (`RenderChunk{origin, version}`): steady
      state uploads zero (was ~13MB/frame re-upload); upload once per remesh
- [x] Spawn camera placed above its own column; shader `MIN_LIGHT` floor so
      light 0 renders dark instead of pure black (superseded in M7: the
      vanilla lightmap makes light 0 true black again, as vanilla)
- [x] Vanilla lightmap curve (`Lightmap.getBrightness`: `v/(4-3v)`,
      additive block+sky in shader; superseded by the full `lightmap.fsh`
      port in M7): AO mids render correctly dark, tunnel
      fade reaches true black with no shelf; diagonal dirty-marking
      (corner edits + streamed chunks remesh all 8 neighbors) so freshly
      loaded corners don't show stale baked AO/light

### 2B. Custom resource packs + in-viewer UI (approved: UI-driven) — done
- [x] Scan folders + `.zip`s via `mcre_fs` (`discover` module: folders and
      zips, `pack.mcmeta` validation, incompatible-format warn flag,
      broken packs listed with errors, never fatal)
- [x] Stack assembly with overlays/directory filters; document
      `mcre/culling.json`
- [x] Hot reload (rebuild atlas + lookup on stack change):
      `Renderer::set_atlas` swaps the GPU texture without rebuilding
      pipelines; worker recreated on the new lookup table; full remesh
      re-uploads via version bumps
- [x] egui pack panel (reuses 1A integration): active stack bottom→top
      with reorder/remove, available list with enable, rescan, reload,
      per-pack errors; selection persisted to `pack_stack.json`
- [x] Tests: merge precedence (existing), broken-pack isolation, zip-backed
      loading + extraction cache + zip-slip rejection; `--reload-test`
      exercises the full hot-reload path headless

### 2C. `mcre_fs` web completion + wasm viewer (approved: viewer runs on wasm)
- [x] Web backend to full `Fs`-trait parity (OPFS handles, dir iteration,
      persistence, quota/error mapping); conformance suite on both backends
- [x] wasm viewer: `winit` web backend, wgpu WebGL/WebGPU limits (atlas caps,
      no main-thread sync handles — worker/async OPFS), no direct Mojang
      fetch in browser (bundled or user-provided packs)
- [x] `just web` (build + local serve); CI headless-browser smoke with
      screenshot comparison vs native demo render
- [x] Design constraints: single-threaded wasm (mesh + `compute_light`
      budgets) documented in `docs/web.md`; OPFS paths are origin-private
- [x] OPFS save import/export: byte-level `mcre_anvil` APIs
      (`RegionFile::from_bytes`, `chunk_nbt`, `read_gzip_nbt_bytes`,
      `read_spawn_from_root`, `load_chunk_from_nbt`) plus a world-zip picker;
      loads the 3×3 spawn area and re-zips OPFS saves for download. Region
      write-back stays out of scope.

### 2D. Milestone queue after Phase 2 (approved order)

M7 — Shape-aware light + vanilla lightmap — done
- [x] Shape-aware light transport in `mcre_world`: new `shape` module
      (`is_empty_shape`, `shape_occludes`, face slabs/boxes) shared with the
      mesher; every relight path applies per-edge occlusion and the sky
      column only free-falls from level 15 through non-occluding, non-damping
      cells; removal cascades use the propagated-removal bound
- [x] JVM oracle probe (16 representative states: slab/stairs orientations,
      fence, panes, carpet, snow, leaves, fluids) captured state properties,
      face occlusion pairs and the full `shapeOccludes` matrix; tests assert
      properties, the 16x16x6 matrix, slab column behavior (bottom slab
      blocks the column below, top slab keeps it above), and removal/restore
      of shape blocks; differential relight fuzz extended with both slab
      halves
- [x] Vanilla lightmap: CPU port of `lightmap.fsh` (`mcre_renderer::lightmap`)
      with the default attributes (block tint `#FFD88C`, block factor 1.4,
      brightness = gamma, default 0.5); baked to a 16x16 RGBA texture on
      sky-factor/brightness change; vertices carry raw `(block, sky)` levels
      and the vertex shader samples the lightmap per vertex (`block.vsh`
      semantics)
- [x] Viewer Brightness slider (0..=1) re-baking the lightmap live

M9 — Clouds — done
- [x] `mcre_renderer::clouds`: vanilla `CloudRenderer` port — 12x4-block
      cells from `textures/environment/clouds.png` (1-bit grayscale + tRNS,
      alpha<10 empty; neighbor-emptiness bits packed like `packCellData`),
      cell-anchored mesh around the camera's cell (ring enumeration, caps for
      above/inside/below, inside-face hull, `0,1,2,2,3,0` quads), drift
      `0.030000001`/tick with the `+3.96` z offset and texture wrap
- [x] `clouds.wgsl`: port of `clouds.vsh`/`clouds.fsh` (packed
      direction/flags, `0xF03CC3`/`0x6666F0`/`0xC3F066` corner bits, face
      colors 0.7/1.0/0.8/0.9, inside-face corner reversal, distance fog fade
      by `min(range*16, 2048)`)
- [x] Renderer: pipelines (fancy cull back / flat no cull, alpha blend,
      depth write), per-cell mesh cache rebuilt when the camera crosses a
      cell, status (Off/Fast/Fancy) + range (2..=128, viewer default 32)
      + height (192.33) + color setters; viewer loads the texture through the
      pack stack, drifts on real time (20 ticks/s), tints by the day/night
      scalar (`Timelines.NIGHT_CLOUD_COLOR_MULTIPLIER`)

M8 — Liquids — done
- [x] `mcre_world::fluid`: vanilla `FlowingFluid` state mapping
      (`LiquidBlock.stateCache` 0→source, 1..7→`8 - level`, 8..15→falling;
      waterlogged blocks hold a source), `ownHeight = amount / 9`,
      `MAX_FLUID_HEIGHT`, `isFaceSturdy` from the extracted support mask
- [x] Data extraction: `isFaceSturdy` 6-bit mask, `isIceBlock`,
      `half_transparent`, recursive `BLOCKS_FLUID_FLOW` tag; shape occlusion
      (`Shapes.blockOccludes` port) validated against the live JVM on all
      30,864 state/direction pairs
- [x] `mcre_renderer::fluid`: `FluidRenderer.tesselate` port — neighbor
      heights + `calculateAverageHeight` weighting, `getFlow` (incl. falling
      `-6` y pull and the recurse-below rule), top/bottom/side quads with
      vanilla offsets (`bottomOffs`), flow-aligned top UVs, half-height side
      UVs, `water_overlay` for half-transparent/leaves neighbors, exact face
      occlusion, vanilla back faces behind the double-sided option
- [x] Translucent fluid pass: `Mesh.translucent_indices`, alpha-blended
      pipelines (`fs_translucent`), second draw range per chunk; water keeps
      the texture alpha (vanilla 180/255) and 5x5 biome-blend tint, lava is
      opaque untinted
- [x] Viewer: demo pond, underwater fog (`Camera.calculateFogType` water +
      eye-block lava approximation, `WATER_FOG_COLOR`/lava colors, 70° → 60°
      FOV at the default `fovEffectScale`)
- [x] Parity capture: water-vision fog scaling and water-animation phase are
      sampled from vanilla and replayed by MCRE; underwater shot FOV respects
      the spec's `fovEffectScale: 0` behavior
- [x] Camera fog uses `FluidState.getHeightForCamera` (including the same-fluid
      block above), so moving through a water column no longer drops the
      underwater state
- [x] Underwater clear color uses the water/lava fog color; translucent
      terrain samples atlas mip 0 to avoid sprite-edge seams on water surfaces

### M10. Texture animations — done
- [x] `.png.mcmeta` animation metadata (`frametime`, `frames`, `width`/`height`,
      `interpolate`), strip/grid frame extraction, per-frame mip chains
- [x] `AtlasAnimator` updates the CPU atlas at 20 Hz (interpolated sprites at
      40 Hz); `Renderer::update_atlas` re-uploads pixels + mips
- [x] Native and web viewers advance animations; shot and demo-shot modes stay
      on frame 0 for deterministic parity
- Torch flicker is not present in 26.4-snapshot-4 (no `flicker` shader or
  source); skipped.

Current queue: M11 particles → M12 block entities → M13 entities.

### M11 — Particles (in progress)
- [x] Native-viewer terrain debris on block break: respects
      `shouldSpawnTerrainParticles`, resolves inherited model `particle`
      textures already present in the terrain atlas (otherwise uses the first
      baked quad texture), samples the atlas and lightmap, interpolates/ticks
      particles, and draws depth-tested blended billboards.
- [ ] Complete vanilla particle providers/events and shape-accurate particle
      spawning/collision; the current debris spawn pattern uses unit-cube
      bounds for every block shape.

## 5. Open decisions / risks
- UI toolkit locked to `egui` (native + wasm). Revisit only if it fights
  `winit`/`wgpu` versions.
- Anvil block-entity/entity loading deferred (needed for chests/signs
  visuals later — Phase 3).
- Wasm wgpu limits may force atlas-size caps or texture downscaling —
  measure during 2C, don't pre-design.
- Execution order: 1A → 1B → 1C → 1D, then 2A → 2B → 2C (anvil gives the
  wasm viewer real terrain; packs land before web so the web build inherits
  hot reload).
