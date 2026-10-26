# PR #57 review — remediation plan

Review of `major-work` → `main` ("Rendering milestone 1"). This file tracks every
confirmed finding, the intended fix, the test that proves it, and the order of
work. Items marked **deferred** are design work that should not block the merge
but must have an issue; the rest are implemented on the branch.

Vanilla references are to `target/mc-src/26.4-snapshot-4/` (decompiled client)
and `target/downloads/26.4-snapshot-5/mc.jar` (data). Paths are repo-relative.

## Priority key

| Tier | Meaning |
|------|---------|
| P0 | Corrupts output, deletes data, or blocks a normal workflow. Fix before merge. |
| P1 | User-visible bug or wrong data, small and contained. Fix in this PR. |
| P2 | Hardcoded/non-data-driven structure or third-party-pack correctness. Fix now if cheap, otherwise issue. |
| P3 | Hardening, docs, dead code. Low risk, do last. |

## Status

| ID | Finding | Tier | State |
|----|---------|------|-------|
| R1 | Arena vertex offsets misaligned after growth | P0 | done |
| R2 | NBT strings decoded as standard UTF-8 | P0 | done |
| R3 | Unknown palette entry discards the whole chunk | P0 | done |
| R4 | `set_windowed` called off the render thread | P0 | done |
| R5 | Night cloud colour is an approximation | P1 | done |
| R6 | Region header `sector_count == 0` panics | P1 | done |
| R7 | `grass_block` break particles are tinted | P1 | done |
| R8 | Pack filters applied N times per pack | P1 | done |
| R9 | `gen_world` reuses a world built with another seed | P1 | done |
| R10 | RCON reply framing uses the wrong size | P1 | done |
| R11 | Overworld timeline/dimension attributes hardcoded | P2 | deferred |
| R12 | Sky-less dimensions imported fully lit | P2 | done |
| R13 | Entity cubes hand-transcribed (~1.1k lines) | P2 | deferred |
| R14 | Block tint table hand-mirrored from `BlockColors` | P2 | deferred |
| R15 | `PackVersion::CURRENT` hardcoded | P2 | deferred |
| R16 | Pack `filter` patterns never match | P2 | done |
| R17 | Pack overlays replace, not merge, per-pack data | P2 | done |
| R18 | `data_gen` hardcoded tag fallbacks | P2 | done |
| R19 | Download caches poisoned by interrupted writes | P2 | done |
| R20 | `pregen` swallows worker panics | P2 | done |
| R21 | NBT/decompression limits, chunk position validation | P3 | done |
| R22 | Docs/API drift | P3 | done |

---

## R1 — Arena vertex offsets are misaligned after growth (P0)

**Where** `crates/mcre_renderer/src/region.rs:19,21,226-251,283-296,312-316`

**Problem** `INITIAL_ARENA_BYTES = 1 << 20 = 1_048_576` is not a multiple of
`VERTEX_STRIDE = 44` (`size_of::<TerrainVertex>()`); 1_048_576 = 44 × 23_831 + 12.
`FreeList::grow` appends a free block at the old byte capacity whenever the tail
block does not already end there — which is the normal path, because `grow` only
runs when no free block fits (arena full, top allocated, free list empty or
fragmented). `GpuArena::alloc` stores `(offset / VERTEX_STRIDE)` while
`write_buffer` used the exact byte offset, so the draw reads 12 bytes early.
`free()` reconstructs the offset from the truncated index, so the freed range
overlaps the tail of the preceding allocation.

**Fix** Make every capacity stride-aligned, then assert the invariant.

1. `const INITIAL_ARENA_BYTES: u64 = align_up(1 << 20, VERTEX_STRIDE);` with
   a `const fn align_up` (const-compatible form: `(v + a - 1) / a * a`).
2. Round the capacity returned by `FreeList::grow_capacity` up to
   `VERTEX_STRIDE` before returning it, so `grow`'s appended block always starts
   aligned.
