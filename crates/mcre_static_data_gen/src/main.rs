use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let blocks = mcre_data::block::Block::all().await.unwrap();
    let states = mcre_data::state::BlockState::all().await.unwrap();
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/mcre_world/src/data");
    mcre_static_data_gen::generate(&output, &blocks, &states);
}
