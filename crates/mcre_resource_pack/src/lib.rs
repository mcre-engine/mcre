pub mod blockstates;
pub mod colormap;
pub mod discover;
pub mod error;
pub mod interner;
pub mod lookup;
pub mod models;
pub mod pack;
pub mod pack_meta;
pub mod resolve;
pub mod tint;

#[cfg(feature = "download")]
pub mod download;

pub use colormap::{
    COLORMAP_NAMES, Colormaps, DRY_FOLIAGE_DEFAULT, FOLIAGE_DEFAULT, GRASS_DEFAULT, sample_colormap,
};
pub use error::{ResourcePackError, Result};
pub use lookup::ResourcePackData;
pub use tint::{TintSource, tint_sources};

use std::collections::HashMap;

use crate::blockstates::merge_blockstate_definitions;
use crate::interner::StringInterner;
use crate::models::merge_block_model_definitions;
use crate::pack::ResourcePack;
use crate::resolve::resolve_all_models;

use mcre_fs::Fs;
use mcre_fs::path::FsPath;

/// A set of resource packs loaded in order (last pack wins).
pub struct ResourcePackSet {
    packs: Vec<ResourcePack>,
}

impl ResourcePackSet {
    pub fn new() -> Self {
        Self { packs: Vec::new() }
    }

    /// Add a resource pack from a directory path.
    pub async fn add_pack(&mut self, fs: &impl Fs, path: &FsPath) -> Result<()> {
        let pack = ResourcePack::load(fs, path).await?;
        self.packs.push(pack);
        Ok(())
    }

    /// Add the default Minecraft resource pack.
    /// Downloads it if not yet cached (requires the `download` feature).
    #[cfg(feature = "download")]
    pub async fn add_default(&mut self, fs: &impl Fs) -> Result<()> {
        let path = download::ensure_default_pack(fs).await?;
        self.add_pack(fs, &path).await
    }

    /// Loaded packs in stack order (bottom first).
    pub fn packs(&self) -> &[ResourcePack] {
        &self.packs
    }

    /// Texture content roots in stack order (bottom first): each pack's root
    /// followed by its applicable overlay directories.
    pub fn texture_roots(&self) -> Vec<FsPath> {
        self.packs
            .iter()
            .flat_map(|pack| pack.texture_roots())
            .collect()
    }

    /// Build the final lookup table by running all loading stages.
    pub async fn build(self, _fs: &impl Fs) -> Result<ResourcePackData> {
        // Stage 2: Merge blockstate definitions across all packs (last wins)
        let blockstate_defs = merge_blockstate_definitions(&self.packs);

        // Stage 3: Merge block model definitions across all packs (last wins)
        let model_defs = merge_block_model_definitions(&self.packs);

        // Stage 3b: Merge colormaps across all packs (last wins per map)
        let mut maps: [Option<Box<[u8]>>; 3] = [None, None, None];
        for pack in &self.packs {
            for (slot, name) in colormap::COLORMAP_NAMES.iter().enumerate() {
                if let Some(bytes) = pack.colormaps.get(*name)
                    && let Some(decoded) = colormap::decode_colormap(bytes)
                {
                    maps[slot] = Some(decoded);
                }
            }
        }
        let [grass, foliage, dry_foliage] = maps;
        let colormaps = Colormaps {
            grass: grass.ok_or_else(|| ResourcePackError::MissingColormap("grass".to_string()))?,
            foliage: foliage
                .ok_or_else(|| ResourcePackError::MissingColormap("foliage".to_string()))?,
            dry_foliage: dry_foliage
                .ok_or_else(|| ResourcePackError::MissingColormap("dry_foliage".to_string()))?,
        };

        // Stage 4: Resolve models for each block_state_id
        let resolved = resolve_all_models(&blockstate_defs, &model_defs)?;

        // Stage 4b: Merge culling overrides across all packs (last wins per block)
        let mut culling = HashMap::new();
        for pack in &self.packs {
            for (name, overrides) in &pack.culling {
                culling.insert(name.clone(), overrides.clone());
            }
        }

        // Stage 5: Build string interner
        let mut interner = StringInterner::new();
        let (entries, particle_texture_ids) =
            lookup::build_entries(&resolved, &model_defs, &mut interner)?;

        // Stage 6: Construct final lookup table
        let data =
            ResourcePackData::new(entries, particle_texture_ids, interner, culling, colormaps);

        Ok(data)
    }
}