3. `debug_assert_eq!(offset % VERTEX_STRIDE, 0)` in `GpuArena::alloc`, plus a
   `#[test]` that grows the list by non-multiple sizes
   and asserts every offset is stride-aligned and that freeing/reallocating
   never overlaps.

**Test** Unit test in `region.rs` (`free_list_grows_on_stride_boundaries`):
allocate 16 vertices at a time until `alloc` returns `None`, assert
`grow_capacity` output, `grow`, then assert every subsequent offset is a
multiple of `VERTEX_STRIDE` and round-trip `free` + `alloc` produces the same
offset. `cargo test -p mcre_renderer region`.

**Risk** Offsets change for existing regions, which is invisible (arena is
rebuilt per frame from mesh data). No persistence.

---

## R2 — NBT strings are decoded as standard UTF-8 (P0)

**Where** `crates/mcre_anvil/src/nbt.rs:266-270` (`name`), `:352` (every
`STRING` payload), `:435` (`find_root_string`), writer `:147-151`.

**Problem** Java NBT uses `DataOutput.writeUTF` / `DataInput.readUTF`
(`StringTag.java:21`), i.e. modified UTF-8: NUL is `C0 80`, supplementary
characters are CESU-8 surrogate pairs. Both are invalid standard UTF-8, so
`String::from_utf8` rejects them:
`Error::InvalidUtf8` aborts `level.dat` parsing (viewer exits) and rejects the
whole chunk when a sign or custom name contains an emoji.

**Fix**

1. Add `fn decode_modified_utf8(bytes: &[u8]) -> Result<String, Error>`:
   ASCII; 2-byte (accept `C0 80` → U+0000, reject other overlong); 3-byte with
   surrogate-pair recombination (`D800..DBFF` + `DC00..DFFF` → one `char`);
   accept 4-byte standard UTF-8 as a liberal extension (Rust's writer emits it
   today and third-party tools may too); reject lone surrogates, truncated
   sequences and 5/6-byte forms.
2. Use it in `name()`; `skip_payload` stays byte-based (matches `skipString`).
3. Writer: add `fn encode_modified_utf8(&str) -> Vec<u8>` and make
   `write_string` length-checked (`u16::MAX` bytes) — return
   `Error::StringTooLong` through `write_named`/`write_nbt` rather than
   truncating. Callers that cannot fail are updated; the public writers already
   return `Result`.

**Tests** New `nbt.rs` tests: round-trip `hello`, `é`, `日本語`, `a\0b`, `🦀`
(supplementary), a lone surrogate rejects, overlong `C0 80` accepted, and a
`level.dat`-shaped fixture whose `LevelName` is an emoji parses. Plus an
`mcre_anvil` integration test that a chunk with an emoji sign loads.

**Risk** Hand-rolled decoder — cover with the cases above; keep the function
private and small.

---

## R3 — One unresolvable block discards the entire chunk (P0)

**Where** `crates/mcre_anvil/src/chunk.rs:171-206` (multi-entry path `:193-197`).

**Problem** `palette_entry_state` returns `None` for an unknown block and
records it, but the multi-entry path turns the in-range `None` into
`Error::PaletteIndex`, which propagates through `decode_section` (`:612`) and
`load_chunk_from_nbt`; the viewer then renders the chunk as air, and
`import_area` aborts the whole rectangle (`:714`). The single-entry path
(`:175-184`) silently airs it, and the biome path in the same function already
degrades gracefully (`:272-274`).

**Fix** Distinguish the two cases:

```rust
let state = *resolved
    .get(*palette_index)
    .ok_or(Error::PaletteIndex(*palette_index))?;   // out of range = corruption
let Some(state) = state else { continue };          // unknown block = skip cell
```

`stats.record_unknown_block` already fires inside `palette_entry_state`.
Add `ImportStats::skipped_blocks` only if the existing counters cannot express
it; otherwise reuse.

