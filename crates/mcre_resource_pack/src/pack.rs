use std::collections::HashMap;

use mcre_fs::Fs;
use mcre_fs::path::FsPath;

use crate::error::{ResourcePackError, Result};
use crate::pack_meta::PackMeta;

/// A loaded resource pack with raw JSON content stored in memory.
#[derive(Debug, Clone)]
pub struct ResourcePack {
    /// Parsed pack.mcmeta
    pub meta: PackMeta,
    /// Root path of this pack
    pub path: FsPath,
    /// Blockstate definitions: key = "namespace:blockname", value = raw JSON
    pub blockstates: HashMap<String, String>,
    /// Model definitions: key = "namespace:modelpath", value = raw JSON
    /// modelpath is like "block/stone" or "item/sword"
    pub models: HashMap<String, String>,
    /// Insertion order for deterministic override behavior
    pub order: usize,
}

impl ResourcePack {
    /// Loads a resource pack from a directory via the given filesystem.
    ///
    /// Reads `pack.mcmeta` for validation, then enumerates all
    /// `assets/<namespace>/blockstates/*.json` and `assets/<namespace>/models/block/*.json`
    /// files and stores their raw content.
    pub async fn load(fs: &impl Fs, path: &FsPath) -> Result<Self> {
        static NEXT_ORDER: core::sync::atomic::AtomicUsize =
            core::sync::atomic::AtomicUsize::new(0);

        let meta_path = path.join("pack.mcmeta");
        if !fs.exists(&meta_path).await {
            return Err(ResourcePackError::InvalidPackFormat(format!(
                "pack.mcmeta not found at {path}"
            )));
        }

        let meta_json = fs.read_to_string(&meta_path).await?;
        let meta: PackMeta = serde_json::from_str(&meta_json)?;

        let assets_path = path.join("assets");

        let mut blockstates = HashMap::new();
        let mut models = HashMap::new();

        // Enumerate namespaces under assets/
        if fs.is_dir(&assets_path).await {
            let namespace_entries = fs.read_dir(&assets_path).await?;
            for ns_entry in &namespace_entries {
                if !ns_entry.metadata.is_dir {
                    continue;
                }
                let namespace = &ns_entry.name;

                // Load blockstates: assets/<ns>/blockstates/*.json
                let blockstates_dir = ns_entry.path.join("blockstates");
                Self::load_json_dir(
                    fs,
                    &blockstates_dir,
                    namespace,
                    "",
                    ".json",
                    &mut blockstates,
                )
                .await;

                // Load block models: assets/<ns>/models/block/*.json
                let models_block_dir = ns_entry.path.join("models").join("block");
                Self::load_json_dir(
                    fs,
                    &models_block_dir,
                    namespace,
                    "block/",
                    ".json",
                    &mut models,
                )
                .await;
            }
        }

        let order = NEXT_ORDER.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        Ok(Self {
            meta,
            path: path.clone(),
            blockstates,
            models,
            order,
        })
    }

    /// Returns the raw JSON for a blockstate definition by block name.
    pub fn blockstate_json(&self, block_name: &str) -> Option<&str> {
        self.blockstates.get(block_name).map(|s| s.as_str())
    }

    /// Returns the raw JSON for a model definition by model path.
    pub fn model_json(&self, model_path: &str) -> Option<&str> {
        self.models.get(model_path).map(|s| s.as_str())
    }

    /// Checks if a filter pattern in this pack's `pack.mcmeta` matches the given
    /// (namespace, path) pair. Used to remove entries from lower-priority packs.
    pub fn matches_filter(&self, namespace: &str, key_path: &str) -> bool {
        let Some(filter) = &self.meta.filter else {
            return false;
        };
        // If any filter pattern matches, the entry is blocked
        filter.block.iter().any(|pattern| {
            let ns_match = pattern
                .namespace
                .as_deref()
                .is_none_or(|ns| regex_match(ns, namespace));

            let path_match = pattern
                .path
                .as_deref()
                .is_none_or(|p| regex_match(p, key_path));

            ns_match && path_match
        })
    }

    /// Loads all JSON files from a directory into a map.
    async fn load_json_dir(
        fs: &impl Fs,
        dir_path: &FsPath,
        namespace: &str,
        key_prefix: &str,
        suffix: &str,
        map: &mut HashMap<String, String>,
    ) {
        if !fs.is_dir(dir_path).await {
            return;
        }
        let entries = match fs.read_dir(dir_path).await {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in &entries {
            if !entry.metadata.is_file {
                continue;
            }
            let name = &entry.name;
            if let Some(stem) = name.strip_suffix(suffix) {
                let key = format!("{namespace}:{key_prefix}{stem}");
                let content = fs.read_to_string(&entry.path).await;
                if let Ok(json) = content {
                    map.insert(key, json);
                }
            }
        }
    }
}

/// A simple regex-lite pattern matcher.
/// Supports `.*` (any sequence), `\.` escaped dots, and prefix/suffix patterns.
fn regex_match(pattern: &str, s: &str) -> bool {
    // Handle `.*` as wildcard (matches everything)
    if pattern == ".*" {
        return true;
    }
    // Split on `.*` to handle wildcards in any position
    let parts: Vec<&str> = pattern.split(".*").collect();
    let unescape = |p: &str| -> String { p.replace(r"\.", ".") };

    match parts.len() {
        1 => s == unescape(parts[0]),
        2 => {
            let prefix = parts[0];
            let suffix = parts[1];
            let prefix_ok = prefix.is_empty() || s.starts_with(prefix);
            let suffix_ok = suffix.is_empty() || s.ends_with(&unescape(suffix));
            prefix_ok && suffix_ok
        }
        _ => {
            // Multiple wildcards: check prefix, suffix, and mid parts
            let prefix = parts[0];
            let suffix = parts[parts.len() - 1];
            if !prefix.is_empty() && !s.starts_with(prefix) {
                return false;
            }
            if !suffix.is_empty() && !s.ends_with(&unescape(suffix)) {
                return false;
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex_match_wildcard() {
        assert!(regex_match(".*", "anything"));
    }

    #[test]
    fn test_regex_match_prefix() {
        assert!(regex_match("minecraft.*", "minecraft:stone"));
        assert!(!regex_match("minecraft.*", "mojang:stone"));
    }

    #[test]
    fn test_regex_match_suffix() {
        assert!(regex_match(".*_wall\\.json", "oak_wall.json"));
        assert!(!regex_match(".*_wall\\.json", "stone.json"));
    }

    #[test]
    fn test_filter_simple() {
        assert!(regex_match("minecraft", "minecraft"));
        assert!(!regex_match("minecraft", "minecraft:stone"));
    }
}
