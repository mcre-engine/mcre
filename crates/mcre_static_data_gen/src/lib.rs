mod analyzer;
mod generators;

use std::path::Path;

use indexmap::IndexMap;
use mcre_data::{biome::Biome, block::Block, shape::OcclusionShape, state::BlockState};

use crate::{analyzer::analyze, generators::Factory};

pub fn generate(
    output_dir: &Path,
    blocks: &[Block],
    states: &[BlockState],
    biomes: &[Biome],
    shapes: &[OcclusionShape],
) {
    let mut foreign_enums: IndexMap<&str, Box<[&str]>> = IndexMap::new();

    foreign_enums.insert(
        "Direction",
        Box::new(["down", "up", "north", "south", "west", "east"]),
    );
    foreign_enums.insert("Axis", Box::new(["x", "y", "z"]));

    let analysis = analyze(blocks, foreign_enums);

    let mut factory = Factory::new(output_dir.to_path_buf());

    factory.add_scope(generators::RootScope {
        blocks,
        states,
        biomes,
        shapes,
    });

    factory.generate_flat(&analysis);
}
