use mcre_assets::BlockModelDefinition;
use mcre_core::{Direction, Vec3f};
use serde::Deserialize;
use std::collections::HashMap;

use crate::colormap::Colormaps;
use crate::error::Result;
use crate::interner::StringInterner;
use crate::models::ModelMap;
use crate::resolve::ResolvedState;

/// A baked quad with an interned texture ID (u32) instead of a string.
pub struct BakedQuadEntry {
    pub vertices: [Vec3f; 4],
    /// Per-vertex UVs in model space (0..16).
    pub uvs: [[f32; 2]; 4],
    /// Index into `ResourcePackData::texture_table`.
    pub texture_id: u32,
    pub cullface: Option<Direction>,
    pub direction: Direction,
    pub tintindex: Option<u8>,
    /// Baked vanilla `CardinalLighting` factor (`shade_direction_override`
    /// direction if set, else the quad's facing).
    pub shade: f32,
    pub light_emission: u8,
    /// Model `ambientocclusion` flag; quads with it set are lit flat when the
    /// block also emits light (vanilla `ModelBlockRenderer` dispatch).
    pub ambient_occlusion: bool,
}

/// Per-block-state rendering data: weighted model variants per rule.
///
/// A unified blockstate has a single rule; a multipart blockstate has one
/// rule per matching part. Vanilla selects one variant per rule at mesh time
/// with a per-position seeded RNG (`ModelBlockRenderer.tesselateBlock`), so
/// all variants are baked and the pick happens in the mesher.
pub struct BlockStateEntry {
    pub rules: Vec<BlockRule>,
}

/// One weighted variant list (a vanilla `WeightedVariants` model, or a lone
/// `SingleVariant`).
pub struct BlockRule {
    pub variants: Vec<BlockVariant>,
}

/// A single baked model variant with its blockstate weight.
pub struct BlockVariant {
    pub weight: u8,
    pub quads: Vec<BakedQuadEntry>,
}

/// Per-block face-culling overrides from a pack's `mcre/culling.json`.
/// Each side is optional: `None` keeps the generated data, `Some` replaces it
/// (including with an empty list).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CullingOverride {
    #[serde(default)]
    pub skip_against_tags: Option<Vec<String>>,
    #[serde(default)]
    pub culling_tags: Option<Vec<String>>,
}

/// Texture IDs backing the built-in fluid models (vanilla
/// `FluidStateModelSet`: water still/flow/overlay, lava still/flow).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FluidTextureIds {
    pub water_still: u32,
    pub water_flow: u32,
    pub water_overlay: u32,
    pub lava_still: u32,
    pub lava_flow: u32,
}

impl FluidTextureIds {
    fn intern(interner: &mut StringInterner) -> Self {
        Self {
            water_still: interner.intern("minecraft:block/water_still"),
            water_flow: interner.intern("minecraft:block/water_flow"),
            water_overlay: interner.intern("minecraft:block/water_overlay"),
            lava_still: interner.intern("minecraft:block/lava_still"),
            lava_flow: interner.intern("minecraft:block/lava_flow"),
        }
    }
}

/// Final lookup table mapping block state IDs to baked rendering data.
///
/// All string IDs (texture paths, model paths) are replaced with compact
/// `u32` integer indices into `texture_table`.
pub struct ResourcePackData {
    /// Indexed by `BlockState(u16).0`, length = BlockState::MAX + 1.
    pub entries: Vec<BlockStateEntry>,
    /// Interned texture path strings, indexed by `texture_id` in `BakedQuadEntry`.
    pub texture_table: Vec<Box<str>>,
    /// Texture IDs for the built-in fluid models; always present in the table.
    pub fluid_textures: FluidTextureIds,
    /// Face-culling overrides keyed by block name (`"oak_leaves"`), merged
    /// across packs with last-pack-wins per block.
    pub culling: HashMap<String, CullingOverride>,
    /// Biome colormaps (256x256 RGB) resolved through the pack stack.
    pub colormaps: Colormaps,
}

impl ResourcePackData {
    pub fn new(
        entries: Vec<BlockStateEntry>,
        mut interner: StringInterner,
        culling: HashMap<String, CullingOverride>,
        colormaps: Colormaps,
    ) -> Self {
        let fluid_textures = FluidTextureIds::intern(&mut interner);
        Self {
            entries,
            texture_table: interner.into_strings(),
            fluid_textures,
            culling,
            colormaps,
        }
    }
}

/// Bake all resolved model variants into the final lookup table.
///
/// For each block state, each resolved rule bakes every variant (model quads
/// with the variant's rotation and uvlock transforms applied), interns all
/// texture path strings, and collects the result into a `BlockStateEntry`.
/// Variant selection is left to mesh time, where vanilla picks per block
/// position with a seeded RNG.
pub fn build_entries(
    resolved: &[ResolvedState],
    model_defs: &ModelMap,
    interner: &mut StringInterner,
) -> Result<Vec<BlockStateEntry>> {
    let mut entries = Vec::with_capacity(resolved.len());

    for state_resolved in resolved {
        let mut rules = Vec::with_capacity(state_resolved.rules.len());

        for rule in &state_resolved.rules {
            let mut variants = Vec::with_capacity(rule.len());

            for (model_key, variant) in rule {
                let Some(model) = model_defs.get(model_key.as_str()) else {
                    continue;
                };

                let parent_resolver =
                    |id: &mcre_assets::BlockModelId| -> Option<BlockModelDefinition> {
                        model_defs.get(&id.to_string()).cloned()
                    };

                let mut baked = match model.bake_quads(parent_resolver) {
                    Ok(q) => q,
                    Err(_) => continue,
                };

                variant.transform_quads(&mut baked);

                let mut quads = Vec::with_capacity(baked.len());
                for quad in baked {
                    let texture_str = quad.texture.to_string();
                    let texture_id = interner.intern(&texture_str);

                    quads.push(BakedQuadEntry {
                        vertices: quad.vertices,
                        uvs: quad.uvs,
                        texture_id,
                        cullface: quad.cullface,
                        direction: quad.direction,
                        tintindex: quad.tintindex,
                        shade: quad.shade_factor(),
                        light_emission: quad.light_emission,
                        ambient_occlusion: model.ambientocclusion,
                    });
                }

                variants.push(BlockVariant {
                    weight: variant.weight,
                    quads,
                });
            }

            rules.push(BlockRule { variants });
        }

        entries.push(BlockStateEntry { rules });
    }

    Ok(entries)
}
