use std::io::{Cursor, Read};

use mcje_downloader::RootManifest;
use mcre_fs::{Fs, FsPath};
use zip::ZipArchive;

use crate::error::{ResourcePackError, Result};

fn embedded_mc_version() -> &'static str {
    env!("MC_VERSION").trim()
}

/// Ensure the default Minecraft resource pack is available in the filesystem.
///
/// Checks the cache directory first. If the pack isn't there, downloads the
/// Minecraft client jar and extracts `assets/minecraft/` into the cache.
///
/// Requires the `download` feature.
pub async fn ensure_default_pack(fs: &impl Fs) -> Result<FsPath> {
    let version = embedded_mc_version();
    let cache_root = FsPath::from(format!("minecraft/resource_packs/{version}/default"));

    let meta_path = cache_root.join("pack.mcmeta");
    if fs.exists(&meta_path).await {
        return Ok(cache_root);
    }

    let root_manifest = RootManifest::fetch()
        .await
        .map_err(|e| ResourcePackError::Download(e.to_string()))?;

    let version_release = root_manifest
        .versions
        .into_iter()
        .find(|v| v.id == version)
        .ok_or_else(|| ResourcePackError::Download(format!("Version {version} not found")))?;

    let version_manifest = version_release
        .fetch_manifest()
        .await
        .map_err(|e| ResourcePackError::Download(e.to_string()))?;

    let jar_bytes = version_manifest
        .downloads
        .client
        .download()
        .await
        .map_err(|e| ResourcePackError::Download(e.to_string()))?;

    let cursor = Cursor::new(jar_bytes.as_ref());
    let mut archive =
        ZipArchive::new(cursor).map_err(|e| ResourcePackError::Download(e.to_string()))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ResourcePackError::Download(e.to_string()))?;

        if !entry.is_file() {
            continue;
        }

        let name = entry.name().to_string();
        if !name.starts_with("assets/minecraft") {
            continue;
        }

        let relative = name.strip_prefix("assets/").unwrap();
        let outpath_str = format!("{cache_root}/{relative}");
        let outpath = FsPath::from(outpath_str.as_str());

        if let Some(parent) = outpath.parent() {
            fs.create_dir_all(&parent).await?;
        }

        let mut data = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut data)
            .map_err(|e| ResourcePackError::Download(e.to_string()))?;

        fs.write(&outpath, &data).await?;
    }

    Ok(cache_root)
}
