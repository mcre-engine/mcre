pub mod asset;
mod generate;
pub mod loader;
pub mod math;

use bevy::{
    asset::RenderAssetUsages,
    color::palettes::css::{GREEN, WHITE},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use mcre_core::{Block, BlockState};
use serde::{Deserialize, Serialize};

use crate::{
    chunk::math::{pos::ChunkPosition, size::ChunkSize},
    textures::BlockTextures,
    utils::sparse::SparseArray,
};

#[derive(Asset, Clone, Debug, TypePath, Deserialize, Serialize)]
pub struct Chunk {
    pub loc: ChunkPosition,
    pub blocks: SparseArray<BlockState>,
    chunk_size: ChunkSize,
}

impl Chunk {
    pub fn empty<P: Into<ChunkPosition>>(chunk_size: ChunkSize, loc: P) -> Self {
        Chunk {
            loc: loc.into(),
            blocks: SparseArray::empty(chunk_size.full_size()),
            chunk_size,
        }
    }

    pub fn size(&self) -> &ChunkSize {
        &self.chunk_size
    }

    pub fn transform(&self) -> Transform {
        Transform::from_translation(self.loc.world_coord(self.chunk_size))
    }

    pub fn set_block<B: Into<BlockState>>(&mut self, pos: UVec3, new_block: B) {
        let idx = self.chunk_size.chunk_index(pos);
        if let Some(block) = self.blocks.get_mut(idx) {
            *block = new_block.into()
        } else {
            self.blocks.insert(idx, new_block.into());
        }
    }

    pub fn get(&self, pos: UVec3) -> Option<BlockState> {
        self.blocks.get(self.chunk_size.chunk_index(pos)).copied()
    }

    fn cull_faces(&self, pos: UVec3) -> (BVec3, BVec3) {
        fn check_occude(block: BlockState) -> bool {
            block.is_air() || !block.can_occlude()
        }

        let bounds = self.chunk_size.in_bounds(pos + 1);
        let positive_faces = BVec3::new(
            !bounds.x || self.get(pos.with_x(pos.x + 1)).is_none_or(check_occude),
            !bounds.y || self.get(pos.with_y(pos.y + 1)).is_none_or(check_occude),
            !bounds.z || self.get(pos.with_z(pos.z + 1)).is_none_or(check_occude),
        );

        let negative_faces = BVec3::new(
            pos.x < 1 || self.get(pos.with_x(pos.x - 1)).is_none_or(check_occude),
            pos.y < 1 || self.get(pos.with_y(pos.y - 1)).is_none_or(check_occude),
            pos.z < 1 || self.get(pos.with_z(pos.z - 1)).is_none_or(check_occude),
        );
        (positive_faces, negative_faces)
    }

    pub fn generate_mesh(&self, textures: &BlockTextures) -> Mesh {
        #[derive(Default)]
        struct VerticesBuilder {
            vertices: Vec<[f32; 3]>,
            normals: Vec<[f32; 3]>,
            uvs: Vec<[f32; 2]>,
            indices: Vec<u32>,
            vert_colors: Vec<[f32; 4]>,
        }

        // -Z is North, +Z is South
        // -X is West, +X is East
        impl VerticesBuilder {
            fn push_north(&mut self, cur: UVec3, uv: Rect, face_color: Srgba) {
                self.push_indices();
                self.push_face_color(face_color);
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [0., 0., -1.];
                self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
                self.push([x + 0., y + 1., z + 0.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 0., y + 0., z + 0.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 1., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
            }

            fn push_east(&mut self, cur: UVec3, uv: Rect, face_color: Srgba) {
                self.push_indices();
                self.push_face_color(face_color);
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [-1., 0., 0.];
                self.push([x + 1., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
                self.push([x + 1., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 1., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
            }

            fn push_south(&mut self, cur: UVec3, uv: Rect, face_color: Srgba) {
                self.push_indices();
                self.push_face_color(face_color);
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [0., 0., 1.];
                self.push([x + 0., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 1., y + 1., z + 1.], normal, [uv.min.x, uv.min.y]);
                self.push([x + 1., y + 0., z + 1.], normal, [uv.min.x, uv.max.y]);
            }

            fn push_west(&mut self, cur: UVec3, uv: Rect, face_color: Srgba) {
                self.push_indices();
                self.push_face_color(face_color);
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [1., 0., 0.];
                self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 0., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 0., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
                self.push([x + 0., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
            }

            fn push_up(&mut self, cur: UVec3, uv: Rect, face_color: Srgba) {
                self.push_indices();
                self.push_face_color(face_color);
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [0., 1., 0.];
                self.push([x + 0., y + 1., z + 0.], normal, [uv.min.x, uv.max.y]);
                self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
                self.push([x + 1., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.max.y]);
            }

            fn push_down(&mut self, cur: UVec3, uv: Rect, face_color: Srgba) {
                self.push_indices();
                self.push_face_color(face_color);
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [0., -1., 0.];
                self.push([x + 1., y + 0., z + 0.], normal, [uv.min.x, uv.min.y]);
                self.push([x + 0., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
                self.push([x + 0., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 1., y + 0., z + 1.], normal, [uv.max.x, uv.min.y]);
            }

            fn push_indices(&mut self) {
                let vertex_count = self.vertices.len() as u32;

                // 0, 3, 1, 1, 3, 2, // triangles making up the top (+y) facing side.
                self.indices.push(vertex_count);
                self.indices.push(vertex_count + 3);
                self.indices.push(vertex_count + 1);

                self.indices.push(vertex_count + 1);
                self.indices.push(vertex_count + 3);
                self.indices.push(vertex_count + 2);
            }

            fn push(&mut self, vertex: [f32; 3], normal: [f32; 3], uv: [f32; 2]) {
                self.vertices.push(vertex);
                self.normals.push(normal);
                self.uvs.push(uv);
            }

            fn push_face_color(&mut self, face_color: Srgba) {
                let vert_color = [
                    face_color.red,
                    face_color.green,
                    face_color.blue,
                    face_color.alpha,
                ];
                for _ in 0..4 {
                    self.vert_colors.push(vert_color);
                }
            }
        }
        let mut builder = VerticesBuilder::default();

        for (i, block) in self.blocks.iter() {
            if block.is_air() {
                continue;
            }
            let Some(uv_rect) = textures.get_uv_rect(*block) else {
                continue;
            };
            let cur = self.chunk_size.block_index(i);
            //TODO: Fix to use known data about block states
            let block_color = match block.block() {
                Block::OAK_LEAVES => GREEN,
                _ => WHITE,
            };

            let (positive, negative) = self.cull_faces(cur);

            if positive.x {
                builder.push_east(cur, uv_rect, block_color);
            }
            if positive.y {
                builder.push_up(cur, uv_rect, block_color);
            }
            if positive.z {
                builder.push_south(cur, uv_rect, block_color);
            }
            if negative.x {
                builder.push_west(cur, uv_rect, block_color);
            }
            if negative.y {
                builder.push_down(cur, uv_rect, block_color);
            }
            if negative.z {
                builder.push_north(cur, uv_rect, block_color);
            }
        }

        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, builder.vertices)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, builder.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, builder.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, builder.vert_colors)
        .with_inserted_indices(Indices::U32(builder.indices))
    }
}

impl From<Chunk> for ChunkData {
    fn from(value: Chunk) -> Self {
        ChunkData {
            loc: value.loc,
            blocks: SparseArray::from(value.blocks),
            chunk_size: value.chunk_size,
        }
    }
}

#[derive(Deserialize, Serialize)]
struct ChunkData {
    pub loc: ChunkPosition,
    pub blocks: SparseArray<u16>,
    chunk_size: ChunkSize,
}

impl From<ChunkData> for Chunk {
    fn from(value: ChunkData) -> Self {
        Chunk {
            loc: value.loc,
            blocks: SparseArray::from(value.blocks),
            chunk_size: value.chunk_size,
        }
    }
}

#[derive(Component)]
pub struct ChunkComponent(pub Handle<Chunk>);
