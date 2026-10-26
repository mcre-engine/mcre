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
    /// File size for zips (0 for folders); keys cache freshness together with
    /// [`Self::fingerprint`].
    pub size: u64,
    /// FNV-1a hash of the zip bytes (0 for folders). Detects same-size edits
    /// that a size check misses.
    pub fingerprint: u64,
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
    let meta_path = pack_meta_path(&mut archive)?;
    let mut entry = archive
        .by_name(&meta_path)
        .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
    let mut json = String::new();
    entry
        .read_to_string(&mut json)
        .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
    serde_json::from_str(&json).map_err(ResourcePackError::Json)
}

fn pack_meta_path(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<String> {
    let mut wrapper_dirs = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
        if name == "pack.mcmeta" {
            return Ok(name.to_string());
        }
        if let Some((wrapper, "pack.mcmeta")) = name.rsplit_once('/')
            && !wrapper.is_empty()
            && !wrapper.contains('/')
        {
            wrapper_dirs.push(wrapper.to_string());
        }
    }
    wrapper_dirs.sort();
    wrapper_dirs.dedup();
    match wrapper_dirs.as_slice() {
        [wrapper] => Ok(format!("{wrapper}/pack.mcmeta")),
        _ => Err(ResourcePackError::InvalidPackFormat(
            "pack.mcmeta not found at archive root or under one top-level folder".to_string(),
        )),
    }
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
                        fingerprint: 0,
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
                        fingerprint: zip_fingerprint(&bytes),
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
        fingerprint: 0,
        compatible: None,
        error: Some(error),
    }
}

fn zip_fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

const ZIP_MARKER: &str = ".mcre_zip_marker";

/// Resolves a discovered pack to a loadable directory: folders pass through,
/// zips extract into `cache_dir/<stem>` (re-extracted when the zip's size or
/// content fingerprint changed since). Rejects zip-slip paths.
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
    let marker_note = format!("{}:{:016x}", pack.size, pack.fingerprint);
    if let Ok(note) = fs.read_to_string(&marker).await
        && note.trim() == marker_note
        && fs.is_dir(&target).await
    {
        return Ok(target);
    }
    fs.remove_dir_all(&target).await.ok();
    fs.create_dir_all(&target).await?;
    let bytes = fs.read(&pack.path).await?;
    let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice()))
        .map_err(|err| ResourcePackError::Zip(err.to_string()))?;
    let meta_path = pack_meta_path(&mut archive)?;
    let wrapper = meta_path.strip_suffix("pack.mcmeta").unwrap();
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
        let Some(name) = name.strip_prefix(wrapper) else {
            continue;
        };
        if name.is_empty() {
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
    fs.write(&marker, marker_note.as_bytes()).await?;
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
        stage_zip(
            &root,
            "wrapped.zip",
            &[
                ("wrapped/pack.mcmeta", META.as_bytes()),
                ("wrapped/assets/x.txt", b"x"),
                ("README.txt", b"ignored"),
            ],
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

        let wrapped = by_id("wrapped.zip");
        assert_eq!(wrapped.display_name, "Pretty Pack");
        assert_eq!(wrapped.compatible, Some(true));
        assert!(wrapped.error.is_none());

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
        stage_zip(
            &root,
            "wrapped.zip",
            &[
                ("wrapped/pack.mcmeta", META.as_bytes()),
                ("wrapped/assets/a.txt", b"a"),
                ("README.txt", b"ignored"),
            ],
        );
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();
        let cache = FsPath::from("cache");

        let packs = discover_packs(&fs, &FsPath::from("packs")).await;
        let zipped = packs.iter().find(|pack| pack.id == "zipped.zip").unwrap();
        let out = ensure_extracted(&fs, zipped, &cache).await.unwrap();
        assert!(fs.exists(&out.join("assets").join("a.txt")).await);
        let again = ensure_extracted(&fs, zipped, &cache).await.unwrap();
        assert_eq!(out, again);

        let wrapped = packs.iter().find(|pack| pack.id == "wrapped.zip").unwrap();
        let out = ensure_extracted(&fs, wrapped, &cache).await.unwrap();
        assert!(fs.exists(&out.join("pack.mcmeta")).await);
        assert!(fs.exists(&out.join("assets").join("a.txt")).await);
        assert!(!fs.exists(&out.join("wrapped")).await);
        assert!(!fs.exists(&out.join("README.txt")).await);

        let evil = packs.iter().find(|pack| pack.id == "evil.zip").unwrap();
        assert!(ensure_extracted(&fs, evil, &cache).await.is_err());
        assert!(!root.join("evil.txt").exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn same_size_zip_edit_is_reextracted() {
        let root = test_root("same-size");
        stage_zip(
            &root,
            "pack.zip",
            &[("pack.mcmeta", META.as_bytes()), ("assets/a.txt", b"one")],
        );
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();
        let cache = FsPath::from("cache");

        let first = discover_packs(&fs, &FsPath::from("packs"))
            .await
            .into_iter()
            .find(|pack| pack.id == "pack.zip")
            .unwrap();
        let out = ensure_extracted(&fs, &first, &cache).await.unwrap();
        assert_eq!(
            fs.read_to_string(&out.join("assets").join("a.txt"))
                .await
                .unwrap(),
            "one"
        );

        stage_zip(
            &root,
            "pack.zip",
            &[("pack.mcmeta", META.as_bytes()), ("assets/a.txt", b"two")],
        );
        let second = discover_packs(&fs, &FsPath::from("packs"))
            .await
            .into_iter()
            .find(|pack| pack.id == "pack.zip")
            .unwrap();
        assert_eq!(first.size, second.size);
        assert_ne!(first.fingerprint, second.fingerprint);

        let out = ensure_extracted(&fs, &second, &cache).await.unwrap();
        assert_eq!(
            fs.read_to_string(&out.join("assets").join("a.txt"))
                .await
                .unwrap(),
            "two"
        );

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
