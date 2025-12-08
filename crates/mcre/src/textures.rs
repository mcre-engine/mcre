use bevy::{asset::LoadState, platform::collections::HashMap, prelude::*};
use mcre_core::{Block, BlockState};

use crate::LoadingState;

const BATCH_SIZE: usize = 10;

#[derive(Resource)]
pub enum BlockTextures {
    Loading {
        // Should probably be `TextureId` not `Block`
        all: Vec<(Block, Option<Handle<Image>>)>,
        cur_index: usize,
        batch: Vec<(usize, Handle<Image>)>,
    },
    Loaded {
        texture: Handle<StandardMaterial>,
        atlas: TextureAtlasLayout,
        blocks: HashMap<Block, usize>,
    },
}

impl Default for BlockTextures {
    fn default() -> Self {
        BlockTextures::Loading {
            all: Block::all().map(|b| (b, None)).collect(),
            cur_index: 0,
            batch: Vec::with_capacity(BATCH_SIZE),
        }
    }
}

impl BlockTextures {
    // Updates batch processing and returns true if finished
    pub fn update_batch(
        &mut self,
        asset_server: &AssetServer,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
    ) -> bool {
        let handles = match self {
            BlockTextures::Loading {
                all,
                cur_index,
                batch,
            } => {
                batch.retain(
                    |(index, handle)| match asset_server.get_load_state(handle.id()) {
                        Some(LoadState::Loaded) => {
                            all[*index].1 = Some(handle.clone());
                            false
                        }
                        Some(LoadState::Failed(err)) => {
                            warn!("Failed to load texture {err:?}");
                            false
                        }
                        None => {
                            warn!("Unknown Asset");
                            false
                        }
                        _ => true,
                    },
                );
                if *cur_index < all.len() - 1 {
                    let diff = BATCH_SIZE - batch.len();
                    if diff > 0 {
                        for i in 0..diff {
                            let new_index = *cur_index + i;
                            if new_index >= all.len() {
                                break;
                            }
                            //TODO: Fix for different textures i.e. using texture id from
                            //BlockState
                            let handle = asset_server.load(format!(
                                "minecraft/textures/block/{}.png",
                                all[new_index].0.name()
                            ));
                            batch.push((new_index, handle));
                        }
                        *cur_index += diff;
                    }
                    return false;
                }
                all.drain(..)
                    .filter_map(|(block, handle)| handle.map(|handle| (block, handle)))
                    .collect::<Vec<_>>()
            }
            BlockTextures::Loaded { .. } => {
                return true;
            }
        };

        let mut builder = TextureAtlasBuilder::default();
        let mut blocks = HashMap::new();
        for (i, (block, handle)) in handles.iter().enumerate() {
            let texture = images.get(handle.id()).unwrap();
            builder.add_texture(Some(handle.id()), texture);
            blocks.insert(*block, i);
        }

        let (atlas, _sources, texture) = builder.build().unwrap();

        for (_, handle) in handles {
            images.remove(handle.id());
        }
        let texture = images.add(texture);

        let texture = materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Mask(0.5),
            reflectance: 0.0,
            // unlit: true,
            ..default()
        });

        *self = BlockTextures::Loaded {
            atlas,
            blocks,
            texture,
        };
        true
    }

    pub fn loading_percent(&self) -> f32 {
        match self {
            BlockTextures::Loading { all, cur_index, .. } => *cur_index as f32 / all.len() as f32,
            BlockTextures::Loaded { .. } => 1.0,
        }
    }

    pub fn texture(&self) -> Option<&Handle<StandardMaterial>> {
        match self {
            BlockTextures::Loading { .. } => None,
            BlockTextures::Loaded { texture, .. } => Some(texture),
        }
    }

    pub fn get_uv_rect(&self, block: BlockState) -> Option<Rect> {
        match self {
            BlockTextures::Loading { .. } => None,
            BlockTextures::Loaded { atlas, blocks, .. } => {
                let idx = blocks.get(&block.block())?;
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

    pub fn load_textures_system(
        mut commands: Commands,
        asset_server: Res<AssetServer>,
        mut images: ResMut<Assets<Image>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        // TODO: Fix, currently
        // Some blocks like `grindstone` have different states and thus its not just the name
        //    `grindstone` for the texture to use. Another example is GrassBlock has grass_block_side,
        //    grass_block_top, etc.
        let mut textures = BlockTextures::default();
        textures.update_batch(&asset_server, &mut images, &mut materials);
        commands.insert_resource(textures);
    }

    pub fn check_loaded_textures_system(
        mut next_state: ResMut<NextState<LoadingState>>,
        mut textures: ResMut<BlockTextures>,
        asset_server: Res<AssetServer>,
        mut images: ResMut<Assets<Image>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        if textures.update_batch(&asset_server, &mut images, &mut materials) {
            //TODO: Setup event here instead
            next_state.set(LoadingState::Chunks);
        }
    }
}
