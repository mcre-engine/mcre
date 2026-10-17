# Renderer Roadmap

Status tracker for renderer-adjacent work. Storage/culling/biome/pack changes
land in other crates but are tracked here alongside the renderer work they serve.

Storage decisions (locked):
- Dense section design ported directly from
  <https://github.com/mcre-engine/dense-vs-sparse-chunk-storage-bench>
  (`Section::{Homogeneous,Paletted,NonPaletted}` + `U256` occupancy + `optimize()`).
- Exact radius ring-buffer manager (`chunk_idx` via `rem_euclid`, stale-`pos` guard).
- Below-world reads stay `AIR` (not `VOID_AIR`) to preserve existing behavior.
- `WorldStorage::get` takes `&self` (bench used `&mut self`).

## M1 — Chunk storage

- [x] `mcre_world::section`: port `U256`, `PalettedSection`, `NonPalettedSection`, `Section`
      (promotion on overflow, demotion to homogeneous, `optimize()` demotion).
- [x] `mcre_world::Chunk`: `pos: ChunkPos`, lazy `Vec<Section>` with implicit AIR gaps,
      `new_at(pos)`, `optimize()`; keep `get_block`/`set_block`/`iter_blocks` signatures.
- [x] `mcre_world::World`: radius ring buffer (`WorldStorage` trait, `chunk_idx`,
      stale-`pos` guard, `add_chunk`, `optimize`).
- [x] Tests: section promotion/demotion/overflow unit tests, manager indexing/stale-guard
      tests, randomized consistency fuzz (chunk + world) across `optimize()`.
- [ ] Bench (`criterion`) port for storage micro-comparison (optional, deferred).

## M2 — Face culling tier 1 (full-cube)

- [x] `mesh_chunk` gains world/neighbor access; skip quad when neighbor
      `mcre_data::Block::can_occlude` using baked rotated `cullface`.
- [x] Absent neighbor chunk ⇒ air ⇒ face kept (unloaded borders render).
- [x] Same-class transparency rules (water/ice/glass — verify vs decompiled
      `shouldRenderFace`).
- [x] `World::set_block` dirty-marking + border-aware remesh in viewer;
      debug quad-count printout to prove the win.
- [x] Later (M6): VoxelShape extraction for silhouette-accurate culling;
      per-chunk frustum culling.
      (Tier-2 `shouldRenderFace` port: extracted occlusion boxes + face-slab
      intervals + exact coverage test; frustum AABBs per chunk in viewer.)
- [x] Later (light): incremental relighting (edits recompute the whole ring
      now); non-cubic quads use vanilla's cubic light path (`faceCubic`/
      `faceShape` weighting skipped); `emissiveRendering` not extracted; `sky_factor`
      uniform wired for a future day/night cycle.
      (Incremental region-superset relighting with differential tests;
      full non-cubic port incl. weighted blends + faceCubic center rule;
      per-state `emissive_rendering` extracted (magma + active sculk);
      day/night cycle drives `sky_factor` + fog.)
- [x] Later (culling): cross-block bars (iron↔copper) skip only via the
      `bars` tag rule, which needs the neighbor's connection props — currently
      renders; single-sided translucent liquids (underwater surface invisible
      with back-face culling); fast-graphics leaves option (`cutoutLeaves`
      flips the leaves rule — only fancy extracted); collision shapes evaluated
      against `EmptyBlockGetter` (vanilla shades with the real level; shapes
      that query neighbors may differ).
      (Cross-bars via extracted `bars` membership + connection props;
      double-sided liquids on by default (menu toggle); fast-leaves toggle;
      occlusion proven level-independent (uses `EmptyBlockGetter`), collision
      shapes for shading stay empty-context extracted.)

## M3 — Light + shading

- [x] Light storage (block + sky nibble data per section); static sky-fill for demo world.
- [x] Vanilla-style per-vertex corner AO (full-cube solids first), honoring parsed
      `ambientocclusion: false`.
- [x] Per-corner smooth light blending + flat-mode toggle.
- [x] Emissive fullbright from baked `light_emission`; distance fog in shader.

## M4 — Biome rendering

- [x] Extract `data/minecraft/worldgen/biome/*.json` from the jar (extend
      `download.rs`/data pipeline) → typed `Biome { temperature, downfall, effects }`
      in `mcre_data`. (68 biomes via `data_gen` jar read → `biomes.json`;
      `mcre_static_data_gen` accessors; swamp modifier extracted, noise
      formula deferred.)
