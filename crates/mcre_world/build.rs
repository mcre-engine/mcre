use std::path::PathBuf;

fn main() {
    let blocks = mcre_data::block::Block::all_sync().unwrap();
    let states = mcre_data::state::BlockState::all_sync().unwrap();

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("mcre_world_gen");
    std::fs::create_dir_all(&out_dir).unwrap();

    mcre_static_data_gen::generate(&out_dir, &blocks, &states);

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mcre_data_dir = manifest_dir.join("../../crates/mcre_data");
    println!(
        "cargo:rerun-if-changed={}",
        mcre_data_dir.join("blocks.json").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        mcre_data_dir.join("block_states.json").display()
    );
}
