use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let mc_version_path = manifest_dir.join("../../mc-version");
    println!("cargo:rerun-if-changed={}", mc_version_path.display());

    let version = std::fs::read_to_string(&mc_version_path)
        .unwrap()
        .trim()
        .to_string();

    println!("cargo:rustc-env=MC_VERSION={}", version);
}
