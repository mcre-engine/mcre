use std::collections::HashMap;
use std::str::FromStr;

use mcre_world::{Biome, Block, BlockState, PropKey, PropVal};

fn unprefix(name: &str) -> &str {
    match name.split_once(':') {
        Some((_, path)) => path,
        None => name,
    }
}

#[derive(Clone)]
pub struct BlockResolver {
    by_name: HashMap<String, Block>,
}

impl BlockResolver {
    pub fn new() -> Self {
        let mut by_name = HashMap::new();
        for id in 0..=u16::from(Block::MAX) {
            let block = Block::from(id);
            by_name.entry(block.name().to_string()).or_insert(block);
        }
        Self { by_name }
    }

    pub fn resolve(&self, name: &str, properties: &[(String, String)]) -> Option<BlockState> {
        let block = self.by_name.get(unprefix(name)).copied()?;
        if properties.is_empty() {
            return Some(block.default_state());
        }
        let mut values = Vec::with_capacity(properties.len());
        for (key, value) in properties {
            let prop_key = PropKey::from_str(key).ok()?;
            values.push(PropVal::parse_with_key(prop_key, value)?);
        }
        block.state(&values)
    }
}

impl Default for BlockResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct BiomeResolver {
    by_name: HashMap<String, Biome>,
}

impl BiomeResolver {
    pub fn new() -> Self {
        let mut by_name = HashMap::new();
        for biome in Biome::all() {
            by_name.entry(biome.name().to_string()).or_insert(biome);
        }
        Self { by_name }
    }

    pub fn resolve(&self, name: &str) -> Option<Biome> {
        self.by_name.get(unprefix(name)).copied()
    }
}

impl Default for BiomeResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_defaults_by_name() {
        let resolver = BlockResolver::new();
        let stone = resolver.resolve("minecraft:stone", &[]);
        assert!(stone.is_some());
        assert_eq!(stone, resolver.resolve("stone", &[]));
        assert!(resolver.resolve("minecraft:not_a_block", &[]).is_none());
    }

    #[test]
    fn resolves_states_with_properties() {
        let resolver = BlockResolver::new();
        let props = vec![("snowy".to_string(), "true".to_string())];
        let state = resolver.resolve("minecraft:grass_block", &props);
        assert!(state.is_some());
        assert_ne!(state, resolver.resolve("minecraft:grass_block", &[]),);
        assert!(
            resolver
                .resolve(
                    "minecraft:grass_block",
                    &[("snowy".to_string(), "maybe".to_string())],
                )
                .is_none()
        );
        assert!(
            resolver
                .resolve(
                    "minecraft:grass_block",
                    &[("bogus_prop".to_string(), "true".to_string())],
                )
                .is_none()
        );
    }

    #[test]
    fn resolves_biomes() {
        let resolver = BiomeResolver::new();
        assert!(resolver.resolve("minecraft:plains").is_some());
        assert_eq!(
            resolver.resolve("minecraft:plains"),
            resolver.resolve("plains"),
        );
        assert!(resolver.resolve("minecraft:not_a_biome").is_none());
    }
}
