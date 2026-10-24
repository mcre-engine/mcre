use std::io::{Cursor, Read};

use zip::ZipArchive;

use mcre_fs::{Fs, path::FsPath};

use crate::error::{ResourcePackError, Result};
use crate::pack_meta::PackMeta;

/// A resource pack found by scanning a directory: either an unpacked folder
/// or a `.zip` archive (vanilla layout in both cases).
#[derive(Debug, Clone)]
pub struct DiscoveredPack {
    /// Stable selection key: the file or directory name.
    pub id: String,
    /// Human label from `pack.mcmeta`'s description (falls back to `id`).
    pub display_name: String,
    /// Path to the folder or `.zip` file.
    pub path: FsPath,
    /// True for `.zip` archives (need extraction before loading).
    pub is_zip: bool,
    /// File size for zips (0 for folders); keys cache freshness.
    pub size: u64,
    /// Format compatibility when `pack.mcmeta` parses; `None` when the pack
    /// is broken (see `error`).
    pub compatible: Option<bool>,
    /// Why this pack cannot load (`None` when healthy).
    pub error: Option<String>,
}

fn display_name(id: &str, meta: Option<&PackMeta>) -> String {
    match meta.and_then(|meta| meta.pack.description.as_str()) {
        Some(text) => text.to_string(),
        None => id.to_string(),
    }
}

fn read_meta_from_zip(bytes: &[u8]) -> Result<PackMeta> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
    let mut entry = archive
        .by_name("pack.mcmeta")
        .map_err(|_| ResourcePackError::InvalidPackFormat("pack.mcmeta not found".to_string()))?;
    let mut json = String::new();
    entry
        .read_to_string(&mut json)
        .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
    serde_json::from_str(&json).map_err(ResourcePackError::Json)
}

/// Scans `dir` for resource packs: subdirectories and `.zip` files.
/// Never fails the whole scan for one broken pack; broken packs are listed
/// with `error` set so the UI can show (and skip) them.
pub async fn discover_packs(fs: &impl Fs, dir: &FsPath) -> Vec<DiscoveredPack> {
    if !fs.is_dir(dir).await {
        return Vec::new();
    }
    let entries = match fs.read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut packs = Vec::new();
    for entry in &entries {
        if entry.metadata.is_dir {
            let meta_path = entry.path.join("pack.mcmeta");
            match fs.read_to_string(&meta_path).await {
                Ok(json) => match serde_json::from_str::<PackMeta>(&json) {
                    Ok(meta) => packs.push(DiscoveredPack {
                        id: entry.name.clone(),
                        display_name: display_name(&entry.name, Some(&meta)),
                        path: entry.path.clone(),
                        is_zip: false,
                        size: 0,
                        compatible: Some(meta.supports_current_version()),
                        error: None,
                    }),
                    Err(err) => {
                        packs.push(broken(&entry.name, &entry.path, false, 0, err.to_string()))
                    }
                },
                Err(_) => packs.push(broken(
                    &entry.name,
                    &entry.path,
                    false,
                    0,
                    "pack.mcmeta not found".to_string(),
                )),
            }
        } else if entry.metadata.is_file && entry.name.ends_with(".zip") {
            match fs.read(&entry.path).await {
                Ok(bytes) => match read_meta_from_zip(&bytes) {
                    Ok(meta) => packs.push(DiscoveredPack {
                        id: entry.name.clone(),
                        display_name: display_name(&entry.name, Some(&meta)),
                        path: entry.path.clone(),
                        is_zip: true,
                        size: bytes.len() as u64,
                        compatible: Some(meta.supports_current_version()),
                        error: None,
                    }),
                    Err(err) => packs.push(broken(
                        &entry.name,
                        &entry.path,
                        true,
                        bytes.len() as u64,
                        err.to_string(),
                    )),
                },
                Err(err) => packs.push(broken(&entry.name, &entry.path, true, 0, err.to_string())),
            }
        }
    }
    packs.sort_by(|a, b| a.id.cmp(&b.id));
    packs
}

fn broken(id: &str, path: &FsPath, is_zip: bool, size: u64, error: String) -> DiscoveredPack {
    DiscoveredPack {
        id: id.to_string(),
        display_name: id.to_string(),
        path: path.clone(),
        is_zip,
        size,
        compatible: None,
        error: Some(error),
    }
}

const ZIP_MARKER: &str = ".mcre_zip_marker";

