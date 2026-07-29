use mcre_assets::BlockModelDefinition;
use mcre_core::{Direction, Vec3f};

use crate::error::Result;
use crate::interner::StringInterner;
use crate::models::ModelMap;
use crate::resolve::ResolvedState;

/// A baked quad with an interned texture ID (u32) instead of a string.
pub struct BakedQuadEntry {
    pub vertices: [Vec3f; 4],
    pub uv: [f32; 4],
    /// Index into `ResourcePackData::texture_table`.
    pub texture_id: u32,
    pub cullface: Option<Direction>,
    pub tintindex: Option<u8>,
    pub shade: bool,
    pub light_emission: u8,
}

/// Per-block-state rendering data: all baked quads for this state.
pub struct BlockStateEntry {
    pub quads: Vec<BakedQuadEntry>,
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
}

impl ResourcePackData {
    pub fn new(entries: Vec<BlockStateEntry>, interner: StringInterner) -> Self {
        Self {
            entries,
            texture_table: interner.into_strings(),
        }
    }
}

/// Bake all resolved model variants into the final lookup table.
///
/// For each block state, this loads each matching model definition,
/// bakes it into quads, interns all texture path strings,
/// and collects them into a `BlockStateEntry`.
pub fn build_entries(
    resolved: &[ResolvedState],
    model_defs: &ModelMap,
    interner: &mut StringInterner,
) -> Result<Vec<BlockStateEntry>> {
    let mut entries = Vec::with_capacity(resolved.len());

    for state_resolved in resolved {
        let mut quads = Vec::new();

        for (model_key, _variant) in &state_resolved.variants {
            let Some(model) = model_defs.get(model_key.as_str()) else {
                continue;
            };

            let parent_resolver = |id: &mcre_assets::BlockModelId| -> Option<BlockModelDefinition> {
                model_defs.get(&id.to_string()).cloned()
            };

            let baked = match model.bake_quads(parent_resolver) {
                Ok(q) => q,
                Err(_) => continue,
            };

            for quad in baked {
                let texture_str = quad.texture.to_string();
                let texture_id = interner.intern(&texture_str);

                quads.push(BakedQuadEntry {
                    vertices: quad.vertices,
                    uv: *quad.uv,
                    texture_id,
                    cullface: quad.cullface,
                    tintindex: quad.tintindex,
                    shade: quad.shade,
                    light_emission: quad.light_emission,
                });
            }
        }

        entries.push(BlockStateEntry { quads });
    }

    Ok(entries)
}