- [x] Colormap loader in `mcre_renderer` for
      `textures/colormap/{grass,foliage,dry_foliage}.png`, vanilla bilinear sampling.
      (Pack-stack last-wins in `ResourcePackSet::build`; vanilla nearest-index
      sampling, not bilinear — colormaps are lookup tables.)
- [x] Per-block tint-kind registry (port of `BlockColors`) replacing hardcoded
      `GRASS_TINT`, gated by `tintindex`. (`TintSource` table incl. redstone
      power, stem age, tall-grass lower-half sampling, 5x5 biome blend.)
- [x] Biome storage: 4×4×4 cells per section + `biome_at()`; demo biome source.
      (Lazy per-section boxes, default plains; `World::biome_at/set_biome`;
      `BlockAccess::biome_at`; demo stays default plains.)
- [x] Special cases (were deferred to M6): swamp grass noise, snowy-skip.
      (Swamp: `BIOME_INFO_NOISE` SimplexNoise port in `mcre_core::noise`,
      bit-exact vs live-JVM oracle, flat wet/dry colors at -0.1 threshold;
      per-column noise through the 5x5 blend. Snowy: no tint code needed —
      `snowy=true` resolves to the untinted `grass_block_snow` model,
      covered by resolver test. Badlands/pale-garden via color overrides;
      dark-forest modifier already implemented.)

## M5 — Texture/resource packs

- [x] Scan + load user packs (folder and `.zip`) via `mcre_fs`.
      (Folder packs via `--pack <dir>` (repeatable, bottom-to-top); `.zip`
      reader exists in `download.rs` for the default pack — user zips route
      through the same `ResourcePack::load` once extracted. Zip-direct user
      packs deferred to Phase 2 pack UI.)
- [x] `pack_format`/overlay application (`pack_meta.rs` already parses them);
      pack-order config. (Current format 100.1 from jar `version.json`,
      jar-cross-checked in tests; overlay dirs layer above primary,
      last-listed wins; incompatible packs warn-and-load with `compatible` flag.)
- [x] Rework `build_atlas` to resolve each texture through the pack stack
      last-wins (models/blockstates already merge; textures don't).
      (`build_atlas` takes ordered roots, top-first hit wins; decode failure
      still falls back to magenta.)
- [x] Hot reload (rebuild atlas + lookup) — landed with Phase 2B pack UI
      (`Renderer::set_atlas` + worker swap + full remesh; egui Packs panel).

## M6 — Polish tiers

- [x] Shape-accurate culling (VoxelShape extraction in `data_gen`).
- [x] Per-chunk frustum culling (AABB).
- [x] Emissive/fog polish, biome special-cases, pack UI + hot reload.
      (Emissive done; fog/day-night cycle done; badlands/pale-garden via
      overrides + dark-forest modifier done; swamp noise + snowy-skip done;
      pack UI + hot reload landed in Phase 2B.)

## M7 — Shape-aware light + vanilla lightmap

- [x] Shape-aware light transport (`mcre_world::shape` shared occlusion
      helpers; per-edge occlusion in all relight paths; sky column only
      free-falls from level 15 under shapes; JVM oracle matrix + behavior
      tests; differential fuzz extended with slabs).
- [x] Vanilla lightmap (`mcre_renderer::lightmap`: CPU port of `lightmap.fsh`,
      16x16 baked RGBA texture regenerated on sky-factor/brightness change,
      per-vertex shader sample; raw-level vertex light attribute; viewer
      Brightness slider).

## M9 — Clouds

- [x] `clouds` module: `CloudRenderer` port (cell texture with tRNS alpha
      holes, ring mesh with caps/hull, drift + wrap, fog end
      `min(range*16, 2048)`), `clouds.wgsl`, renderer pipelines + per-cell
      mesh cache, viewer Off/Fast/Fancy + range UI and day/night tint.

Order: M1 → M2 → M3 → M4 → M5 → M6 → M7 → M9. Next: M8 (liquids).
Verify each milestone: `cargo ck`, `cargo lint -- -D warnings`,
`cargo test --workspace --all-features`, `cargo fmt --check`, `typos`,
25s viewer smoke (expect exit 124).