**Test** New `chunk.rs` test: a two-entry palette where entry 1 is
`minecraft:not_a_block`, one cell referencing it. Assert
`load_chunk_from_nbt` returns `Ok`, the known block is placed, the unknown cell
is air, and `stats.unknown_blocks` contains the name. Also assert an
out-of-range palette index still errors.

---

## R4 — `set_windowed` runs off the render thread (P0)

**Where** `crates/mcje/src/client.rs:177-190`, caller
`tasks/vanilla_shot/src/main.rs:334`.

**Problem** `Window.setWindowed` → `setMode()` → `RenderSystem.assertOnRenderThread()`
(`Window.java:722-725`, `:483-484`; the assert is unconditional,
`RenderSystem.java:138-141`). The client's render thread is the process main
thread; `drive` runs on the controller thread, so the JNI call throws
`IllegalStateException` whenever `verify_framebuffer` needs to resize — the
normal path on a 2× display, since `Window.getWidth()` is the framebuffer width
(`Window.java:781`).

**Fix** Marshal through `Minecraft.execute` like every other mutation.

1. `ShotDriver.java`: add a `windowed` step that reads `arg` (`"WxH"`) and calls
   `mc.getWindow().setWindowed(w, h)` on the client thread.
2. `ClientControl::set_windowed(w, h)` → `schedule_step("windowed", Some("WxH"))`,
   and have the caller poll `step_error()` (already done via `check_step_error`
   after other steps).

**Test** Manual: `just shot <spec>` on a 2× display reaches the target
framebuffer instead of erroring; `cargo ck` compiles the Java side (build script
recompiles `ShotDriver`).

---

## R5 — Night cloud colour is an approximation (P1)

**Where** `crates/mcre_renderer/src/atmosphere.rs:11-12,128-139`; callers
`examples/native/viewer_native.rs:1059,1848,1876,1892`; export `src/lib.rs:26`.

**Problem** `cloud_color_for(sky_factor)` lerps `NIGHT_CLOUD_COLOR → white` on
the sky-light scalar. Vanilla reads `EnvironmentAttributes.CLOUD_COLOR`
(`LevelExtractor.java:224`), a timeline track
(`timeline/day.json` → `visual/cloud_color`, `multiply`, keyframes
133/11867/13670/22330, `#ffffffff` → `#ff191926`) over the dimension base
`#ccffffff`. At tick 18000 vanilla is `(0.10, 0.10, 0.15)`; the current code
gives `(0.316, 0.316, 0.354)` — ~3.2× too bright with the wrong hue, and the
ramp starts 597 ticks early because it follows the sky-light keyframes.

**Fix**

```rust
pub const CLOUD_COLOR_MULTIPLIER: [(f32, [f32; 3]); 4] = [...];  // ARGB as f32
pub fn cloud_color(time_ticks: f32) -> [f32; 4] { /* base * track, alpha 0.8 */ }
```

Delete `cloud_color_for` and `NIGHT_CLOUD_COLOR` (keep the constant if the
timeline table wants it as a named value). Update the export and the four
viewer call sites to pass `self.time_ticks`. Update `docs/roadmap.md:258-259`.

**Test** `atmosphere.rs` tests: noon → `[1,1,1,0.8]`, 18000 → `[0.098,0.098,0.149,0.8]`
within 1e-3, and the track is continuous across the day wrap.

---

## R6 — Region header with `sector_count == 0` panics (P1)

**Where** `crates/mcre_anvil/src/region.rs:84-106`.

**Problem** `end == start` when `sector_count == 0`; the `end > len` check
passes, then `self.bytes[start..start + 4]` (`:96-101`) is read before any
`start + 5 <= end` check → index out of bounds. `has_chunk` reports the entry as
present, so the viewer walks into it. Vanilla rejects `numSectors == 0` and
sector numbers < 2.

**Fix** Validate up front: `sector_count == 0` or `sectors_offset < 2` →
`Ok(None)` (treat as absent, matching `has_chunk`) or `Error::Truncated`;
prefer `Ok(None)` so a corrupt header does not abort the load. Move the
header checks before the read: require `start + 5 <= end` and `end <= len`.

