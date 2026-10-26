use TintSource::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TintSource {
    Blank,
    Constant([u8; 3]),
    Grass,
    DoubleTallGrass,
    Foliage,
    DryFoliage,
    Water,
    Redstone,
    SugarCane,
    Stem,
}

pub(crate) const fn rgb(color: i32) -> [u8; 3] {
    [(color >> 16) as u8, (color >> 8) as u8, color as u8]
}

const SPRUCE: TintSource = TintSource::Constant(rgb(-10380959));
const BIRCH: TintSource = TintSource::Constant(rgb(-8345771));
const ATTACHED_STEM: TintSource = TintSource::Constant(rgb(-2046180));
const LILY_PAD: TintSource = TintSource::Constant(rgb(-14647248));

pub fn tint_sources(block_name: &str) -> &'static [TintSource] {
    match block_name {
        "large_fern" | "tall_grass" => &[DoubleTallGrass],
        "fern" | "short_grass" | "potted_fern" | "bush" => &[Grass],
        "grass_block" => &[Grass],
        "pink_petals" | "wildflowers" => &[Blank, Grass],
        "spruce_leaves" => &[SPRUCE],
        "birch_leaves" => &[BIRCH],
        "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves" | "vine"
        | "mangrove_leaves" => &[Foliage],
        "leaf_litter" => &[DryFoliage],
        "water_cauldron" | "water" | "bubble_column" => &[Water],
        "redstone_wire" => &[Redstone],
        "sugar_cane" => &[SugarCane],
        "attached_melon_stem" | "attached_pumpkin_stem" => &[ATTACHED_STEM],
        "melon_stem" | "pumpkin_stem" => &[Stem],
        "lily_pad" => &[LILY_PAD],
        _ => &[],
    }
}

/// Vanilla `BlockTintSource::colorAsTerrainParticle` for the layer-0 source,
/// used by break particles. `None` is vanilla's `-1` (no tint);
/// `BlockTintSources.grassBlock` overrides the default method that way.
pub fn particle_tint_source(block_name: &str) -> Option<TintSource> {
    match block_name {
        "grass_block" => None,
        other => tint_sources(other).first().copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_table_blocks_exist() {
        let blocks = mcre_data::block::Block::all_sync().unwrap();
        let names: std::collections::HashSet<&str> =
            blocks.iter().map(|block| block.name.as_str()).collect();
        let table = [
            "large_fern",
            "tall_grass",
            "fern",
            "short_grass",
            "potted_fern",
            "bush",
            "grass_block",
            "pink_petals",
            "wildflowers",
            "spruce_leaves",
            "birch_leaves",
            "oak_leaves",
            "jungle_leaves",
            "acacia_leaves",
            "dark_oak_leaves",
            "vine",
            "mangrove_leaves",
            "leaf_litter",
            "water_cauldron",
            "water",
            "bubble_column",
            "redstone_wire",
            "sugar_cane",
            "attached_melon_stem",
            "attached_pumpkin_stem",
            "melon_stem",
            "pumpkin_stem",
            "lily_pad",
        ];
        for name in table {
            assert!(names.contains(name), "tint table block missing: {name}");
            assert!(
                !tint_sources(name).is_empty(),
                "tint table entry empty: {name}"
            );
        }
    }

    #[test]
    fn constant_colors_match_vanilla_decimals() {
        assert_eq!(rgb(-10380959), [0x61, 0x99, 0x61]);
        assert_eq!(rgb(-8345771), [0x80, 0xA7, 0x55]);
        assert_eq!(rgb(-2046180), [0xE0, 0xC7, 0x1C]);
        assert_eq!(rgb(-14647248), [0x20, 0x80, 0x30]);
    }

    #[test]
    fn particle_tint_matches_vanilla_terrain_particle_colors() {
        assert_eq!(particle_tint_source("grass_block"), None);
        assert_eq!(
            particle_tint_source("sugar_cane"),
            Some(TintSource::SugarCane)
        );
        assert_eq!(particle_tint_source("stone"), None);
        assert_eq!(
            particle_tint_source("oak_leaves"),
            Some(TintSource::Foliage)
        );
        assert_eq!(
            particle_tint_source("redstone_wire"),
            Some(TintSource::Redstone)
        );
    }
}
