use core::array;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use mcre_core::{Axis, Direction, Vec3f, Vec4f};
use serde::{Deserialize, Deserializer};

use crate::{BlockModelId, FxHashMap, RefOr, ReferenceId, RotationDegrees, TextureId};

#[derive(Debug, Clone)]
pub struct BlockModelDefinition {
    pub gui_light: Option<GuiLight>,
    pub parent: Option<BlockModelId>,
    pub ambientocclusion: bool,
    pub elements: Vec<BlockModelElement>,
    pub textures: FxHashMap<String, BlockModelTexture>,
    pub display: FxHashMap<String, Transform>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiLight {
    Side,
    Front,
}

fn default_ambientocclusion() -> bool {
    true
}

fn default_origin() -> Vec3f {
    Vec3f::new(0.0, 0.0, 0.0)
}

fn default_scale() -> Vec3f {
    Vec3f::new(1.0, 1.0, 1.0)
}

#[derive(Debug, Clone, Deserialize)]
pub struct Transform {
    #[serde(default = "default_origin")]
    pub translation: Vec3f,
    #[serde(default = "default_origin")]
    pub rotation: Vec3f,
    #[serde(default = "default_scale")]
    pub scale: Vec3f,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockModelElement {
    pub from: Vec3f,
    pub to: Vec3f,
    pub rotation: Option<BlockModelElementRotation>,
    pub faces: FxHashMap<Direction, BlockModelFace>,
    pub shade_direction_override: Option<Direction>,
    #[serde(default)]
    pub light_emission: u8,
}

#[derive(Debug, Clone)]
pub enum BlockModelElementRotation {
    AxisAngle {
        origin: Vec3f,
        axis: Axis,
        angle: f32,
        rescale: bool,
    },
    Euler {
        origin: Vec3f,
        x: f32,
        y: f32,
        z: f32,
    },
}

impl<'de> Deserialize<'de> for BlockModelElementRotation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        #[derive(Deserialize)]
        struct RotationData {
            origin: Vec3f,
            #[serde(default)]
            rescale: bool,
            axis: Option<Axis>,
            angle: Option<f32>,
            x: Option<f32>,
            y: Option<f32>,
            z: Option<f32>,
        }

        let data = RotationData::deserialize(deserializer)?;

