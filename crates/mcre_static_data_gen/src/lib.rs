mod analyzer;
mod generators;

use std::path::Path;

use indexmap::IndexMap;
use mcre_data::{block::Block, state::BlockState};

use crate::{analyzer::analyze, generators::Factory};

pub fn generate(output_dir: &Path, blocks: &[Block], states: &[BlockState]) {
    let mut foreign_enums: IndexMap<&str, Box<[&str]>> = IndexMap::new();

    foreign_enums.insert(
        "Direction",
        Box::new(["down", "up", "north", "south", "west", "east"]),
    );
    foreign_enums.insert("Axis", Box::new(["x", "y", "z"]));

    let analysis = analyze(blocks, foreign_enums);

    let mut factory = Factory::new(output_dir.to_path_buf());

    factory.add_scope(generators::RootScope { blocks, states });

    factory.generate_flat(&analysis);
}