**Test** Extend the region tests: 8 KiB file with entry `(offset 2, count 0)`,
and an entry whose last sector holds only 3 bytes; assert `chunk_payload`
returns `Ok(None)`/`Err(Truncated)` rather than panicking.

---

## R7 — `grass_block` break particles are tinted (P1)

**Where** `crates/mcre_resource_pack/src/tint.rs:26-45`;
`crates/mcre_renderer/src/mesh.rs:4601-4611`.

**Problem** `particle_tint` uses the layer-0 source with `color`/`colorInWorld`
semantics. Vanilla calls `colorAsTerrainParticle` (`TerrainParticle.java:44`),
and `BlockTintSources.grassBlock()` overrides it to `-1`
(`BlockTintSources.java:80`), so grass-block debris is neutral while mcre tints
it with the biome grass colour. Everything else in the table happens to agree
(`water`/`waterParticles` are equivalent for this call, `sugarCane` falls
through to `colorInWorld`).

**Fix** Add to `tint.rs`:

```rust
/// `BlockTintSource::colorAsTerrainParticle` for layer 0. `None` = vanilla -1.
pub fn particle_tint_source(block_name: &str) -> Option<TintSource> {
    match block_name {
        "grass_block" => None,
        other => tint_sources(other).first().copied(),
    }
}
```

`particle_tint` uses it and returns `[1.0; 3]` for `None`.

**Test** `tint.rs`: `particle_tint_source("grass_block") == None`,
`particle_tint_source("sugar_cane") == Some(SugarCane)`,
`particle_tint_source("stone") == None`.

---

## R8 — Pack filters applied N times; filters never match (P1/P2, merges R16)

See R16; the redundant loop is removed in the same edit.

---

## R9 — `gen_world` reuses a world built with another seed (P1)

**Where** `tasks/gen_world/src/main.rs:125-132`, `:283`,
`write_server_properties` (`level-seed`), `:316-346` (shutdown), `die`.

**Problem** `prepare_out_root` reuses `target/gen_world/world`; vanilla ignores
`level-seed` once `level.dat` exists, so `--seed 123` after any earlier run
generates nothing and prints "generation complete". Separately `stop_server`
ignores `save-all flush`/`stop` failures, treats the 120 s RCON timeout as
success, and accepts a stable directory size 1.5 s apart as "saved"; `die`
`exit(1)`s right after `mark_stopping`, killing the JVM mid-write.

**Fix**

1. Persist the seed: read `Data.WorldGenSettings.seed`/`RandomSeed` from
   `level.dat` (mcre_anvil is already a dependency of `gen_world`? verify) or
   write a `.mcre-seed` marker when the world is created. On mismatch, fail
   with a message telling the user to delete the directory (do not silently
   wipe).
2. `stop_server` returns `Result`; `save-all flush` and `stop` failures are
   reported; on timeout return an error, do not assume success.
3. `die` waits (bounded) for the process to exit before `exit(1)`.

**Test** Manual: second run with a different `--seed` fails fast with a clear
message; same seed reuses. Unit-test the marker comparison helper.

---

## R10 — RCON reply framing uses the wrong constant (P1)

**Where** `tasks/gen_world/src/rcon.rs:11-13`, `:183`, `:200`, `:216-228`.

**Problem** Vanilla splits responses at 4096 **chars**
(`RconClient.java:124-133`) and the packet size field is UTF-8 bytes + 10.
`MAX_PAYLOAD = 1446` makes replies of 1446–4095 bytes look incomplete (600 s
wait then truncated `Ok`), and the `10..=4096` body limit rejects vanilla's
4106-byte full chunk. A 500 ms timeout after a partial read returns `Closed`
and loses the parsed bytes.

**Fix** `MAX_PAYLOAD_CHARS = 4096`; decide "another packet follows" from the
decoded payload's `chars().count()`, not bytes; allow body lengths up to
`MAX_PAYLOAD_CHARS * 4 + 10`. Keep a partial-read buffer in the reader so a
timeout resumes instead of discarding.

