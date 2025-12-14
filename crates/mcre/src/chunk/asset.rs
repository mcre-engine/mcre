use bevy::{
    asset::{AssetLoader, AsyncWriteExt, saver::AssetSaver},
    prelude::*,
};
use thiserror::Error;

use crate::chunk::Chunk;

pub struct ChunkAssetLoader<C = bincode::config::Configuration> {
    pub bincode_config: C,
}

impl Default for ChunkAssetLoader<bincode::config::Configuration> {
    fn default() -> Self {
        Self {
            bincode_config: bincode::config::standard(),
        }
    }
}

impl<C: bincode::config::Config> ChunkAssetLoader<C> {
    pub fn to_bytes(&self, chunk: &Chunk) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(chunk, self.bincode_config)
    }
}

impl<C: bincode::config::Config + Send + Sync + 'static> AssetLoader for ChunkAssetLoader<C> {
    type Asset = Chunk;

    type Settings = ();

    type Error = ChunkLoaderError;

    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        _load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> std::result::Result<Self::Asset, Self::Error> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await?;
        let (data, _) = bincode::serde::decode_from_slice::<Chunk, C>(&data, self.bincode_config)?;
        Ok(data)
    }

    fn extensions(&self) -> &[&str] {
        &["mcra"]
    }
}

impl<C: bincode::config::Config + Send + Sync + 'static> AssetSaver for ChunkAssetLoader<C> {
    type Asset = Chunk;

    type Settings = ();

    type OutputLoader = Self;

    type Error = ChunkLoaderError;

    async fn save(
        &self,
        writer: &mut bevy::asset::io::Writer,
        asset: bevy::asset::saver::SavedAsset<'_, Self::Asset>,
        _settings: &Self::Settings,
    ) -> std::result::Result<<Self::OutputLoader as AssetLoader>::Settings, Self::Error> {
        let v = self.to_bytes(asset.get())?;
        Ok(writer.write_all(&v).await?)
    }
}

#[derive(Error, Debug)]
pub enum ChunkLoaderError {
    #[error(transparent)]
    Decode(#[from] bincode::error::DecodeError),
    #[error(transparent)]
    Encode(#[from] bincode::error::EncodeError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}
