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

/// Single/left/right sprite IDs for one chest material
/// (`MultiblockChestResources`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChestTextures {
    pub single: u32,
    pub left: u32,
    pub right: u32,
}

impl ChestTextures {
    fn intern(interner: &mut StringInterner, prefix: &str) -> Self {
        Self {
            single: interner.intern(&format!("minecraft:entity/chest/{prefix}")),
            left: interner.intern(&format!("minecraft:entity/chest/{prefix}_left")),
            right: interner.intern(&format!("minecraft:entity/chest/{prefix}_right")),
        }
    }
}

/// Texture IDs backing the built-in font sheets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontTextureIds {
    pub ascii: u32,
}

impl FontTextureIds {
    fn intern(interner: &mut StringInterner) -> Self {
        Self {
            ascii: interner.intern("minecraft:font/ascii"),
        }
    }
}

/// Texture IDs backing the built-in entity models.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityTextureIds {
    pub pig_temperate: u32,
    pub pig_warm: u32,
    pub cow_temperate: u32,
    pub chicken_temperate: u32,
    pub chicken_warm: u32,
    pub creeper: u32,
    pub sheep: u32,
    pub sheep_wool: u32,
    pub zombie: u32,
    pub skeleton: u32,
}

impl EntityTextureIds {
    fn intern(interner: &mut StringInterner) -> Self {
        Self {
            pig_temperate: interner.intern("minecraft:entity/pig/pig_temperate"),
            pig_warm: interner.intern("minecraft:entity/pig/pig_warm"),
            cow_temperate: interner.intern("minecraft:entity/cow/cow_temperate"),
            chicken_temperate: interner.intern("minecraft:entity/chicken/chicken_temperate"),
            chicken_warm: interner.intern("minecraft:entity/chicken/chicken_warm"),
            creeper: interner.intern("minecraft:entity/creeper/creeper"),
            sheep: interner.intern("minecraft:entity/sheep/sheep"),
            sheep_wool: interner.intern("minecraft:entity/sheep/sheep_wool"),
            zombie: interner.intern("minecraft:entity/zombie/zombie"),
            skeleton: interner.intern("minecraft:entity/skeleton/skeleton"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChestTextureIds {
    pub normal: ChestTextures,
    pub trapped: ChestTextures,
    pub ender: u32,
    pub copper: ChestTextures,
    pub copper_exposed: ChestTextures,
    pub copper_weathered: ChestTextures,
    pub copper_oxidized: ChestTextures,
}

impl ChestTextureIds {
    fn intern(interner: &mut StringInterner) -> Self {
        Self {
            normal: ChestTextures::intern(interner, "normal"),
            trapped: ChestTextures::intern(interner, "trapped"),
            ender: interner.intern("minecraft:entity/chest/ender"),
            copper: ChestTextures::intern(interner, "copper"),
            copper_exposed: ChestTextures::intern(interner, "copper_exposed"),
            copper_weathered: ChestTextures::intern(interner, "copper_weathered"),
            copper_oxidized: ChestTextures::intern(interner, "copper_oxidized"),
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
    /// Particle texture IDs indexed by `BlockState(u16).0`.
    pub particle_texture_ids: Vec<Option<u32>>,
    /// Interned texture path strings, indexed by `texture_id` in `BakedQuadEntry`.
    pub texture_table: Vec<Box<str>>,
    /// Texture IDs for the built-in fluid models; always present in the table.
    pub fluid_textures: FluidTextureIds,
    pub chest_textures: ChestTextureIds,
    pub font_textures: FontTextureIds,
    pub entity_textures: EntityTextureIds,
    /// Face-culling overrides keyed by block name (`"oak_leaves"`), merged
    /// across packs with last-pack-wins per block.
    pub culling: HashMap<String, CullingOverride>,
    /// Biome colormaps (256x256 RGB) resolved through the pack stack.
    pub colormaps: Colormaps,
}

impl ResourcePackData {
    pub fn new(
        entries: Vec<BlockStateEntry>,
        particle_texture_ids: Vec<Option<u32>>,
        mut interner: StringInterner,
        culling: HashMap<String, CullingOverride>,
        colormaps: Colormaps,
    ) -> Self {
        let fluid_textures = FluidTextureIds::intern(&mut interner);
        let chest_textures = ChestTextureIds::intern(&mut interner);
        let font_textures = FontTextureIds::intern(&mut interner);
        let entity_textures = EntityTextureIds::intern(&mut interner);
        Self {
            entries,
            particle_texture_ids,
            texture_table: interner.into_strings(),
            fluid_textures,
            chest_textures,
            font_textures,
            entity_textures,
            culling,
            colormaps,
        }
    }
}

impl ResourcePackData {
    pub fn particle_texture_id(&self, state_id: u16) -> Option<u32> {
        self.particle_texture_ids
            .get(usize::from(state_id))
            .copied()
            .flatten()
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
) -> Result<(Vec<BlockStateEntry>, Vec<Option<u32>>)> {
    let mut entries = Vec::with_capacity(resolved.len());
    let mut particle_texture_ids = Vec::with_capacity(resolved.len());
    let mut particle_texture_paths = HashMap::new();

    for state_resolved in resolved {
        let mut rules = Vec::with_capacity(state_resolved.rules.len());
        let mut particle_texture_id = None;

        for rule in &state_resolved.rules {
            let mut variants = Vec::with_capacity(rule.len());

            for (model_key, variant) in rule {
                let Some(model) = model_defs.get(model_key.as_str()) else {
                    continue;
                };

                let particle_texture = particle_texture_paths
                    .entry(model_key.clone())
                    .or_insert_with(|| {
                        model
                            .build_texture_map(|id: &mcre_assets::BlockModelId| {
                                model_defs.get(&id.to_string()).cloned()
                            })
                            .ok()
                            .and_then(|textures| {
                                textures
                                    .get(&mcre_assets::ReferenceId::new("particle".to_string()))
                                    .map(ToString::to_string)
                            })
                    })
                    .clone();

                let mut baked = match model.bake_quads(|id: &mcre_assets::BlockModelId| {
                    model_defs.get(&id.to_string()).cloned()
                }) {
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

                if particle_texture_id.is_none() {
                    particle_texture_id = particle_texture
                        .as_deref()
                        .and_then(|texture| interner.get(texture))
                        .or_else(|| quads.first().map(|quad| quad.texture_id));
                }

                variants.push(BlockVariant {
                    weight: variant.weight,
                    quads,
                });
            }

            rules.push(BlockRule { variants });
        }

        entries.push(BlockStateEntry { rules });
        particle_texture_ids.push(particle_texture_id);
    }

    Ok((entries, particle_texture_ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ModelMap;
    use mcre_assets::BlockModelDefinition;

    #[test]
    fn particle_texture_uses_inherited_parent_reference() {
        let parent: BlockModelDefinition = serde_json::from_str(
            r##"{"textures":{"all":"minecraft:block/dirt"},"particle":"#all"}"##,
        )
        .unwrap();
        let child: BlockModelDefinition = serde_json::from_str(
            r##"{"parent":"minecraft:block/parent","textures":{"all":"minecraft:block/stone"}}"##,
        )
        .unwrap();
        let variant = serde_json::from_str(r#"{"model":"minecraft:block/child"}"#).unwrap();
        let resolved = [ResolvedState {
            rules: vec![vec![("minecraft:block/child".to_string(), variant)]],
        }];
        let model_defs: ModelMap = [
            ("minecraft:block/parent".to_string(), parent),
            ("minecraft:block/child".to_string(), child),
        ]
        .into_iter()
        .collect();
        let mut interner = StringInterner::new();
        interner.intern("minecraft:block/stone");

        let (_, particle_texture_ids) =
            build_entries(&resolved, &model_defs, &mut interner).unwrap();

        let texture_id = particle_texture_ids[0].unwrap();
        assert_eq!(interner.resolve(texture_id), Some("minecraft:block/stone"));
    }
}