**Test** Unit tests with synthetic packets: 2000-byte reply terminates
immediately; two 4096-char chunks concatenate; a 4106-byte body decodes;
partial-read resume across two `read` calls.

---

## R11 — Overworld timeline/dimension attributes are hardcoded (P2, deferred)

**Where** `crates/mcre_renderer/src/atmosphere.rs:5-33`, `lightmap.rs:17-53`,
`clouds.rs:14-24`.

**Problem** Values that ship in `data/minecraft/timeline/day.json` and
`data/minecraft/dimension_type/overworld.json` are transcribed as constants:
fog/sky/cloud colours, fog/sky/cloud/sky-light keyframes, sky fog end distance,
ambient light, cloud height. The client is silently Overworld-only and the
constants drift on a version bump. `data_gen` already reads arbitrary
`data/minecraft/…` JSON (`tasks/data_gen/src/main.rs:249`), so this is an
extension, not new infrastructure.

**Plan (issue, then implement)**

1. `data_gen`: extract `timeline/{day,in_overworld…}.json` and
   `dimension_type/{overworld,the_nether,the_end}.json` into
   `mcre_data` (`timelines.json`, `dimension_types.json`), following the biome
   extraction. Generated accessors in `mcre_static_data_gen`.
2. `mcre_renderer`: add a `DimensionEnv` value type (fog/sky/cloud colours,
   `has_skylight`, cloud height, ambient light) plus a `Timeline` sampler that
   takes keyframes + modifier (`multiply`/`maximum`/`or`/`constant easing`).
3. Replace the constants with lookups; keep the current values as fallback
   constants where the data is absent (packs, tests).
4. `viewer_native`: choose the dimension from `save.spawn.dimension`.

Blocked on `data_gen` regeneration, so it lands as its own PR.

---

## R12 — Sky-less dimensions are imported fully lit (P2)

**Where** `crates/mcre_anvil/src/chunk.rs:657-678`,
`crates/mcre_world/src/world.rs:303-311,361-374`.

**Problem** `import_chunk_light` seeds `sky = 0xFF` and repeats a lit plane for
missing layers. Vanilla omits `SkyLight` when `!hasSkyLight`
(`SerializableChunkData.java:251,265`), so Nether chunks get sky 15 in every
section, and `mark_light_stored()` stops the relight from correcting it.

**Fix (minimum, no data dependency)** Thread `has_skylight: bool` through
`load_chunk*` and the renderer's streaming source. When false, seed missing
sky layers with `0x00` and still mark imported light stored so the world does
not relight a sky-less dimension as if it had a sky. Resolve the boolean from
the built-in dimension id (`the_nether`/`the_end` → false) until R11 supplies
real dimension data. `import_area` remains the block-only data-generation
path. A synthetic sky-less chunk test asserts zero sky light while retaining
block light.

---

## R13 — Entity cubes are hand-transcribed (P2, deferred)

**Where** `crates/mcre_renderer/src/mesh.rs:1302-2447` (18 models),
`lookup.rs:121` (`EntityTextureIds`).

**Problem** ~1,150 lines of cube geometry/UVs copied from Java model classes;
acknowledged in `TODO.md` M13. Vanilla entity models are code, so the only
non-JVM option is transcription. Maintenance cost is the largest single blob in
the PR.

**Plan** Add a `data_gen`-style JVM extractor that walks
`EntityModelSet.bakeLayer` for every `ModelLayerLocation`, emits cube
`{origin, size, uv, mirror, inflate, rotation}` JSON, and generates the Rust
tables. Land after the current milestone; file an issue referencing M13.

---

## R14 — Block tint table hand-mirrored (P2, deferred)

**Where** `crates/mcre_resource_pack/src/tint.rs:26-45`.

**Problem** Mirrors `BlockColors.createDefault()` by block name; the existing
test only checks the names exist, so a renamed block silently loses its tint.
R7 is a symptom of collapsing the vanilla method distinctions.

