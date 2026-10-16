use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcclusionShape {
    pub boxes: Vec<[f64; 6]>,
}

impl OcclusionShape {
    pub fn all_sync() -> io::Result<Vec<Self>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let shape_data_path = root.join("shapes.json");
        let shape_data_json = std::fs::read_to_string(shape_data_path)?;
        Ok(serde_json::from_str(&shape_data_json)?)
    }

    pub async fn all() -> io::Result<Vec<Self>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let shape_data_path = root.join("shapes.json");
        let shape_data_json = fs::read_to_string(shape_data_path).await?;
        let shape_data: Vec<Self> = serde_json::from_str(&shape_data_json)?;

        Ok(shape_data)
    }
}

#[cfg(test)]
mod tests {
    use crate::shape::OcclusionShape;

    #[tokio::test]
    async fn test_shape_data_load() {
        let shapes = OcclusionShape::all().await.unwrap();
        assert!(!shapes.is_empty());
    }
}
