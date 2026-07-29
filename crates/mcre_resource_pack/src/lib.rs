pub mod blockstates;
pub mod error;
pub mod interner;
pub mod lookup;
pub mod models;
pub mod pack;
pub mod pack_meta;
pub mod resolve;

#[cfg(feature = "download")]
pub mod download;

pub use error::{ResourcePackError, Result};
pub use lookup::ResourcePackData;

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

    /// Build the final lookup table by running all loading stages.
    pub async fn build(self, _fs: &impl Fs) -> Result<ResourcePackData> {
        // Stage 2: Merge blockstate definitions across all packs (last wins)
        let blockstate_defs = merge_blockstate_definitions(&self.packs);

        // Stage 3: Merge block model definitions across all packs (last wins)
        let model_defs = merge_block_model_definitions(&self.packs);

        // Stage 4: Resolve models for each block_state_id
        let resolved = resolve_all_models(&blockstate_defs, &model_defs)?;

        // Stage 5: Build string interner
        let mut interner = StringInterner::new();
        let entries = lookup::build_entries(&resolved, &model_defs, &mut interner)?;

        // Stage 6: Construct final lookup table
        let data = ResourcePackData::new(entries, interner);

        Ok(data)
    }
}

impl Default for ResourcePackSet {
    fn default() -> Self {
        Self::new()
    }
}
