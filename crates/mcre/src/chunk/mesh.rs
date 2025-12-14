use bevy::{
    asset::RenderAssetUsages,
    color::palettes::css::{GREEN, WHITE},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use mcre_core::{Block, BlockState};

use crate::{
    chunk::{Chunk, math::pos::BlockPosition},
    textures::BlockTextures,
};

pub struct ChunkMeshBuilder<'a> {
    chunk: &'a Chunk,
}

impl<'a> ChunkMeshBuilder<'a> {
    pub fn new(chunk: &'a Chunk) -> Self {
        ChunkMeshBuilder { chunk }
    }

    pub fn update_mesh(&self, mesh: &mut Mesh, textures: &BlockTextures) {
        let mut builder = MeshBuilder::default();
        self.update_mesh_attributes(&mut builder, textures);
        //TODO Optimize in place?
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, builder.vertices);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, builder.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, builder.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, builder.vert_colors);
        mesh.insert_indices(Indices::U32(builder.indices));
    }

    fn cull_faces(&self, pos: BlockPosition) -> (BVec3, BVec3) {
        fn check_occude(block: BlockState) -> bool {
            // If it's an air block or if the block does NOT Occlude
            block.is_air() || !block.can_occlude()
        }

        let bounds = (pos + 1).in_bounds(*self.chunk.size());
        let positive_faces = BVec3::new(
            !bounds.x || self.chunk.get(pos.east()).is_none_or(check_occude),
            self.chunk.get(pos.up()).is_none_or(check_occude),
            !bounds.y || self.chunk.get(pos.south()).is_none_or(check_occude),
        );

        let negative_faces = BVec3::new(
            pos.x < 1 || self.chunk.get(pos.west()).is_none_or(check_occude),
            self.chunk.get(pos.down()).is_none_or(check_occude),
            pos.z < 1 || self.chunk.get(pos.north()).is_none_or(check_occude),
        );
        (positive_faces, negative_faces)
    }

    fn update_mesh_attributes(&self, builder: &mut MeshBuilder, textures: &BlockTextures) {
        let chunk_size = self.chunk.size();
        for (i, block) in self.chunk.iter() {
            if block.is_air() {
                continue;
            }
            let Some(uv_rect) = textures.get_uv_rect(*block) else {
                continue;
            };
            let cur = BlockPosition::from_index(i, *chunk_size);
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
    }

    pub fn build(self, textures: &BlockTextures) -> Mesh {
        let mut builder = MeshBuilder::default();
        self.update_mesh_attributes(&mut builder, textures);

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

#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    vert_colors: Vec<[f32; 4]>,
}

// -Z is North, +Z is South
// -X is West, +X is East
impl MeshBuilder {
    fn push_north(&mut self, cur: BlockPosition, uv: Rect, face_color: Srgba) {
        self.push_indices();
        self.push_face_color(face_color);
        let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
        let normal = [0., 0., -1.];
        self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
        self.push([x + 0., y + 1., z + 0.], normal, [uv.max.x, uv.min.y]);
        self.push([x + 0., y + 0., z + 0.], normal, [uv.max.x, uv.max.y]);
        self.push([x + 1., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
    }

    fn push_east(&mut self, cur: BlockPosition, uv: Rect, face_color: Srgba) {
        self.push_indices();
        self.push_face_color(face_color);
        let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
        let normal = [-1., 0., 0.];
        self.push([x + 1., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
        self.push([x + 1., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
        self.push([x + 1., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
        self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
    }

    fn push_south(&mut self, cur: BlockPosition, uv: Rect, face_color: Srgba) {
        self.push_indices();
        self.push_face_color(face_color);
        let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
        let normal = [0., 0., 1.];
        self.push([x + 0., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
        self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
        self.push([x + 1., y + 1., z + 1.], normal, [uv.min.x, uv.min.y]);
        self.push([x + 1., y + 0., z + 1.], normal, [uv.min.x, uv.max.y]);
    }

    fn push_west(&mut self, cur: BlockPosition, uv: Rect, face_color: Srgba) {
        self.push_indices();
        self.push_face_color(face_color);
        let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
        let normal = [1., 0., 0.];
        self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
        self.push([x + 0., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
        self.push([x + 0., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
        self.push([x + 0., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
    }

    fn push_up(&mut self, cur: BlockPosition, uv: Rect, face_color: Srgba) {
        self.push_indices();
        self.push_face_color(face_color);
        let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
        let normal = [0., 1., 0.];
        self.push([x + 0., y + 1., z + 0.], normal, [uv.min.x, uv.max.y]);
        self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
        self.push([x + 1., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
        self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.max.y]);
    }

    fn push_down(&mut self, cur: BlockPosition, uv: Rect, face_color: Srgba) {
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