/// Resolves a discovered pack to a loadable directory: folders pass through,
/// zips extract into `cache_dir/<stem>` (re-extracted when the zip size
/// changed since). Rejects zip-slip paths.
pub async fn ensure_extracted(
    fs: &impl Fs,
    pack: &DiscoveredPack,
    cache_dir: &FsPath,
) -> Result<FsPath> {
    if !pack.is_zip {
        return Ok(pack.path.clone());
    }
    let stem = pack.id.strip_suffix(".zip").unwrap_or(&pack.id).to_string();
    let target = cache_dir.join(stem.as_str());
    let marker = target.join(ZIP_MARKER);
    if let Ok(note) = fs.read_to_string(&marker).await
        && note.trim() == pack.size.to_string()
        && fs.is_dir(&target).await
    {
        return Ok(target);
    }
    fs.remove_dir_all(&target).await.ok();
    fs.create_dir_all(&target).await?;
    let bytes = fs.read(&pack.path).await?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
        let name = entry
            .name()
            .map_err(|err| ResourcePackError::Zip(err.to_string()))?
            .to_string();
        if name.contains("..") || name.starts_with('/') || name.starts_with('\\') {
            return Err(ResourcePackError::Zip(format!("unsafe path: {name}")));
        }
        if entry.is_dir() {
            continue;
        }
        let parts: Vec<Box<str>> = name
            .split('/')
            .filter(|part| !part.is_empty())
            .map(Box::from)
            .collect();
        if parts.is_empty() {
            continue;
        }
        let out = parts
            .into_iter()
            .fold(target.clone(), |path, part| path.join(part));
        if let Some(parent) = out.parent() {
            fs.create_dir_all(&parent).await?;
        }
        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
        fs.write(&out, &contents).await?;
    }
    fs.write(&marker, pack.size.to_string().as_bytes()).await?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcre_fs::native::NativeFs;

    fn test_root(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("mcre-discover-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("packs")).unwrap();
        root
    }

    fn stage_dir(root: &std::path::Path, name: &str, mcmeta: Option<&str>) {
        let dir = root.join("packs").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(meta) = mcmeta {
            std::fs::write(dir.join("pack.mcmeta"), meta).unwrap();
        }
    }

    fn stage_zip(root: &std::path::Path, name: &str, files: &[(&str, &[u8])]) {
        let path = root.join("packs").join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, contents) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            use std::io::Write;
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap();
    }

    const META: &str = r#"{"pack": {"description": "Pretty Pack"}}"#;
    const META_OLD: &str = r#"{"pack": {"description": "Old Pack", "pack_format": 18}}"#;

    #[tokio::test]
    async fn discovers_dirs_zips_and_broken_packs() {
        let root = test_root("list");
        stage_dir(&root, "good", Some(META));
        stage_dir(&root, "old", Some(META_OLD));
        stage_dir(&root, "broken", None);
        stage_dir(
            &root,
            "empty",
            Some(r#"{"pack": {"description": "Empty"}}"#),
        );
        stage_zip(
            &root,
            "zipped.zip",
            &[("pack.mcmeta", META.as_bytes()), ("assets/x.txt", b"x")],
        );
        stage_zip(&root, "notazip.zip", &[("pack.mcmeta", b"{oops")]);
        std::fs::write(root.join("packs").join("notes.txt"), b"hi").unwrap();
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();

        let packs = discover_packs(&fs, &FsPath::from("packs")).await;
        let by_id = |id: &str| packs.iter().find(|pack| pack.id == id).unwrap();

        let good = by_id("good");
        assert_eq!(good.display_name, "Pretty Pack");
        assert!(!good.is_zip);
        assert_eq!(good.compatible, Some(true));
        assert!(good.error.is_none());

        assert_eq!(by_id("old").compatible, Some(false));
        assert!(by_id("broken").error.is_some());
        assert_eq!(by_id("broken").compatible, None);

        let zipped = by_id("zipped.zip");
        assert!(zipped.is_zip);
        assert!(zipped.size > 0);
        assert_eq!(zipped.compatible, Some(true));

        assert!(by_id("notazip.zip").error.is_some());
        assert!(packs.iter().all(|pack| pack.id != "notes.txt"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn extraction_is_cached_and_rejects_zip_slip() {
        let root = test_root("extract");
        stage_zip(
            &root,
            "zipped.zip",
            &[("pack.mcmeta", META.as_bytes()), ("assets/a.txt", b"a")],
        );
        stage_zip(
            &root,
            "evil.zip",
            &[("pack.mcmeta", META.as_bytes()), ("../evil.txt", b"!")],
        );
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();
        let cache = FsPath::from("cache");

        let packs = discover_packs(&fs, &FsPath::from("packs")).await;
        let zipped = packs.iter().find(|pack| pack.id == "zipped.zip").unwrap();
        let out = ensure_extracted(&fs, zipped, &cache).await.unwrap();
        assert!(fs.exists(&out.join("assets").join("a.txt")).await);
        let again = ensure_extracted(&fs, zipped, &cache).await.unwrap();
        assert_eq!(out, again);

        let evil = packs.iter().find(|pack| pack.id == "evil.zip").unwrap();
        assert!(ensure_extracted(&fs, evil, &cache).await.is_err());
        assert!(!root.join("evil.txt").exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn missing_dir_discovers_nothing() {
        let root = test_root("missing");
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();
        assert!(discover_packs(&fs, &FsPath::from("nope")).await.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