impl Default for ResourcePackSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcre_fs::native::NativeFs;

    fn solid_png(rgb: [u8; 3]) -> Vec<u8> {
        let mut pixels = Vec::with_capacity(256 * 256 * 3);
        for _ in 0..256 * 256 {
            pixels.extend_from_slice(&rgb);
        }
        let mut encoded = Vec::new();
        let mut encoder = png::Encoder::new(&mut encoded, 256, 256);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
        encoded
    }

    fn stage_pack(root: &std::path::Path, name: &str, rgb: [u8; 3]) {
        let dir = root.join(name);
        std::fs::create_dir_all(dir.join("assets/minecraft/textures/colormap")).unwrap();
        std::fs::write(
            dir.join("pack.mcmeta"),
            r#"{"pack": {"description": "Test"}}"#,
        )
        .unwrap();
        for map in colormap::COLORMAP_NAMES {
            std::fs::write(
                dir.join(format!("assets/minecraft/textures/colormap/{map}.png")),
                solid_png(rgb),
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn colormaps_merge_last_wins() {
        let root =
            std::env::temp_dir().join(format!("mcre-pack-merge-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();
        stage_pack(&root, "base", [255, 0, 0]);
        stage_pack(&root, "top", [0, 255, 0]);

        let mut set = ResourcePackSet::new();
        set.add_pack(&fs, &FsPath::from("base")).await.unwrap();
        set.add_pack(&fs, &FsPath::from("top")).await.unwrap();
        let data = set.build(&fs).await.unwrap();

        assert_eq!(&data.colormaps.grass[0..3], &[0, 255, 0]);
        assert_eq!(&data.colormaps.foliage[0..3], &[0, 255, 0]);
        assert_eq!(&data.colormaps.dry_foliage[0..3], &[0, 255, 0]);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn builtin_texture_ids_resolve() {
        let root =
            std::env::temp_dir().join(format!("mcre-pack-builtin-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();
        stage_pack(&root, "base", [255, 0, 0]);

        let mut set = ResourcePackSet::new();
        set.add_pack(&fs, &FsPath::from("base")).await.unwrap();
        let data = set.build(&fs).await.unwrap();

        let path = |id: u32| data.texture_table[id as usize].as_ref();
        let chests = data.chest_textures;
        assert_eq!(path(chests.normal.single), "minecraft:entity/chest/normal");
        assert_eq!(
            path(chests.normal.left),
            "minecraft:entity/chest/normal_left"
        );
        assert_eq!(
            path(chests.normal.right),
            "minecraft:entity/chest/normal_right"
        );
        assert_eq!(path(chests.ender), "minecraft:entity/chest/ender");
        assert_eq!(
            path(chests.copper_weathered.single),
            "minecraft:entity/chest/copper_weathered"
        );
        assert_eq!(
            path(chests.copper_oxidized.right),
            "minecraft:entity/chest/copper_oxidized_right"
        );
        assert_eq!(path(data.font_textures.ascii), "minecraft:font/ascii");
        assert_eq!(
            path(data.entity_textures.pig_temperate),
            "minecraft:entity/pig/pig_temperate"
        );
        assert_eq!(
            path(data.entity_textures.pig_warm),
            "minecraft:entity/pig/pig_warm"
        );
        assert_eq!(
            path(data.entity_textures.cow_temperate),
            "minecraft:entity/cow/cow_temperate"
        );
        assert_eq!(
            path(data.entity_textures.chicken_temperate),
            "minecraft:entity/chicken/chicken_temperate"
        );
        assert_eq!(
            path(data.entity_textures.chicken_warm),
            "minecraft:entity/chicken/chicken_warm"
        );
        assert_eq!(
            path(data.entity_textures.creeper),
            "minecraft:entity/creeper/creeper"
        );
        assert_eq!(
            path(data.entity_textures.sheep),
            "minecraft:entity/sheep/sheep"
        );
        assert_eq!(
            path(data.entity_textures.sheep_wool),
            "minecraft:entity/sheep/sheep_wool"
        );
        assert_eq!(
            path(data.entity_textures.zombie),
            "minecraft:entity/zombie/zombie"
        );
        assert_eq!(
            path(data.entity_textures.drowned),
            "minecraft:entity/zombie/drowned"
        );
        assert_eq!(
            path(data.entity_textures.husk),
            "minecraft:entity/zombie/husk"
        );
        assert_eq!(
            path(data.entity_textures.skeleton),
            "minecraft:entity/skeleton/skeleton"
        );
        assert_eq!(
            path(data.entity_textures.wolf_pale),
            "minecraft:entity/wolf/wolf"
        );
        assert_eq!(
            path(data.entity_textures.wolf_spotted),
            "minecraft:entity/wolf/wolf_spotted"
        );
        assert_eq!(
            path(data.entity_textures.wolf_snowy),
            "minecraft:entity/wolf/wolf_snowy"
        );
        assert_eq!(
            path(data.entity_textures.wolf_black),
            "minecraft:entity/wolf/wolf_black"
        );
        assert_eq!(
            path(data.entity_textures.wolf_ashen),
            "minecraft:entity/wolf/wolf_ashen"
        );
        assert_eq!(
            path(data.entity_textures.wolf_rusty),
            "minecraft:entity/wolf/wolf_rusty"
        );
        assert_eq!(
            path(data.entity_textures.wolf_woods),
            "minecraft:entity/wolf/wolf_woods"
        );
        assert_eq!(
            path(data.entity_textures.wolf_chestnut),
            "minecraft:entity/wolf/wolf_chestnut"
        );
        assert_eq!(
            path(data.entity_textures.wolf_striped),
            "minecraft:entity/wolf/wolf_striped"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn colormaps_missing_is_an_error() {
        let root =
            std::env::temp_dir().join(format!("mcre-pack-empty-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fs = NativeFs::new_custom_path(root.clone()).await.unwrap();
        std::fs::create_dir_all(root.join("pack")).unwrap();
        std::fs::write(
            root.join("pack/pack.mcmeta"),
            r#"{"pack": {"description": "Test"}}"#,
        )
        .unwrap();

        let mut set = ResourcePackSet::new();
        set.add_pack(&fs, &FsPath::from("pack")).await.unwrap();
        let result = set.build(&fs).await;
        assert!(matches!(result, Err(ResourcePackError::MissingColormap(_))));

        std::fs::remove_dir_all(&root).unwrap();
    }
}
