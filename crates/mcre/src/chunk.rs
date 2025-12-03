use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use mcre_core::BlockId;

use crate::textures::BlockTextures;

pub const CHUNK_SIZE: usize = 16;

#[derive(Component)]
pub struct Chunk {
    pub loc: Vec3,
    // TODO: Consider sparse chunk?
    pub blocks: [BlockId; CHUNK_SIZE.pow(3)],
}

impl Chunk {
    pub fn filled(loc: Vec3, block: BlockId) -> Self {
        Chunk {
            loc,
            blocks: [block; CHUNK_SIZE.pow(3)],
        }
    }

    pub fn into_bundle(
        self,
        textures: &BlockTextures,
        meshes: &mut ResMut<Assets<Mesh>>,
        materials: &mut ResMut<Assets<StandardMaterial>>,
    ) -> impl Bundle {
        let mat = materials.add(StandardMaterial {
            base_color_texture: textures.texture().cloned(),
            reflectance: 0.0,
            // unlit: true,
            ..default()
        });

        let scale = Vec3::ONE;
        let transform = Transform::from_xyz(
            self.loc.x * CHUNK_SIZE as f32,
            self.loc.y * CHUNK_SIZE as f32,
            self.loc.z * CHUNK_SIZE as f32,
        )
        .with_scale(scale);
        let mesh = meshes.add(self.generate_mesh(textures));
        (self, transform, Mesh3d(mesh), MeshMaterial3d(mat))
    }

    pub fn set_block(&mut self, pos: UVec3, new_block: BlockId) {
        if let Some(block) = self.get_mut(pos) {
            *block = new_block
        }
    }

    pub fn get(&self, pos: UVec3) -> Option<&BlockId> {
        self.blocks.get(
            pos.x as usize * CHUNK_SIZE * CHUNK_SIZE + pos.y as usize * CHUNK_SIZE + pos.z as usize,
        )
    }

    pub fn get_mut(&mut self, pos: UVec3) -> Option<&mut BlockId> {
        self.blocks.get_mut(
            pos.x as usize * CHUNK_SIZE * CHUNK_SIZE + pos.y as usize * CHUNK_SIZE + pos.z as usize,
        )
    }

    fn generate_mesh(&self, textures: &BlockTextures) -> Mesh {
        #[derive(Default)]
        struct VerticesBuilder {
            verticies: Vec<[f32; 3]>,
            normals: Vec<[f32; 3]>,
            uvs: Vec<[f32; 2]>,
            indicies: Vec<u32>,
        }

        // -Z is North, +Z is South
        // -X is West, +X is East
        impl VerticesBuilder {
            fn push_north(&mut self, cur: UVec3, uv: Rect) {
                self.push_indicies();
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [0., 0., -1.];
                self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
                self.push([x + 0., y + 1., z + 0.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 0., y + 0., z + 0.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 1., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
            }

            fn push_east(&mut self, cur: UVec3, uv: Rect) {
                self.push_indicies();
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [-1., 0., 0.];
                self.push([x + 1., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
                self.push([x + 1., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 1., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
            }

            fn push_south(&mut self, cur: UVec3, uv: Rect) {
                self.push_indicies();
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [0., 0., 1.];
                self.push([x + 0., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 1., y + 1., z + 1.], normal, [uv.min.x, uv.min.y]);
                self.push([x + 1., y + 0., z + 1.], normal, [uv.min.x, uv.max.y]);
            }

            fn push_west(&mut self, cur: UVec3, uv: Rect) {
                self.push_indicies();
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [1., 0., 0.];
                self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 0., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 0., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
                self.push([x + 0., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
            }

            fn push_up(&mut self, cur: UVec3, uv: Rect) {
                self.push_indicies();
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [0., 1., 0.];
                self.push([x + 0., y + 1., z + 0.], normal, [uv.min.x, uv.max.y]);
                self.push([x + 1., y + 1., z + 0.], normal, [uv.min.x, uv.min.y]);
                self.push([x + 1., y + 1., z + 1.], normal, [uv.max.x, uv.min.y]);
                self.push([x + 0., y + 1., z + 1.], normal, [uv.max.x, uv.max.y]);
            }

            fn push_down(&mut self, cur: UVec3, uv: Rect) {
                self.push_indicies();
                let (x, y, z) = (cur.x as f32, cur.y as f32, cur.z as f32);
                let normal = [0., -1., 0.];
                self.push([x + 1., y + 0., z + 0.], normal, [uv.min.x, uv.min.y]);
                self.push([x + 0., y + 0., z + 0.], normal, [uv.min.x, uv.max.y]);
                self.push([x + 0., y + 0., z + 1.], normal, [uv.max.x, uv.max.y]);
                self.push([x + 1., y + 0., z + 1.], normal, [uv.max.x, uv.min.y]);
            }

            fn push_indicies(&mut self) {
                let vertex_count = self.verticies.len() as u32;

                // 0, 3, 1, 1, 3, 2, // triangles making up the top (+y) facing side.
                self.indicies.push(vertex_count);
                self.indicies.push(vertex_count + 3);
                self.indicies.push(vertex_count + 1);

                self.indicies.push(vertex_count + 1);
                self.indicies.push(vertex_count + 3);
                self.indicies.push(vertex_count + 2);
            }

            fn push(&mut self, vertex: [f32; 3], normal: [f32; 3], uv: [f32; 2]) {
                self.verticies.push(vertex);
                self.normals.push(normal);
                self.uvs.push(uv);
            }
        }
        let mut builder = VerticesBuilder::default();

        for x in 0..CHUNK_SIZE {
            for y in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    let cur = UVec3 {
                        x: x as u32,
                        y: y as u32,
                        z: z as u32,
                    };
                    let Some(block) = self.get(cur) else {
                        continue;
                    };
                    if *block == BlockId::AIR {
                        continue;
                    }
                    let Some(uv_rect) = textures.get_uv_rect(*block) else {
                        continue;
                    };
                    if z + 1 >= CHUNK_SIZE
                        || self
                            .get(cur.with_z(z as u32 + 1))
                            .is_none_or(|block| *block == BlockId::AIR)
                    {
                        builder.push_south(cur, uv_rect);
                    }
                    if z < 1
                        || self
                            .get(cur.with_z(z as u32 - 1))
                            .is_none_or(|block| *block == BlockId::AIR)
                    {
                        builder.push_north(cur, uv_rect);
                    }
                    if x + 1 >= CHUNK_SIZE
                        || self
                            .get(cur.with_x(x as u32 + 1))
                            .is_none_or(|block| *block == BlockId::AIR)
                    {
                        builder.push_east(cur, uv_rect);
                    }
                    if x < 1
                        || self
                            .get(cur.with_x(x as u32 - 1))
                            .is_none_or(|block| *block == BlockId::AIR)
                    {
                        builder.push_west(cur, uv_rect);
                    }
                    if y + 1 >= CHUNK_SIZE
                        || self
                            .get(cur.with_y(y as u32 + 1))
                            .is_none_or(|block| *block == BlockId::AIR)
                    {
                        builder.push_up(cur, uv_rect);
                    }
                    if y < 1
                        || self
                            .get(cur.with_y(y as u32 - 1))
                            .is_none_or(|block| *block == BlockId::AIR)
                    {
                        builder.push_down(cur, uv_rect);
                    }
                }
            }
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, builder.verticies)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, builder.uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, builder.normals)
        .with_inserted_indices(Indices::U32(builder.indicies))
    }
}
