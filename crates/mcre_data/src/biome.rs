use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrassColorModifier {
    #[default]
    None,
    DarkForest,
    Swamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Biome {
    pub id: u16,
    pub name: String,
    pub temperature: f32,
    pub downfall: f32,
    pub has_precipitation: bool,
    pub water_color: [u8; 3],
    pub foliage_color_override: Option<[u8; 3]>,
    pub dry_foliage_color_override: Option<[u8; 3]>,
    pub grass_color_override: Option<[u8; 3]>,
    #[serde(default)]
    pub grass_color_modifier: GrassColorModifier,
}

impl Biome {
    pub fn all_sync() -> io::Result<Vec<Self>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let biome_data_path = root.join("biomes.json");
        let biome_data_json = std::fs::read_to_string(biome_data_path)?;
        Ok(serde_json::from_str(&biome_data_json)?)
    }

    pub async fn all() -> io::Result<Vec<Self>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let biome_data_path = root.join("biomes.json");
        let biome_data_json = fs::read_to_string(biome_data_path).await?;
        let biome_data: Vec<Self> = serde_json::from_str(&biome_data_json)?;

        Ok(biome_data)
    }
}

#[cfg(test)]
mod tests {
    use crate::biome::Biome;

    #[tokio::test]
    async fn test_biome_data_load() {
        let biomes = Biome::all().await.unwrap();
        assert!(!biomes.is_empty());
    }
}