        if let (Some(axis), Some(angle)) = (data.axis, data.angle) {
            Ok(BlockModelElementRotation::AxisAngle {
                origin: data.origin,
                axis,
                angle,
                rescale: data.rescale,
            })
        } else if let (Some(x), Some(y), Some(z)) = (data.x, data.y, data.z) {
            Ok(BlockModelElementRotation::Euler {
                origin: data.origin,
                x,
                y,
                z,
            })
        } else {
            Err(Error::custom(
                "invalid rotation: expected either `axis`/`angle` or `x`/`y`/`z`",
            ))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockModelFace {
    pub texture: RefOr<TextureId>,
    #[serde(default)]
    pub rotation: RotationDegrees,
    pub uv: Option<Vec4f>,
    pub tintindex: Option<u8>,
    pub cullface: Option<Direction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BlockModelTextureObject {
    pub force_translucent: bool,
    pub sprite: RefOr<TextureId>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum BlockModelTexture {
    RefOr(RefOr<TextureId>),
    Object(BlockModelTextureObject),
}

impl BlockModelElementRotation {
    /// Port of vanilla `CuboidRotation.computeTransform`: rotation about the
    /// element origin (`angle` is in degrees, as authored), with rescale as
    /// `M = R` unless rescale is set and the rotation is non-identity, then
    /// `M = R·S` with per-axis factors from the transformed axis units.
    fn transform_matrix(axis: Axis, angle_degrees: f32, rescale: bool) -> [[f32; 3]; 3] {
        // libm: no float methods in `no_std` on this toolchain.
        let radians = angle_degrees * (core::f32::consts::PI / 180.0);
        let (sin, cos) = (libm::sinf(radians), libm::cosf(radians));
        let r = match axis {
            Axis::X => [[1.0, 0.0, 0.0], [0.0, cos, -sin], [0.0, sin, cos]],
            Axis::Y => [[cos, 0.0, sin], [0.0, 1.0, 0.0], [-sin, 0.0, cos]],
            Axis::Z => [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]],
        };
        if !rescale || angle_degrees == 0.0 {
            return r;
        }
        let mut s = [0.0; 3];
        for j in 0..3 {
            // libm: no float methods in `no_std` on this toolchain.
            let mut max = libm::fabsf(r[0][j]);
            let b = libm::fabsf(r[1][j]);
            if b > max {
                max = b;
            }
            let c = libm::fabsf(r[2][j]);
            if c > max {
                max = c;
            }
            s[j] = 1.0 / max;
        }
        array::from_fn(|i| array::from_fn(|j| r[i][j] * s[j]))
    }

    fn rotate_axis_angle(
        point: Vec3f,
        origin: Vec3f,
        axis: Axis,
        angle: f32,
        rescale: bool,
    ) -> Vec3f {
        let m = Self::transform_matrix(axis, angle, rescale);
        let point = point - origin;
        Vec3f::new(
            m[0][0] * point[0] + m[0][1] * point[1] + m[0][2] * point[2],
            m[1][0] * point[0] + m[1][1] * point[1] + m[1][2] * point[2],
            m[2][0] * point[0] + m[2][1] * point[1] + m[2][2] * point[2],
        ) + origin
    }

    fn rotate_euler(mut point: Vec3f, origin: Vec3f, x: f32, y: f32, z: f32) -> Vec3f {
        point = point - origin;

        let radians = x * (core::f32::consts::PI / 180.0);
        let (sin, cos) = (libm::sinf(radians), libm::cosf(radians));
        let y1 = point[1];
        let z1 = point[2];
        point[1] = y1 * cos - z1 * sin;
        point[2] = y1 * sin + z1 * cos;

        let radians = y * (core::f32::consts::PI / 180.0);
        let (sin, cos) = (libm::sinf(radians), libm::cosf(radians));
        let x1 = point[0];
        let z1 = point[2];
        point[0] = x1 * cos + z1 * sin;
        point[2] = z1 * cos - x1 * sin;

        let radians = z * (core::f32::consts::PI / 180.0);
        let (sin, cos) = (libm::sinf(radians), libm::cosf(radians));
        let x1 = point[0];
        let y1 = point[1];
        point[0] = x1 * cos - y1 * sin;
        point[1] = x1 * sin + y1 * cos;

        point + origin
    }

    pub fn apply_on_point(&self, point: Vec3f) -> Vec3f {
        match self {
            BlockModelElementRotation::AxisAngle {
                origin,
                axis,
                angle,
                rescale,
            } => Self::rotate_axis_angle(point, *origin, *axis, *angle, *rescale),
            BlockModelElementRotation::Euler { origin, x, y, z } => {
                Self::rotate_euler(point, *origin, *x, *y, *z)
            }
        }
    }

    pub fn apply_on_quad(&self, quad: [Vec3f; 4]) -> [Vec3f; 4] {
        array::from_fn(|i| self.apply_on_point(quad[i]))
    }
}

fn build_quad(min: Vec3f, max: Vec3f, dir: Direction) -> [Vec3f; 4] {
    let (x0, y0, z0) = (min[0], min[1], min[2]);
    let (x1, y1, z1) = (max[0], max[1], max[2]);
    let v = Vec3f::new;
    match dir {
        Direction::Down => [v(x0, y0, z1), v(x0, y0, z0), v(x1, y0, z0), v(x1, y0, z1)],
        Direction::Up => [v(x0, y1, z0), v(x0, y1, z1), v(x1, y1, z1), v(x1, y1, z0)],
        Direction::North => [v(x1, y1, z0), v(x1, y0, z0), v(x0, y0, z0), v(x0, y1, z0)],
        Direction::South => [v(x0, y1, z1), v(x0, y0, z1), v(x1, y0, z1), v(x1, y1, z1)],
        Direction::West => [v(x0, y1, z0), v(x0, y0, z0), v(x0, y0, z1), v(x0, y1, z1)],
        Direction::East => [v(x1, y1, z1), v(x1, y0, z1), v(x1, y0, z0), v(x1, y1, z0)],
    }
}

fn default_face_uv(min: Vec3f, max: Vec3f, dir: Direction) -> Vec4f {
    match dir {
        Direction::Down => Vec4f::new(min[0], 16.0 - max[2], max[0], 16.0 - min[2]),
        Direction::Up => Vec4f::new(min[0], min[2], max[0], max[2]),
        Direction::North => Vec4f::new(16.0 - max[0], 16.0 - max[1], 16.0 - min[0], 16.0 - min[1]),
        Direction::South => Vec4f::new(min[0], 16.0 - max[1], max[0], 16.0 - min[1]),
        Direction::West => Vec4f::new(min[2], 16.0 - max[1], max[2], 16.0 - min[1]),
        Direction::East => Vec4f::new(16.0 - max[2], 16.0 - max[1], 16.0 - min[2], 16.0 - min[1]),
    }
}

fn quad_normal(vertices: [Vec3f; 4]) -> Vec3f {
    let a = vertices[1] - vertices[0];
    let b = vertices[2] - vertices[0];
    Vec3f::new(
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )
}

fn dir_unit(dir: Direction) -> Vec3f {
    match dir {
        Direction::Down => Vec3f::new(0.0, -1.0, 0.0),
        Direction::Up => Vec3f::new(0.0, 1.0, 0.0),
        Direction::North => Vec3f::new(0.0, 0.0, -1.0),
        Direction::South => Vec3f::new(0.0, 0.0, 1.0),
        Direction::West => Vec3f::new(-1.0, 0.0, 0.0),
        Direction::East => Vec3f::new(1.0, 0.0, 0.0),
    }
}

fn vanilla_closest_direction(normal: Vec3f) -> Direction {
    if !(normal[0].is_finite() && normal[1].is_finite() && normal[2].is_finite()) {
        return Direction::Up;
    }

    let mut best = Direction::Up;
    let mut closest = 0.0f32;
    for dir in Direction::ALL {
        let unit = dir_unit(dir);
        let product = normal[0] * unit[0] + normal[1] * unit[1] + normal[2] * unit[2];
        if product >= 0.0 && product > closest {
            closest = product;
            best = dir;
        }
    }
    best
}

pub struct BakedQuad {
    pub vertices: [Vec3f; 4],
    pub uvs: [[f32; 2]; 4],
    pub direction: Direction,
    pub texture: TextureId,
    pub cullface: Option<Direction>,
    pub tintindex: Option<u8>,
    pub shade_direction_override: Option<Direction>,
    pub light_emission: u8,
}

pub fn cardinal_lighting(direction: Direction) -> f32 {
    match direction {
        Direction::Down => 0.5,
        Direction::Up => 1.0,
        Direction::North => 0.8,
        Direction::South => 0.8,
        Direction::West => 0.6,
        Direction::East => 0.6,
    }
}

impl BakedQuad {
    pub fn shade_factor(&self) -> f32 {
        cardinal_lighting(self.shade_direction_override.unwrap_or(self.direction))
    }
}

#[derive(Debug)]
pub enum ModelBakeError {
    TextureNotFound(String),
    ParentNotFound(String),
}

impl BlockModelDefinition {
    fn _collect_texture_defs<F>(
        &self,
        parent_resolver: &F,
        defs: &mut FxHashMap<ReferenceId, RefOr<TextureId>>,
    ) -> Result<(), ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<BlockModelDefinition>,
    {
        if let Some(parent_id) = &self.parent {
            let parent = parent_resolver(parent_id)
                .ok_or_else(|| ModelBakeError::ParentNotFound(parent_id.to_string()))?;
            parent._collect_texture_defs(parent_resolver, defs)?;
        }

        for (name, texture) in &self.textures {
            let value = match texture {
                BlockModelTexture::RefOr(value) => value.clone(),
                BlockModelTexture::Object(obj) => obj.sprite.clone(),
            };
            defs.insert(ReferenceId::new(name.clone()), value);
        }

        Ok(())
    }

    fn _resolve_elements<F>(
        &self,
        parent_resolver: &F,
    ) -> Result<Vec<BlockModelElement>, ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<BlockModelDefinition>,
    {
        if !self.elements.is_empty() {
            return Ok(self.elements.clone());
        }
        match &self.parent {
            Some(parent_id) => {
                let parent = parent_resolver(parent_id)
                    .ok_or_else(|| ModelBakeError::ParentNotFound(parent_id.to_string()))?;
                parent._resolve_elements(parent_resolver)
            }
            None => Ok(Vec::new()),
        }
    }

    pub fn build_texture_map<F>(
        &self,
        parent_resolver: F,
    ) -> Result<FxHashMap<ReferenceId, TextureId>, ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<BlockModelDefinition>,
    {
        let mut defs = FxHashMap::default();
        self._collect_texture_defs(&parent_resolver, &mut defs)?;

        let mut texture_map = FxHashMap::default();
        for (name, value) in &defs {
            let mut current = value.clone();
            let mut hops = 0;
            while hops <= 16 {
                match current {
                    RefOr::Value(texture) => {
                        texture_map.insert(name.clone(), texture);
                        break;
                    }
                    RefOr::Ref(next) => match defs.get(&next) {
                        Some(next_value) => current = next_value.clone(),
                        None => break,
                    },
                }
                hops += 1;
            }
        }

        Ok(texture_map)
    }

    pub fn bake<F, E>(&self, parent_resolver: F) -> Result<(), ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<BlockModelDefinition>,
    {
        let _ = self.bake_quads(parent_resolver)?;
        Ok(())
    }

    /// Bake the model into a list of quads with resolved texture references.
    pub fn bake_quads<F>(&self, parent_resolver: F) -> Result<Vec<BakedQuad>, ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<BlockModelDefinition>,
    {
        let elements = self._resolve_elements(&parent_resolver)?;
        let texture_map = self.build_texture_map(parent_resolver)?;

        let mut quads = Vec::new();
        for element in &elements {
            let min = Vec3f::new(
                element.from[0].min(element.to[0]),
                element.from[1].min(element.to[1]),
                element.from[2].min(element.to[2]),
            );
            let max = Vec3f::new(
                element.from[0].max(element.to[0]),
                element.from[1].max(element.to[1]),
                element.from[2].max(element.to[2]),
            );
            for direction in Direction::ALL {
                if let Some(face) = element.faces.get(&direction) {
                    let quad_vertices = build_quad(min, max, direction);
                    let rotated_quad_vertices = if let Some(rotation) = &element.rotation {
                        rotation.apply_on_quad(quad_vertices)
                    } else {
                        quad_vertices
                    };

                    let uv = face
                        .uv
                        .unwrap_or_else(|| default_face_uv(min, max, direction));
                    let [u1, v1, u2, v2] = *uv;
                    let corners = [[u1, v1], [u1, v2], [u2, v2], [u2, v1]];
                    let shift = face.rotation.quadrant();
                    let uvs = array::from_fn(|i| corners[(i + shift) % 4]);

                    let texture = match &face.texture {
                        RefOr::Ref(id) => texture_map.get(id).cloned().unwrap_or_else(|| {
                            TextureId::Block(crate::BlockTextureId::new(
                                "minecraft".to_string(),
                                "missingno".to_string(),
                            ))
                        }),
                        RefOr::Value(id) => id.clone(),
                    };

                    quads.push(BakedQuad {
                        vertices: rotated_quad_vertices,
                        uvs,
                        direction: vanilla_closest_direction(quad_normal(rotated_quad_vertices)),
                        texture,
                        tintindex: face.tintindex,
                        cullface: face.cullface,
                        shade_direction_override: element.shade_direction_override,
                        light_emission: element.light_emission,
                    });
                }
            }
        }

        Ok(quads)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        path::PathBuf,
    };

    use crate::{FxHashMap, block::BlockModelDefinition};

    #[tokio::test]
    async fn test_parse_block_model_definition() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let manifest_dir = PathBuf::from(manifest_dir);
        let target_mc_version = std::fs::read_to_string(manifest_dir.join("../../mc-version"))
            .unwrap()
            .trim()
            .to_string();
        let root_dir = manifest_dir
            .join("../../target/downloads")
            .join(&target_mc_version)
            .join("assets/minecraft/models/block");

        let mut total = 0;
        let mut passed = 0;
        let mut failed = Vec::new();

        let mut block_state_definitions = FxHashMap::default();

        for entry in fs::read_dir(&root_dir).unwrap() {
            total += 1;
            let entry = entry.unwrap();
            let path = entry.path();
            let file = File::open(&path).unwrap();

            let file_name = path.file_name().unwrap().to_str().unwrap();
            let name = file_name.strip_suffix(".json").unwrap().to_string();

            let result: Result<BlockModelDefinition, _> = serde_json::from_reader(file);

            match result {
                Ok(block_state_definition) => {
                    passed += 1;
                    block_state_definitions.insert(name, block_state_definition);
                }
                Err(err) => {
                    failed.push((name, err));
                }
            }
        }

        if !failed.is_empty() {
            eprintln!("Failed to parse:");
            for (name, err) in failed {
                eprintln!("- {}: {}", name, err);
            }
        }

        assert_eq!(passed, total);
    }

    use mcre_core::{Direction, Vec3f};
    use mcre_world::BlockState;

    use crate::{BlockStateDefinition, blockstates::ModelVariant, id::BlockModelId};

    fn assets_dir() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let target_mc_version = std::fs::read_to_string(manifest_dir.join("../../mc-version"))
            .unwrap()
            .trim()
            .to_string();
        manifest_dir
            .join("../../target/downloads")
            .join(target_mc_version)
            .join("assets/minecraft")
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    fn dir_unit(dir: Direction) -> [f32; 3] {
        match dir {
            Direction::Down => [0.0, -1.0, 0.0],
            Direction::Up => [0.0, 1.0, 0.0],
            Direction::North => [0.0, 0.0, -1.0],
            Direction::South => [0.0, 0.0, 1.0],
            Direction::West => [-1.0, 0.0, 0.0],
            Direction::East => [1.0, 0.0, 0.0],
        }
    }

    #[test]
    fn test_bake_default_uv_face_rotation_and_winding() {
        let json = r##"{
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 8, 16],
                    "faces": {
                        "north": { "texture": "#t" },
                        "south": { "texture": "#t", "rotation": 90 }
                    }
                }
            ],
            "textures": { "t": "minecraft:block/stone" }
        }"##;

