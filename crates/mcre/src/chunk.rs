use bevy::{
    asset::RenderAssetUsages,
    color::palettes::css::{GREEN, WHITE},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use mcre_core::{Block, BlockState};

use crate::textures::BlockTextures;

pub const CHUNK_SIZE: usize = 4;

#[derive(Component)]
pub struct Chunk {
    pub loc: UVec3,
    // TODO: Consider sparse chunk?
    pub blocks: [BlockState; CHUNK_SIZE.pow(3)],
}

impl Chunk {
    pub fn filled<B: Into<BlockState>>(loc: UVec3, block: B) -> Self {
        Chunk {
            loc,
            blocks: [block.into(); CHUNK_SIZE.pow(3)],
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
            alpha_mode: AlphaMode::Mask(0.5),
            reflectance: 0.0,
            // unlit: true,
            ..default()
        });

        let scale = Vec3::ONE;
        let transform = Transform::from_xyz(
            self.loc.x as f32 * CHUNK_SIZE as f32,
            self.loc.y as f32 * CHUNK_SIZE as f32,
            self.loc.z as f32 * CHUNK_SIZE as f32,
        )
        .with_scale(scale);
        let mesh = meshes.add(self.generate_mesh(textures));
        (self, transform, Mesh3d(mesh), MeshMaterial3d(mat))
    }

    pub fn set_block<B: Into<BlockState>>(&mut self, pos: UVec3, new_block: B) {
        if let Some(block) = self.get_mut(pos) {
            *block = new_block.into()
        }
    }

    pub fn get(&self, pos: UVec3) -> Option<&BlockState> {
        self.blocks.get(
            pos.x as usize * CHUNK_SIZE * CHUNK_SIZE + pos.y as usize * CHUNK_SIZE + pos.z as usize,
        )
    }

    pub fn get_mut(&mut self, pos: UVec3) -> Option<&mut BlockState> {
        self.blocks.get_mut(
            pos.x as usize * CHUNK_SIZE * CHUNK_SIZE + pos.y as usize * CHUNK_SIZE + pos.z as usize,
        )
    }

    fn generate_mesh(&self, textures: &BlockTextures) -> Mesh {
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
        fn check_occude(block: &BlockState) -> bool {
            block.is_air() || !block.can_occlude()
        }

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
                    if block.is_air() {
                        continue;
                    }
                    let Some(uv_rect) = textures.get_uv_rect(*block) else {
                        continue;
                    };
                    //TODO: Fix to use known data about block states
                    let block_color = match block.block() {
                        Block::OAK_LEAVES => GREEN,
                        _ => WHITE,
                    };
                    if z + 1 >= CHUNK_SIZE
                        || self.get(cur.with_z(cur.z + 1)).is_none_or(check_occude)
                    {
                        builder.push_south(cur, uv_rect, block_color);
                    }
                    if z < 1 || self.get(cur.with_z(cur.z - 1)).is_none_or(check_occude) {
                        builder.push_north(cur, uv_rect, block_color);
                    }
                    if x + 1 >= CHUNK_SIZE
                        || self.get(cur.with_x(cur.x + 1)).is_none_or(check_occude)
                    {
                        builder.push_east(cur, uv_rect, block_color);
                    }
                    if x < 1 || self.get(cur.with_x(cur.x - 1)).is_none_or(check_occude) {
                        builder.push_west(cur, uv_rect, block_color);
                    }
                    if y + 1 >= CHUNK_SIZE
                        || self.get(cur.with_y(cur.y + 1)).is_none_or(check_occude)
                    {
                        builder.push_up(cur, uv_rect, block_color);
                    }
                    if y < 1 || self.get(cur.with_y(cur.y - 1)).is_none_or(check_occude) {
                        builder.push_down(cur, uv_rect, block_color);
                    }
                }
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
