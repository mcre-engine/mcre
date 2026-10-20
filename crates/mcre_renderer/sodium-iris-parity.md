# Sodium/Iris-parity rendering for `mcre_renderer`

Goal: match Sodium's terrain pipeline (and Iris's XHFP vertex extensions) exactly —
per-section meshing, compact 20-byte(+extensions) packed vertices, per-facing face
slices, regions with shared GPU vertex arenas, shared quad index buffer, and
batched draw submission (multi-draw-indirect feature-gated; `draw_indexed` loop
with `base_vertex` on Metal).

---

## Research summary (what we are matching)

### Sodium draw-call path (`DefaultChunkRenderer`)

- World divided into `RenderSection`s (16³); sections grouped into `RenderRegion`s
  of **8×4×8 sections** (128×64×128 blocks). One vertex-buffer **arena** per region
  (best-fit free-list allocator over one big `GlMutableBuffer`, staging-buffer uploads).
- Per section, geometry is split into **7 facing slices** (UNASSIGNED + 6 axis
  facings, `ModelQuadFacing` ordinals 0..6) laid out contiguously in the region
  arena. Metadata per section per pass (`SectionRenderDataUnsafe`, 48 B):
  `base_vertex`, `base_element`, `facing_list` (u56), `slice_mask` (u32),
  `is_local_index`, `vertex_count[7]`.
- **Block-face culling**: `getVisibleFaces(camera, section)` computes a 7-bit mask
  (branchless sign-bit compares, ±3-block epsilon for model overhang) intersected
  with the section's `slice_mask`; consecutive visible facing runs are merged into
  one draw command (`addSharedIndexedDrawCommands`).
- **Shared quad index buffer**: one global u32 buffer filled with the repeating
  pattern `4q+{0,1,2, 2,3,0}` per quad `q`. Draws reference a *range* of it with a
  `base_vertex`, so opaque/cutout geometry stores **zero indices per section**.
- **Draw submission**: per region, fill a `MultiDrawBatch` (arrays of
  `first_index / base_vertex / count`) and issue **one**
  `glMultiDrawElementsBaseVertex` (GL), `vkCmdMultiDrawIndexedEXT` (VK), or an
  indirect variant. Each vertex carries a **section-id byte** (`a_LightAndData.w`,
  0..255 within the region); the shader adds `Chunks[section_id].offset` (world
  origin of that section) — `_vert_position + _get_draw_translation(_draw_id)`.
- Translucent geometry uses **per-section local index buffers** (quad-sorted:
  BSP tree / topological / normal-relative heuristics) drawn the same batched way;
  sections drawn back-to-front.

### Sodium `CompactChunkVertex` — exact 20-byte layout

| Offset | Attr           | Format        | Contents |
|--------|----------------|---------------|----------|
| 0      | `a_Position`   | `RG32_UINT`   | x/y/z quantized to 20 bits each over a **32-block range centered −8** on the section origin (`q = ((8+v)/32 · 2²⁰) & 0xFFFFF`); hi word = bits 10..19, lo word = bits 0..9 |
| 8      | `a_Color`      | `RGBA8_UNORM` | shade·tint·AO color, **AO premultiplied** (`ColorARGB.mulRGB`) |
| 12     | `a_TexCoord`   | `RG16_UINT`   | u,v quantized to 15 bits (`round(uv·2³¹⁵)`), biased **1 quantum toward the quad's UV centroid**, sign of bias in bit 15; GPU re-expands with a small epsilon (anti-bleed) |
| 16     | `a_LightAndData` | `RGBA8_UINT` | light: `block+8`, `sky+8` each clamped 8..248 (lightmap uv = value/256); material bits (u8); **section id in region (u8)** |

### Iris XHFP extensions (appended after byte 20, selected by bitmask like `FormatAnalyzer`)

| Attr           | Format      | Encoding |
|----------------|-------------|----------|
| `BLOCK_ID`     | `u32`       | `((block_id + 1) << 1) \| (render_type & 1)` |
| `NORMAL`       | `i8×4`      | octahedral-encoded face normal xy + special "tangentEncode" xy (tangent xyz octahedral, w-sign folded into y), each component snorm8 via `NormI8.pack(x·127,y·127,z·127,w·127)` at offsets 0/8/16/24 |
| `MID_TEX_COORD`| `u16×2`     | UV centroid of the quad, `round(uv·32768)` |
| `MID_BLOCK`    | `i8×4`      | vertex position relative to its block (Iris `ExtendedDataHelper.computeMidBlock`), **byte 3 overwritten with block emission level** |

Full XHFP (all four attrs) → stride **36 bytes**. Normal computed per quad via
cross product of the diagonal edges (`computeFaceNormalManual`); tangent from UV
gradient (`computeTangent`, fallback to triangle 2, then Iris default +Y).

