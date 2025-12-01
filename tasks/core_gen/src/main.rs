mod analyzer;
mod generators;

use std::path::PathBuf;

use indexmap::IndexMap;
use mcre_data::{block::Block, state::BlockState};

use crate::{
    analyzer::analyze,
    generators::{Factory, RootScope},
};

#[tokio::main]
async fn main() {
    let blocks = Block::all().await.unwrap();
    let block_states = BlockState::all().await.unwrap();

    let mut foreign_enums: IndexMap<&str, Box<[&str]>> = IndexMap::new();

    foreign_enums.insert(
        "Direction",
        Box::new(["down", "up", "north", "south", "west", "east"]),
    );
    foreign_enums.insert("Axis", Box::new(["x", "y", "z"]));

    let analysis = analyze(&blocks, foreign_enums);

    let mut factory = Factory::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/mcre_core/src/data"),
    );

    factory.add_scope(RootScope {
        blocks: &blocks,
        states: &block_states,
    });

    factory.generate(&analysis).await;
}
