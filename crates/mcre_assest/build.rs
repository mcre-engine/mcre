use std::{
    env,
    fs::File,
    io::{self, Cursor},
    path::PathBuf,
};

use mcje_downloader::RootManifest;
use tokio::fs;
use zip::ZipArchive;

#[tokio::main]
async fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_dir = PathBuf::from(manifest_dir);
    if manifest_dir.join("assets").exists() {
        return;
    }
    let root_manifest = RootManifest::fetch().await.unwrap();
    let version_release = root_manifest
        .versions
        .into_iter()
        .find(|ver| ver.id == "1.21.10")
        .unwrap();

    let version_manifest = version_release.fetch_manifest().await.unwrap();

    let jar = version_manifest.downloads.client.download().await.unwrap();
    let jar_cursor = Cursor::new(jar);

    let mut jar_archive = ZipArchive::new(jar_cursor).unwrap();

    for i in 0..jar_archive.len() {
        let mut entry = jar_archive.by_index(i).unwrap();
        let name = entry.name();

        if name.starts_with("assets/minecraft") {
            let outpath = manifest_dir.join(name);
            if let Some(parent) = outpath.parent()
                && !parent.exists()
            {
                fs::create_dir_all(parent).await.unwrap();
            }

            let mut file = File::create(outpath).unwrap();
            io::copy(&mut entry, &mut file).unwrap();
        }
    }
}
