use bevy::{asset::LoadState, platform::collections::HashMap, prelude::*};
use mcre_core::BlockId;

use crate::AppState;

#[derive(Resource)]
pub enum BlockTextures {
    Loading(Vec<(BlockId, Handle<Image>)>),
    Loaded {
        texture: Handle<Image>,
        atlas: TextureAtlasLayout,
        blocks: HashMap<BlockId, usize>,
    },
}

impl BlockTextures {
    pub fn texture(&self) -> Option<&Handle<Image>> {
        match self {
            BlockTextures::Loading(_) => None,
            BlockTextures::Loaded { texture, .. } => Some(texture),
        }
    }

    pub fn get_uv_rect(&self, block: BlockId) -> Option<Rect> {
        match self {
            BlockTextures::Loading(_) => None,
            BlockTextures::Loaded { atlas, blocks, .. } => {
                let idx = blocks.get(&block)?;
                let size = atlas.textures[*idx];
                Some(Rect {
                    min: Vec2::new(
                        size.min.x as f32 / atlas.size.x as f32,
                        size.min.y as f32 / atlas.size.y as f32,
                    ),
                    max: Vec2::new(
                        size.max.x as f32 / atlas.size.x as f32,
                        size.max.y as f32 / atlas.size.y as f32,
                    ),
                })
            }
        }
    }
}

pub fn load_textures(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handles = [
        BlockId::DIRT,
        BlockId::STONE,
        BlockId::COBBLESTONE,
        BlockId::IRON_ORE,
        BlockId::DIAMOND_ORE,
        BlockId::BEDROCK,
    ]
    .into_iter()
    .map(|block| {
        (
            block,
            asset_server.load(format!("minecraft/textures/block/{}.png", block.name())),
        )
    })
    .collect();
    // TODO: Fix, currently two issues
    // 1. Too many files open (need some sort of batching)
    // 2. Some blocks like `grindstone` have different states and thus its not just the name
    //    `grindstone` for the texture to use. Another example is GrassBlock has grass_block_side,
    //    grass_block_top, etc.
    // let handles = Block::all()
    //     .into_iter()
    //     .map(|block| asset_server.load(format!("minecraft/textures/block/{}.png", block.name())))
    //     .collect();

    commands.insert_resource(BlockTextures::Loading(handles));
}

pub fn check_loaded_textures(
    mut next_app_state: ResMut<NextState<AppState>>,
    mut textures: ResMut<BlockTextures>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    //TODO: Make UI with loading screen

    let (blocks, texture, atlas) = match &mut *textures {
        BlockTextures::Loading(handles) => {
            for (_, handle) in handles.iter() {
                match asset_server.get_load_state(handle.id()) {
                    Some(LoadState::Loaded) => {
                        continue;
                    }
                    Some(LoadState::Failed(err)) => {
                        warn!("Failed to load texture {err:?}");
                    }
                    None => warn!("Unknown Asset"),
                    _ => {}
                }

                return;
            }

            let mut builder = TextureAtlasBuilder::default();
            let mut blocks = HashMap::new();
            for (i, (block, handle)) in handles.iter().enumerate() {
                let texture = images.get(handle.id()).unwrap();
                builder.add_texture(Some(handle.id()), texture);
                blocks.insert(*block, i);
            }

            let (atlas, _sources, texture) = builder.build().unwrap();
            (blocks, texture, atlas)
        }
        _ => {
            return;
        }
    };
    let texture = images.add(texture);
    *textures = BlockTextures::Loaded {
        blocks,
        texture,
        atlas,
    };
    next_app_state.set(AppState::InGame);
}