        let def: BlockModelDefinition = serde_json::from_str(json).unwrap();
        let quads = def.bake_quads(|_| None).unwrap();
        assert_eq!(quads.len(), 2);

        let north = quads
            .iter()
            .find(|q| q.direction == Direction::North)
            .unwrap();
        assert_eq!(
            north.uvs,
            [[0.0, 8.0], [0.0, 16.0], [16.0, 16.0], [16.0, 8.0]]
        );

        let south = quads
            .iter()
            .find(|q| q.direction == Direction::South)
            .unwrap();
        assert_eq!(
            south.uvs,
            [[0.0, 16.0], [16.0, 16.0], [16.0, 8.0], [0.0, 8.0]]
        );

        for quad in &quads {
            let v: [[f32; 3]; 4] = quad.vertices.map(|v| [v[0], v[1], v[2]]);
            let normal = cross(sub(v[1], v[0]), sub(v[2], v[0]));
            let dir = dir_unit(quad.direction);
            let dot = normal[0] * dir[0] + normal[1] * dir[1] + normal[2] * dir[2];
            assert!(
                dot > 0.0,
                "quad facing {:?} has inward winding (dot = {dot})",
                quad.direction
            );
        }
    }

    #[test]
    fn test_all_cube_faces_wind_outward() {
        let json = r##"{
            "elements": [
                {
                    "from": [0, 0, 0],
                    "to": [16, 16, 16],
                    "faces": {
                        "down": { "texture": "#t" },
                        "up": { "texture": "#t" },
                        "north": { "texture": "#t" },
                        "south": { "texture": "#t" },
                        "west": { "texture": "#t" },
                        "east": { "texture": "#t" }
                    }
                }
            ],
            "textures": { "t": "minecraft:block/stone" }
        }"##;

        let def: BlockModelDefinition = serde_json::from_str(json).unwrap();
        let quads = def.bake_quads(|_| None).unwrap();
        assert_eq!(quads.len(), 6);

        for dir in Direction::ALL {
            let quad = quads
                .iter()
                .find(|q| q.direction == dir)
                .unwrap_or_else(|| panic!("missing {dir:?} face"));
            let v: [[f32; 3]; 4] = quad.vertices.map(|v| [v[0], v[1], v[2]]);
            let normal = cross(sub(v[1], v[0]), sub(v[2], v[0]));
            let unit = dir_unit(dir);
            let dot = normal[0] * unit[0] + normal[1] * unit[1] + normal[2] * unit[2];
            assert!(
                dot > 0.0,
                "quad facing {dir:?} has inward winding (dot = {dot})"
            );
        }
    }

    #[test]
    fn test_cross_element_rotation_rescale() {
        // cross.json rotates axis-aligned planes 45 degrees about Y with
        // rescale: vanilla `CuboidRotation` enlarges them to corner-to-corner
        // diagonals instead of shrinking their projected width.
        let json = r##"{
            "elements": [
                {   "from": [ 0.8, 0, 8 ],
                    "to": [ 15.2, 16, 8 ],
                    "rotation": { "origin": [ 8, 8, 8 ], "axis": "y", "angle": 45, "rescale": true },
                    "faces": {
                        "north": { "uv": [ 0, 0, 16, 16 ], "texture": "#t" }
                    }
                }
            ],
            "textures": { "t": "minecraft:block/stone" }
        }"##;

        let def: BlockModelDefinition = serde_json::from_str(json).unwrap();
        let quads = def.bake_quads(|_| None).unwrap();
        assert_eq!(quads.len(), 1);

        let expected = [
            [15.2, 16.0, 0.8],
            [15.2, 0.0, 0.8],
            [0.8, 0.0, 15.2],
            [0.8, 16.0, 15.2],
        ];
        for (got, want) in quads[0].vertices.iter().zip(expected) {
            for a in 0..3 {
                assert!(
                    (got[a] - want[a]).abs() < 1e-3,
                    "got {got:?}, want {want:?}"
                );
            }
        }
    }

    #[test]
    fn test_variant_rotation_and_uvlock() {
        let model: BlockModelId = serde_json::from_str("\"minecraft:block/test\"").unwrap();
        let variant = ModelVariant {
            model,
            uvlock: true,
            weight: 1,
            x: crate::RotationDegrees::R0,
            y: crate::RotationDegrees::R90,
            z: crate::RotationDegrees::R0,
        };

        let north_face: [[f32; 3]; 4] = [
            [16.0, 16.0, 0.0],
            [16.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 16.0, 0.0],
        ];
        let mut quads = vec![crate::BakedQuad {
            vertices: north_face.map(|v| Vec3f::new(v[0], v[1], v[2])),
            uvs: [[0.0, 0.0], [16.0, 0.0], [16.0, 16.0], [0.0, 16.0]],
            direction: Direction::North,
            texture: crate::TextureId::Block(crate::BlockTextureId::new(
                "minecraft".to_string(),
                "stone".to_string(),
            )),
            cullface: Some(Direction::North),
            tintindex: None,
            shade_direction_override: None,
            light_emission: 0,
        }];

        variant.transform_quads(&mut quads);

        for vertex in &quads[0].vertices {
            assert!(
                (vertex[0] - 16.0).abs() < 1e-4,
                "expected east plane, got {:?}",
                vertex
            );
        }
        assert_eq!(quads[0].direction, Direction::East);
        assert_eq!(quads[0].cullface, Some(Direction::East));
        assert_eq!(quads[0].shade_factor(), 0.6);
    }

    #[test]
    fn test_real_furnace_variant_rotation() {
        let assets = assets_dir();

        let blockstate: BlockStateDefinition =
            serde_json::from_reader(File::open(assets.join("blockstates/furnace.json")).unwrap())
                .unwrap();

        let state = BlockState::all()
            .find(|s| s.block().name() == "furnace" && s.facing() == Direction::East)
            .expect("furnace east state");

        let resolution = blockstate.resolve(state).unwrap();
        let crate::BlockModelResolution::Unified(variants) = resolution else {
            panic!("expected unified resolution");
        };
        assert_eq!(variants.len(), 1);
        assert!(matches!(variants[0].y, crate::RotationDegrees::R90));
        assert!(!variants[0].uvlock);

        let model: BlockModelDefinition =
            serde_json::from_reader(File::open(assets.join("models/block/furnace.json")).unwrap())
                .unwrap();

        let resolver = |id: &BlockModelId| -> Option<BlockModelDefinition> {
            let name = id.to_string().rsplit('/').next()?.to_string();
            let path = assets.join("models/block").join(format!("{name}.json"));
            serde_json::from_reader(File::open(path).ok()?).ok()
        };

        let mut quads = model.bake_quads(resolver).unwrap();
        assert!(!quads.is_empty());

        let front_index = quads
            .iter()
            .position(|q| q.texture.to_string().contains("furnace_front"))
            .expect("front face quad");
        assert_eq!(quads[front_index].direction, Direction::North);
        for vertex in &quads[front_index].vertices {
            assert!(
                (vertex[2]).abs() < 1e-4,
                "front should start on north plane"
            );
        }

        variants[0].transform_quads(&mut quads);

        for vertex in &quads[front_index].vertices {
            assert!(
                (vertex[0] - 16.0).abs() < 1e-4,
                "front should move to east plane, got {:?}",
                vertex
            );
        }

        assert_eq!(quads[front_index].direction, Direction::East);
        assert_eq!(quads[front_index].cullface, Some(Direction::East));
        assert_eq!(quads[front_index].shade_factor(), 0.6);
    }

    #[test]
    fn test_grass_side_uv_orientation() {
        let assets = assets_dir();
        let model: BlockModelDefinition = serde_json::from_reader(
            File::open(assets.join("models/block/grass_block.json")).unwrap(),
        )
        .unwrap();

        let resolver = |id: &BlockModelId| -> Option<BlockModelDefinition> {
            let name = id.to_string().rsplit('/').next()?.to_string();
            let path = assets.join("models/block").join(format!("{name}.json"));
            serde_json::from_reader(File::open(path).ok()?).ok()
        };

        let quads = model.bake_quads(resolver).unwrap();

        let south = quads
            .iter()
            .find(|q| q.direction == Direction::South && q.tintindex.is_none())
            .expect("untinted south side face");
        assert_eq!(south.vertices[0], Vec3f::new(0.0, 16.0, 16.0));
        assert_eq!(
            south.uvs,
            [[0.0, 0.0], [0.0, 16.0], [16.0, 16.0], [16.0, 0.0]]
        );
        assert!(south.texture.to_string().ends_with("grass_block_side"));

        let up = quads
            .iter()
            .find(|q| q.direction == Direction::Up)
            .expect("top face");
        assert_eq!(up.tintindex, Some(0));
        assert_eq!(up.uvs, [[0.0, 0.0], [0.0, 16.0], [16.0, 16.0], [16.0, 0.0]]);
    }

    #[test]
    fn test_cross_model_shade_override() {
        let assets = assets_dir();
        let model: BlockModelDefinition =
            serde_json::from_reader(File::open(assets.join("models/block/cross.json")).unwrap())
                .unwrap();

        let quads = model.bake_quads(|_| None).unwrap();
        assert_eq!(quads.len(), 4);

        for quad in &quads {
            assert_eq!(quad.shade_direction_override, Some(Direction::Up));
            assert_eq!(quad.shade_factor(), 1.0);
            assert!(matches!(
                quad.direction,
                Direction::North | Direction::South | Direction::West | Direction::East
            ));
        }
    }

    #[test]
    fn test_grass_variant_rotation_permutes_faces_rigidly() {
        use crate::blockstates::ModelVariant;
        let assets = assets_dir();
        let resolver = |id: &BlockModelId| -> Option<BlockModelDefinition> {
            let name = id.to_string().rsplit('/').next()?.to_string();
            let path = assets.join("models/block").join(format!("{name}.json"));
            serde_json::from_reader(File::open(path).ok()?).ok()
        };
        let model: BlockModelDefinition = serde_json::from_reader(
            File::open(assets.join("models/block/grass_block.json")).unwrap(),
        )
        .unwrap();
        let mut quads = model.bake_quads(resolver).unwrap();
        let variant = ModelVariant {
            model: serde_json::from_str("\"minecraft:block/grass_block\"").unwrap(),
            uvlock: false,
            weight: 1,
            x: crate::RotationDegrees::R0,
            y: crate::RotationDegrees::R90,
            z: crate::RotationDegrees::R0,
        };
        variant.transform_quads(&mut quads);
        assert_eq!(quads.len(), 10);

        // Authored-north faces rotate onto the east plane with east shade.
        for (texture, tint) in [
            ("grass_block_side", None),
            ("grass_block_side_overlay", Some(0)),
        ] {
            let quad = quads
                .iter()
                .find(|q| {
                    q.texture.to_string().ends_with(texture) && q.direction == Direction::East
                })
                .unwrap_or_else(|| panic!("missing rotated {texture}"));
            assert_eq!(quad.cullface, Some(Direction::East));
            assert_eq!(quad.shade_factor(), 0.6);
            assert_eq!(quad.tintindex, tint);
            for v in &quad.vertices {
                assert!((v[0] - 16.0).abs() < 1e-3, "not rigid: {v:?}");
            }
        }

        let top = quads
            .iter()
            .find(|q| q.direction == Direction::Up)
            .expect("top face");
        assert_eq!(top.shade_factor(), 1.0);
        assert_eq!(top.tintindex, Some(0));
    }

    #[test]
    fn test_texture_chain_resolution() {
        let assets = assets_dir();
        let model: BlockModelDefinition = serde_json::from_reader(
            File::open(assets.join("models/block/blast_furnace.json")).unwrap(),
        )
        .unwrap();

        let resolver = |id: &BlockModelId| -> Option<BlockModelDefinition> {
            let name = id.to_string().rsplit('/').next()?.to_string();
            let path = assets.join("models/block").join(format!("{name}.json"));
            serde_json::from_reader(File::open(path).ok()?).ok()
        };

        let quads = model.bake_quads(resolver).unwrap();
        assert!(!quads.is_empty());

        let down = quads
            .iter()
            .find(|q| q.direction == Direction::Down)
            .expect("down face");
        assert!(
            down.texture.to_string().ends_with("blast_furnace_top"),
            "chained ref down -> bottom -> top resolved to {}",
            down.texture
        );

        assert!(
            quads
                .iter()
                .any(|q| q.texture.to_string().contains("blast_furnace_front")),
            "child texture override survives parent chain"
        );
    }
}

