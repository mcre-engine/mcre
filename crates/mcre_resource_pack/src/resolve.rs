use mcre_assets::{BlockModelResolution, ModelVariant};
use mcre_world::BlockState;

use crate::blockstates::BlockstateMap;
use crate::error::Result;
use crate::models::ModelMap;

/// Resolved model references for a single block state.
pub struct ResolvedState {
    /// One entry per model rule. A unified blockstate has a single rule;
    /// a multipart blockstate has one rule per matching part. Each rule is a
    /// weighted variant list from which a single variant is selected at bake
    /// time (mirroring vanilla's weighted random pick, deterministically).
    pub rules: Vec<Vec<(String, ModelVariant)>>,
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
            let rules = match def.resolve(state) {
                Some(BlockModelResolution::Unified(slice)) => {
                    vec![
                        slice
                            .iter()
                            .map(|m| (model_id_to_key(&m.model), m.clone()))
                            .collect(),
                    ]
                }
                Some(BlockModelResolution::Multipart(slices)) => slices
                    .iter()
                    .map(|s| {
                        s.iter()
                            .map(|m| (model_id_to_key(&m.model), m.clone()))
                            .collect()
                    })
                    .collect(),
                None => Vec::new(),
            };
            ResolvedState { rules }
        } else {
            ResolvedState { rules: Vec::new() }
        };

        result.push(entry);
    }

    Ok(result)
}

fn model_id_to_key(id: &mcre_assets::BlockModelId) -> String {
    id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::ResourcePack;
    use crate::pack_meta::{PackMeta, PackSection};

    fn grass_pack() -> ResourcePack {
        let blockstates = [(
            "minecraft:grass_block".to_string(),
            r#"{"variants": {
                "snowy=false": [{"model": "minecraft:block/grass_block"}],
                "snowy=true": {"model": "minecraft:block/grass_block_snow"}
            }}"#
            .to_string(),
        )]
        .into_iter()
        .collect();
        ResourcePack {
            meta: PackMeta {
                pack: PackSection {
                    description: serde_json::Value::Null,
                    pack_format: None,
                    min_format: None,
                    max_format: None,
                },
                filter: None,
                language: None,
                overlays: None,
            },
            path: mcre_fs::path::FsPath::from("test"),
            blockstates,
            models: Default::default(),
            culling: Default::default(),
            colormaps: Default::default(),
            compatible: true,
            order: 0,
        }
    }

    #[test]
    fn snowy_grass_resolves_to_snow_model() {
        let defs = crate::blockstates::merge_blockstate_definitions(&[grass_pack()]);
        let resolved = resolve_all_models(&defs, &Default::default()).unwrap();
        let snowy = &resolved[8];
        assert_eq!(snowy.rules.len(), 1);
        assert!(
            snowy.rules[0]
                .iter()
                .any(|(key, _)| key.contains("grass_block_snow")),
            "snowy=true must use the untinted snow model, got {:?}",
            snowy.rules[0]
                .iter()
                .map(|(key, _)| key)
                .collect::<Vec<_>>(),
        );
        let plain = &resolved[9];
        assert_eq!(plain.rules.len(), 1);
        assert!(
            plain.rules[0]
                .iter()
                .any(|(key, _)| key == "minecraft:block/grass_block"),
        );
    }
}