### Current `mcre_renderer` state (gaps)

- `Vertex` = 40 B of floats (absolute world positions, float UV/color/light).
- One mesh per 16×world-height column; one GPU buffer pair + one `draw_indexed`
  per chunk per pass; no shared index buffer; no regions/arenas; no face-slice
  culling; no section-id/draw-translation; light as two f32s + `textureLoad`;
  atlas anti-bleed via epsilon inset in UVs (not Sodium's centroid-bias trick).
- Passes: opaque(+cutout discard) + translucent — Sodium has SOLID / CUTOUT /
  TRANSLUCENT.

---

## Decisions (confirmed with user)

1. **Full Sodium parity**: per-section meshing, 8×4×8 regions, arenas, slices.
2. **Draw path**: Sodium-style batch fill; execute via `draw_indexed` loop with
   shared buffers + `base_vertex` (works on Metal/WebGPU core); feature-gate
   `MULTI_DRAW_INDEXED_INDIRECT` for Vulkan later.
3. **Full XHFP now**: all four Iris attributes implemented from the start with
   `FormatAnalyzer`-style bitmask selection (default: all on).

---

## Implementation plan

### Phase 1 — Vertex format + encoders (`vertex.rs`, new)

- `TerrainVertexLayout`: descriptor built from a `FormatFlags` bitmask
  (block_id/normal/mid_uv/mid_block), stride 20..36, attribute offsets exactly as
  `FormatAnalyzer` (extensions in order: blockId, normal, midUv, midBlock at
  20/24/28/32).
- Bit-exact encoder fns (port of `CompactChunkVertex` + `XHFPTerrainVertex`):
  `quantize_position`, `pack_position_hi/lo`, `encode_texture` (centroid bias),
  `encode_light`, `pack_light_and_data`, `pack_block_id`, octahedral +
  `tangent_encode`, `NormI8::pack`, `compute_mid_block` (port Iris
  `ExtendedDataHelper` exactly — fetch source during implementation).
- WGSL include (`terrain_vertex.wgsl`) with `_deinterleave_u20x3`, tex-bias
  re-expansion (`uv + bias·epsilon`), lightmap uv = `light/256`, octahedral decode.
- Unit tests: round-trip quantization error bounds, bias-sign behavior, light
  clamping, NormI8/octahedral golden values against Iris outputs.

### Phase 2 — Per-section mesher (`mesh.rs` rework, `fluid.rs` update)

- New output type `SectionMesh`: per-facing vertex groups
  (`vertices: [[PackedVertex]; 7]` or one `Vec` + `counts[7]`), section-local
  positions (origin = section corner; quantization range −8..+24 blocks allows
  neighbor overhang), plus per-face `facing` classification (quad normal →
  `ModelQuadFacing`; UNASSIGNED for non-axis-aligned).
- `mesh_section(state-slice, access, atlas, …)` replaces `mesh_chunk`; keeps
  existing vanilla AO/light/tint/variant logic but:
  - emits into facing buckets, no indices (opaque/cutout),
  - bakes color = shade·tint·AO as u8 (premultiplied AO),
  - encodes light as `(block+8, sky+8)` clamped bytes,
  - computes XHFP attrs per quad (normal/tangent/midUV/midBlock/blockId),
  - writes UVs in atlas-normalized 0..1 (quantizer does the 15-bit snap);
    drop the old `eps = 0.01` inset — rely on centroid-bias + shader epsilon.
- `fluid.rs`: emit into facing buckets likewise; fluid quads get face normals
  from their facing.
- `SectionPos`/local-section-index helpers (8×4×8 packing) and section-id byte
  `((sy&3)<<6?) — mirror Sodium `LocalSectionIndex` (x:3 bits, y:2, z:3).
- Port tests: adapt `mesh.rs`/`fluid.rs`/integration tests to unpack packed
  vertices via a test-only decoder.

### Phase 3 — Regions, arenas, GPU upload (`renderer.rs` rework)

- `RenderRegion`: `origin` (section coords), `GpuArena` (vertex buffer, bump +
  free-list coalescing, grow-by-allocate-copy-free like `GlBufferArena`), per-pass
  `SectionRenderData` (base_vertex, base_element, slice_mask, vertex_count[7],
  facing_list, is_local_index), cached `MultiDrawBatch`.
- Global `SharedQuadIndexBuffer`: u32, pattern `4q+{0,1,2,2,3,0}`, grow-only.
- Translucent: per-section local sorted index buffer arena (simple quad-order
  sort first — Phase 5 upgrades sorting).
- **Draw translation**: one storage buffer `chunks: array<vec4<u32>>` (256
  entries; xyz = section world origin in blocks, w spare), indexed by the
  per-vertex section-id byte. Bound in group 0.
- Pipelines: 3 passes (SOLID, CUTOUT, TRANSLUCENT) × {wireframe, cull} variants;
  vertex layout from `TerrainVertexLayout`; CUTOUT keeps `discard` at α<0.5.
  Sampler for lightmap becomes linear (uv-based), atlas stays nearest.
- Upload path: mesher results staged into arenas on the render thread (or via
  `queue.write_buffer`), `SectionRenderData` updated, stale segments freed.

### Phase 4 — Batch fill + draw submission (`renderer.rs`)

- Port `getVisibleFaces` (branchless compares with ±3 epsilon) and the
  `addSharedIndexedDrawCommands` / `addLocalIndexedDrawCommands` run-merging into
  a `MultiDrawBatch` (CPU struct of `Vec<DrawCmd{first_index, base_vertex, count}>`).
- Render loop: iterate visible regions → per pass → bind region vertex buffer +
  shared (or section-local translucent) index buffer **once** →
  `draw_indexed(first..first+count, base_vertex, 0..1)` per cmd.
- Feature-gate: if `MULTI_DRAW_INDEXED_INDIRECT` present, write cmds into an
  indirect buffer and issue `multi_draw_indexed_indirect` (Vulkan path; stub +
  cfg today, no Metal impact).
- Frustum culling stays caller-driven (viewer) but moves to per-section AABBs.

### Phase 5 — Remesh pipeline (`remesh.rs`, viewer integration)

- Jobs at section granularity (dirty set of `SectionPos`; a block edit dirties
  its section + border-adjacent sections). `MeshResult` = Vec of
  `(SectionPos, generation, SectionMesh)>`. Drop the whole-`World`-clone-per-job
  in favor of cloning only touched sections + neighbor borders (keep current
  clone-world approach initially if simpler; optimize after parity).
- Viewer: maintain regions; per frame build visible-section lists per pass;
  translucent: back-to-front section sort by center distance (as now), then
  Phase 5b: per-section quad sorting (start with Sodium's `StaticNormalRelative`
  heuristic; BSP/topological sorting explicitly deferred as future work).
- Debug UI: draw-command count, per-region arena usage, pass totals (proves the
  batching win).

### Phase 6 — Cleanup & verification

- Remove old float `Vertex`/`Mesh` column path, old pipelines, `translucent_start`
  hack; update `lib.rs` exports and `examples/viewer.rs`.
- `just ready` green (fmt, ck, tests, clippy `-D warnings`, typos, doc check).
- Visual parity check in viewer (demo world + Anvil save): AO, water layers,
  cutout foliage, fog, day/night lightmap.

### Deferred (documented, not in scope)

- Sodium translucent BSP/topological sorting (baseline quad sort only).
- Mipped atlas / `MIPPED_CUTOUT` pass, `GlBufferArena`-style staging-buffer
  zero-copy uploads, occlusion-cull graph (BFS visibility) — viewer keeps
  frustum-only culling for now.
- Shader-pack (OptiFine GLSL) compatibility layer — XHFP data is produced and
  decodable, but no pack loading.

---

## Files touched

| File | Change |
|------|--------|
| `crates/mcre_renderer/src/vertex.rs` | **new** — packed format, encoders, `FormatFlags` |
| `crates/mcre_renderer/src/mesh.rs` | section mesher, facing buckets; tests updated |
| `crates/mcre_renderer/src/fluid.rs` | facing buckets, packed output |
| `crates/mcre_renderer/src/region.rs` | **new** — `RenderRegion`, `GpuArena`, `SectionRenderData`, `MultiDrawBatch`, local-section-index |
| `crates/mcre_renderer/src/renderer.rs` | arenas, shared index buffer, chunks SSBO, 3 passes, batched draws |
| `crates/mcre_renderer/src/shader.wgsl` (+ new includes) | packed-vertex decode, XHFP varyings, lightmap uv sampling |
| `crates/mcre_renderer/src/remesh.rs` | section-granular jobs/results |
| `crates/mcre_renderer/examples/viewer.rs` | region/section bookkeeping, per-section culling, debug stats |
| `crates/mcre_renderer/tests/*` | adapt to packed vertices |

## Key risks

- **Test churn**: ~1 900 lines of mesh tests assert float vertices — mitigate
  with a `#[cfg(test)]` unpack helper that reconstructs floats from packed data.
- **AA overhang**: 20-bit/32-block quantization clips geometry beyond +24 blocks
  from section origin — models must stay within Sodium's range (they do in
  vanilla; verify with mods-free pack).
- **Translucent correctness**: baseline = section sort + quad sort may differ
  from Sodium's BSP on intersecting glass; acceptable interim.
- **World-clone jobs**: unchanged cost initially; section-granular cloning is a
  follow-up optimization inside Phase 5.