mod de_impl {
    use core::fmt;

    use alloc::{string::String, vec::Vec};
    use serde::{Deserialize, Deserializer, de};
    use serde_json::Value;

    use crate::{
        BlockModelId, FxHashMap,
        block::{
            BlockModelDefinition, BlockModelElement, BlockModelTexture, GuiLight, Transform,
            default_ambientocclusion,
        },
    };

    // The required Deserialize trait implementation
    impl<'de> Deserialize<'de> for BlockModelDefinition {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            // Field names used by the Visitor to identify keys
            const FIELD_NAMES: &[&str] = &[
                "gui_light",
                "parent",
                "ambientocclusion",
                "elements",
                "textures",
                "display",
            ];

            // The Visitor struct is used to hold the custom deserialization logic.
            struct BlockModelDefinitionVisitor;

            impl<'de> de::Visitor<'de> for BlockModelDefinitionVisitor {
                type Value = BlockModelDefinition;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("struct BlockModelDefinition")
                }

                // This is the main method for deserializing a JSON object into the struct.
                fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                where
                    A: de::MapAccess<'de>,
                {
                    // Initialize fields, using Option<T> to track if they were present in the input.
                    let mut gui_light: Option<Option<GuiLight>> = None;
                    let mut parent: Option<Option<BlockModelId>> = None;
                    let mut ambientocclusion: Option<bool> = None;
                    let mut elements: Option<Vec<BlockModelElement>> = None;
                    let mut textures: Option<FxHashMap<String, BlockModelTexture>> = None;
                    let mut display: Option<FxHashMap<String, Transform>> = None;

                    // Loop over key-value pairs in the input map
                    while let Some(key) = map.next_key::<String>()? {
                        match key.as_str() {
                            "gui_light" => {
                                if gui_light.is_some() {
                                    return Err(de::Error::duplicate_field("gui_light"));
                                }
                                gui_light = Some(map.next_value()?);
                            }
                            "parent" => {
                                if parent.is_some() {
                                    return Err(de::Error::duplicate_field("parent"));
                                }
                                parent = Some(map.next_value()?);
                            }
                            "ambientocclusion" => {
                                if ambientocclusion.is_some() {
                                    return Err(de::Error::duplicate_field("ambientocclusion"));
                                }
                                ambientocclusion = Some(map.next_value()?);
                            }
                            "elements" => {
                                if elements.is_some() {
                                    return Err(de::Error::duplicate_field("elements"));
                                }
                                elements = Some(map.next_value()?);
                            }
                            "textures" => {
                                if textures.is_some() {
                                    return Err(de::Error::duplicate_field("textures"));
                                }

                                let raw_map: FxHashMap<String, Value> = map.next_value()?;

                                let mut filtered_map = FxHashMap::default();

                                for (key, value) in raw_map {
                                    if let Value::String(s) = &value
                                        && s == "minecraft:missingno"
                                    {
                                        continue;
                                    }

                                    let texture: BlockModelTexture =
                                        serde::Deserialize::deserialize(value)
                                            .map_err(de::Error::custom)?;

                                    filtered_map.insert(key, texture);
                                }

                                textures = Some(filtered_map);
                            }
                            "display" => {
                                if display.is_some() {
                                    return Err(de::Error::duplicate_field("display"));
                                }
                                display = Some(map.next_value()?);
                            }
                            _ => {
                                // Ignore unknown fields, as derived implementations do
                                let _: de::IgnoredAny = map.next_value()?;
                            }
                        }
                    }

                    // Apply default values for missing fields
                    let gui_light = gui_light.flatten(); // Flatten Option<Option<T>> to Option<T>
                    let parent = parent.flatten();

                    let ambientocclusion =
                        ambientocclusion.unwrap_or_else(default_ambientocclusion);
                    let elements = elements.unwrap_or_default();
                    let textures = textures.unwrap_or_default(); // Uses HashMap::default, which is empty {}
                    let display = display.unwrap_or_default();

                    Ok(BlockModelDefinition {
                        gui_light,
                        parent,
                        ambientocclusion,
                        elements,
                        textures,
                        display,
                    })
                }
            }

            // This is the line that makes the implementation look like a derived one.
            deserializer.deserialize_struct(
                "BlockModelDefinition",
                FIELD_NAMES,
                BlockModelDefinitionVisitor,
            )
        }
    }
}
