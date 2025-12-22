use bevy::{asset::LoadState, platform::collections::HashMap, prelude::*};
use mcre_core::{Block, BlockState};

use crate::LoadingState;

const BATCH_SIZE: usize = 20;

#[derive(Resource)]
pub struct BlockTexturesBuilder {
    all: Vec<(Block, Option<Handle<Image>>)>,
    cur_index: usize,
    batch: Vec<(usize, Handle<Image>)>,
}

impl Default for BlockTexturesBuilder {
    fn default() -> Self {
        BlockTexturesBuilder {
            all: Block::all().map(|b| (b, None)).collect(),
            cur_index: 0,
            batch: Vec::with_capacity(BATCH_SIZE),
        }
    }
}

impl BlockTexturesBuilder {
    pub fn update_builder_system(
        mut commands: Commands,
        mut builder: ResMut<Self>,
        mut next_state: ResMut<NextState<LoadingState>>,
        asset_server: Res<AssetServer>,
        mut images: ResMut<Assets<Image>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        if builder.update(&asset_server) {
            let textures = builder.build(&mut images, &mut materials);
            commands.insert_resource(textures);
            commands.remove_resource::<Self>();
            //TODO Setup event instead
            next_state.set(LoadingState::Chunks);
        }
    }

    pub fn update(&mut self, asset_server: &AssetServer) -> bool {
        self.batch.retain(
            |(index, handle)| match asset_server.get_load_state(handle.id()) {
                Some(LoadState::Loaded) => {
                    self.all[*index].1 = Some(handle.clone());
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
        if self.cur_index < self.all.len() - 1 {
            let diff = BATCH_SIZE - self.batch.len();
            if diff > 0 {
                for i in 0..diff {
                    let new_index = self.cur_index + i;
                    if new_index >= self.all.len() {
                        break;
                    }
                    //TODO: Fix for different textures i.e. using texture id from
                    //BlockState
                    let handle = asset_server.load(format!(
                        "minecraft/textures/block/{}.png",
                        self.all[new_index].0.name()
                    ));
                    self.batch.push((new_index, handle));
                }
                self.cur_index += diff;
            }
            return false;
        }
        true
    }

    pub fn build(
        &self,
        images: &mut Assets<Image>,
        materials: &mut Assets<StandardMaterial>,
    ) -> BlockTextures {
        let handles = self
            .all
            .iter()
            .filter_map(|(b, h)| h.as_ref().map(|h| (b, h)))
            .collect::<Vec<_>>();
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
        BlockTextures {
            texture,
            atlas: BlockTexturesAtlas {
                size: atlas.size,
                textures: blocks
                    .into_iter()
                    .map(|(b, i)| (*b, atlas.textures[i]))
                    .collect(),
            },
        }
    }

    pub fn loading_percent(&self) -> f32 {
        self.cur_index as f32 / self.all.len() as f32
    }
}

#[derive(Resource)]
pub struct BlockTextures {
    pub texture: Handle<StandardMaterial>,
    pub atlas: BlockTexturesAtlas,
}

#[derive(Clone)]
pub struct BlockTexturesAtlas {
    size: UVec2,
    textures: HashMap<Block, URect>,
}

impl BlockTexturesAtlas {
    pub fn uv_rect(&self, block: BlockState) -> Option<Rect> {
        let size = self.textures.get(&block.block())?;
        Some(Rect {
            min: Vec2::new(
                size.min.x as f32 / self.size.x as f32,
                size.min.y as f32 / self.size.y as f32,
            ),
            max: Vec2::new(
                size.max.x as f32 / self.size.x as f32,
                size.max.y as f32 / self.size.y as f32,
            ),
        })
    }
}
