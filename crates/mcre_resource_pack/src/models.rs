use hashbrown::HashMap;
use mcre_assets::BlockModelDefinition;
use rustc_hash::FxBuildHasher;

use crate::pack::ResourcePack;

pub(crate) type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

/// Map from model path ("minecraft:block/stone") to parsed BlockModelDefinition.
pub type ModelMap = FxHashMap<String, BlockModelDefinition>;

/// Merges block model definitions from all resource packs.
///
/// Packs are processed in order. Later packs override earlier ones for the
/// same model path. Filter patterns in a pack remove matching entries that
/// were added by earlier packs.
pub fn merge_block_model_definitions(packs: &[ResourcePack]) -> ModelMap {
    let mut result: ModelMap = HashMap::with_capacity_and_hasher(4096, FxBuildHasher);
    let mut previous_packs: Vec<&ResourcePack> = Vec::new();

    for pack in packs {
        // Apply filter: remove entries from earlier packs matching this pack's filter
        for _prev_pack in &previous_packs {
            let to_remove: Vec<String> = result
                .keys()
                .filter(|key| {
                    let (ns, path) = split_key(key);
                    pack.matches_filter(ns, path)
                })
                .cloned()
                .collect();
            for key in to_remove {
                result.remove(&key);
            }
        }

        // Parse and insert this pack's model entries
        for (key, raw_json) in &pack.models {
            match serde_json::from_str::<BlockModelDefinition>(raw_json) {
                Ok(def) => {
                    result.insert(key.clone(), def);
                }
                Err(err) => {
                    eprintln!("Warning: failed to parse model '{key}': {err}");
                }
            }
        }

        previous_packs.push(pack);
    }

    result
}

/// Splits a key like "minecraft:block/stone" into ("minecraft", "block/stone").
fn split_key(key: &str) -> (&str, &str) {
    if let Some(pos) = key.find(':') {
        (&key[..pos], &key[pos + 1..])
    } else {
        ("minecraft", key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_key_with_namespace() {
        let (ns, path) = split_key("minecraft:block/stone");
        assert_eq!(ns, "minecraft");
        assert_eq!(path, "block/stone");
    }

    #[test]
    fn test_split_key_without_namespace() {
        let (ns, path) = split_key("block/stone");
        assert_eq!(ns, "minecraft");
        assert_eq!(path, "block/stone");
    }
}
