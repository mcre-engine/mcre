use mcre_assets::{BlockModelResolution, ModelVariant};
use mcre_world::BlockState;

use crate::blockstates::BlockstateMap;
use crate::error::Result;
use crate::models::ModelMap;

/// Resolved model references for a single block state.
pub struct ResolvedState {
    /// Model path keys (e.g., "minecraft:block/stone") with their ModelVariant metadata.
    pub variants: Vec<(String, ModelVariant)>,
}

/// Resolve all block states to their matching model variants.
///
/// For each `BlockState` (0..MAX), looks up the block name in the merged
/// blockstate definitions, calls `BlockStateDefinition::resolve()` to get
/// the applicable model variants, and collects them.
pub fn resolve_all_models(
    blockstate_defs: &BlockstateMap,
    _model_defs: &ModelMap,
) -> Result<Vec<ResolvedState>> {
    let mut result = Vec::new();

    for state in BlockState::all() {
        let block_name = state.block().name();
        let key = format!("minecraft:{block_name}");

        let entry = if let Some(def) = blockstate_defs.get(&key) {
            let variants = match def.resolve(state) {
                Some(BlockModelResolution::Unified(slice)) => slice
                    .iter()
                    .map(|m| (model_id_to_key(&m.model), m.clone()))
                    .collect(),
                Some(BlockModelResolution::Multipart(slices)) => slices
                    .iter()
                    .flat_map(|s| s.iter())
                    .map(|m| (model_id_to_key(&m.model), m.clone()))
                    .collect(),
                None => Vec::new(),
            };
            ResolvedState { variants }
        } else {
            ResolvedState {
                variants: Vec::new(),
            }
        };

        result.push(entry);
    }

    Ok(result)
}

fn model_id_to_key(id: &mcre_assets::BlockModelId) -> String {
    id.to_string()
}