**Plan** Extend `data_gen` to emit `(block, layer, source-kind, params)` from
the live `BlockColors` registry (constant colours, layer count, and the
`colorAsTerrainParticle` override) and generate the table, replacing the
`mcre_data` name test with a data-equality test.

---

## R15 — `PackVersion::CURRENT` hardcoded (P2, deferred)

**Where** `crates/mcre_resource_pack/src/pack_meta.rs:51-54`; guard at `:333-360`.

**Problem** Correct for snapshot-5 (100.2) but only checked by a
`download`-feature test that needs the jar on disk, so a clean CI checkout never
verifies it.

**Plan** Emit `pack_version` into `mcre_data` from `data_gen` (it already reads
the jar's `version.json` for the version string) and derive `CURRENT` from it.
Meanwhile make the jar test skip gracefully instead of failing when the jar is
absent, and fix `TODO.md:100`.

---

## R16 — Pack filters never match (P2)

**Where** `crates/mcre_resource_pack/src/pack.rs:172-190`,
`crates/mcre_resource_pack/src/blockstates.rs:23-35`,
`src/models.rs:23-35`; tests `pack.rs:264-285`.

**Problem** Vanilla matches `filter.block[].path` against the full resource path
(`MultiPackResourceManager.java:31` → `location.getPath()`), e.g.
`blockstates/stone.json`, and `namespace` against the namespace; the predicate
is an **unanchored** Java regex `find()` (`IdentifierPattern` uses
`Pattern::asPredicate`). mcre passes bare `"stone"` / `"block/stone"`, so
patterns like `.*_wall\.json` can never match; `regex_match` only understands
`.*` and `\.`.

**Fix**

1. Pass vanilla paths: `blockstates/{name}.json`, `models/block/{name}.json`.
2. Replace `regex_match` with a small backtracking matcher with `find`
   semantics supporting literals, `.`, `*`, `+`, `?`, `[...]` (ranges, `^`
   negation), `\.` and other escapes, `^`/`$`. Update the doc comment to state
   the supported subset; unsupported constructs match literally.
3. Drop the redundant `for _prev_pack in &previous_packs` loop (R8) in both
   merge functions — apply each pack's filter once.
4. Textures are still unfiltered; document that in the module docs.

**Test** `pack.rs`: `.*_wall\.json` matches `blockstates/oak_wall.json` but not
`blockstates/oak_fence.json`; namespace filter `minecraft` rejects `mymod`;
integration test where a top pack filters a bottom pack's blockstate and model,
asserting both disappear.

---

## R17 — Pack overlays replace per-pack data (P2)

**Where** `crates/mcre_resource_pack/src/pack.rs:119-134` (culling merge),
`:162-183` (`ensure_extracted` marker at `discover.rs:162-183`),
`pack_meta.rs:131-141` (overlay `formats`).

**Problem** (a) An overlay's `mcre/culling.json` replaces the base pack's whole
map instead of merging; the test at `pack.rs:306-329` encodes the loss.
(b) Zip pack cache keys on file size only, so a same-size edit is never
re-extracted. (c) Overlay `"formats": N` (single int) fails deserialization and
kills `pack.mcmeta`; vanilla accepts an int or `[min, max]`.

**Fix** (a) Merge per key (later overlay entries replace only the same key),
update the test to assert both the base and overlay entries survive. (b) Hash
the zip bytes into the marker because `mcre_fs::Metadata` has no mtime.
(c) Custom `Deserialize` for `formats` accepting int or pair (reuse
`PackVersion`'s visitor shape).

**Test** Extend the existing overlay test; add `pack_meta` cases for
`"formats": 100` and `"formats": [90, 200]`. Same-size zip replacement is
covered by rediscovery/re-extraction using a content fingerprint.

---

## R18 — `data_gen` hardcoded tag fallbacks (P2)

**Where** `tasks/data_gen/src/main.rs:28-39,88-135`.

**Problem** `ICE_TAG_FALLBACK`/`BARS_TAG_FALLBACK` are dead code (the jar is
always present when the tool runs) and mask a tag rename on a version bump;
`fluid_flow_tag_members()` silently returns an empty list, disabling the fluid
flow tag. Failure modes are inconsistent.

**Fix** Delete the fallback tables. Each tag reader returns `Vec<String>` but
panics with a message naming the tag and jar path when resolution fails. One
shared helper for the three tags; keep the recursion and `#`-prefix handling.

**Test** `cargo ck` (tool compiles); manual `cargo run -r -p data_gen` in an
environment with the jar.

---

## R19 — Download caches poisoned by interrupted writes (P2)

**Where** `crates/mcje/build.rs:42-45,67-76,140-145`;
`crates/mcje_downloader/src/lib.rs:339-391`.

**Problem** Existence/size-only checks with direct writes to the final path and
no SHA-1 verification. Ctrl-C or disk-full leaves a truncated `mc.jar`,
library or asset that every later build trusts.

**Fix** Add `write_atomic(path, bytes)` (temp file in the same directory →
`rename`) and use it for `mc.jar`, `libs/*.jar`, the asset index and every
downloaded asset. Verify the manifest SHA-1 before the rename when the hash is
available; on mismatch return an error that names the file. For
`mcje_downloader`, hash cached files against the index entry instead of
comparing length.

**Test** Unit tests for `write_atomic` (existing target is replaced, temp file
cleaned up) and a hash-mismatch case. Manual: interrupt a `cargo ck`, rerun.

---

## R20 — `pregen` swallows worker panics (P2)

**Where** `tasks/gen_world/src/pregen.rs:396-413`.

**Problem** `handle.join().unwrap_or_default()` drops a panicking worker's
range; `tile_full` then has fewer entries than `tiles`, so counts pair with the
wrong tiles and `run_pregen` indexes out of bounds (`:267`), and
`sweep_missing` silently skips the range.

**Fix** Propagate: `.join().unwrap_or_else(|payload| resume_unwind(payload))`,
keeping the scope-based collection.

**Test** Unit test with a `chunked_map` closure that panics; assert the panic
reaches the caller.

---

## R21 — Input hardening (P3)

1. **NBT depth cap**: thread a `depth: usize` through `payload`/`skip_payload`,
   error after 128 nested payload levels. Test with a 129-deep synthetic tag.
2. **Decompression cap**: region payloads and gzip NBT use a shared bounded
   reader (64 MiB maximum), reading at most limit + 1 and returning
   `Error::DecompressedTooLarge` when exceeded.
3. **Chunk position validation**: `import_chunk`, `load_chunk_from_nbt`, and
   `import_area` use the shared `xPos`/`zPos` validator, returning
   `Error::ChunkMismatch`.
4. **`import_area` policy**: keep fail-fast for malformed data and document
   that missing chunks are counted in `skipped_chunks`.

---

## R22 — Docs/API drift (P3)

- `crates/mcre_renderer/TODO.md:100`: "Current format 100.1" → 100.2.
- `docs/roadmap.md:255-259`: cloud tint description follows R5.
- `ResourcePackSet::build(self, _fs)` (`lib.rs:74`): drop the unused parameter
  and update callers.
- `mcre_world/src/chunk.rs:204`: `fill_biome(PLAINS)` early-return leaves
  existing biomes; make it fill unconditionally (or document the default
  optimisation) and add a test.
- `SavePaths` only knows three dimensions (`mcre_anvil/src/lib.rs:68-87`);
  note the limitation next to R11.
- M11–M13 remain accurately marked in progress; the PR description/checklist
  should not describe those milestones as complete.

---

## Verification

Per item: `cargo test -p <crate>` for the touched crate.

Before handing back:

```
cargo fmt
cargo ck
cargo test --workspace --all-features
cargo lint -- -D warnings
typos
```

For renderer items, a `just shot` capture is the manual parity check
(needs the JVM/asset cache warm).
