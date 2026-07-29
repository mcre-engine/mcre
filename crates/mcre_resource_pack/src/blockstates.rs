use hashbrown::HashMap;
use mcre_assets::BlockStateDefinition;
use rustc_hash::FxBuildHasher;

use crate::pack::ResourcePack;

pub(crate) type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

/// Map from block name ("minecraft:stone") to parsed BlockStateDefinition.
pub type BlockstateMap = FxHashMap<String, BlockStateDefinition>;

/// Merges blockstate definitions from all resource packs.
///
/// Packs are processed in order. Later packs override earlier ones for the
/// same block name. Filter patterns in a pack remove matching entries that
/// were added by earlier packs.
pub fn merge_blockstate_definitions(packs: &[ResourcePack]) -> BlockstateMap {
    let mut result: BlockstateMap = HashMap::with_capacity_and_hasher(1024, FxBuildHasher);
    let mut previous_packs: Vec<&ResourcePack> = Vec::new();

    for pack in packs {
        // Apply filter: remove entries from earlier packs that match this pack's filter
        for _prev_pack in &previous_packs {
            let to_remove: Vec<String> = result
                .keys()
                .filter(|key| {
                    let (ns, path) = split_key(key);
                    // Check if current pack's filter removes this entry from a previous pack
                    pack.matches_filter(ns, path)
                })
                .cloned()
                .collect();
            for key in to_remove {
                result.remove(&key);
            }
        }

        // Parse and insert this pack's blockstate entries
        for (key, raw_json) in &pack.blockstates {
            match serde_json::from_str::<BlockStateDefinition>(raw_json) {
                Ok(def) => {
                    result.insert(key.clone(), def);
                }
                Err(err) => {
                    // Log and skip malformed entries
                    eprintln!("Warning: failed to parse blockstate '{key}': {err}");
                }
            }
        }

        previous_packs.push(pack);
    }

    result
}

/// Splits a key like "minecraft:stone" into ("minecraft", "stone").
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
        let (ns, path) = split_key("minecraft:stone");
        assert_eq!(ns, "minecraft");
        assert_eq!(path, "stone");
    }

    #[test]
    fn test_split_key_without_namespace() {
        let (ns, path) = split_key("stone");
        assert_eq!(ns, "minecraft");
        assert_eq!(path, "stone");
    }
}
