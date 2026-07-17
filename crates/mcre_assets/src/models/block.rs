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
    #[serde(default = "default_shade")]
    pub shade: bool,
    #[serde(default)]
    pub light_emission: u8,
}

fn default_shade() -> bool {
    true
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
    fn rotate_axis_angle(
        point: Vec3f,
        origin: Vec3f,
        axis: Axis,
        angle: f32,
        rescale: bool,
    ) -> Vec3f {
        let mut point = point - origin;
        let (sin, cos) = angle.sin_cos();

        let scale = if rescale {
            1.0 / (cos.abs() + sin.abs())
        } else {
            1.0
        };

        match axis {
            Axis::X => {
                let y = point[1];
                let z = point[2];
                point[1] = (y * cos - z * sin) * scale;
                point[2] = (y * sin + z * cos) * scale;
            }
            Axis::Y => {
                let x = point[0];
                let z = point[2];
                point[0] = (x * cos + z * sin) * scale;
                point[2] = (z * cos - x * sin) * scale;
            }
            Axis::Z => {
                let x = point[0];
                let y = point[1];
                point[0] = (x * cos - y * sin) * scale;
                point[1] = (x * sin + y * cos) * scale;
            }
        }
        point + origin
    }

    fn rotate_euler(mut point: Vec3f, origin: Vec3f, x: f32, y: f32, z: f32) -> Vec3f {
        point = point - origin;

        let (sin, cos) = x.to_radians().sin_cos();
        let y1 = point[1];
        let z1 = point[2];
        point[1] = y1 * cos - z1 * sin;
        point[2] = y1 * sin + z1 * cos;

        let (sin, cos) = y.to_radians().sin_cos();
        let x1 = point[0];
        let z1 = point[2];
        point[0] = x1 * cos + z1 * sin;
        point[2] = z1 * cos - x1 * sin;

        let (sin, cos) = z.to_radians().sin_cos();
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
    // 1. The two axes that span the rectangle
    let [a1, a2] = dir.axis().complementary_axes();

    // 2. Get min/max for variable axes
    let range = |axis: Axis| -> (f32, f32) {
        let min = axis.select(min);
        let max = axis.select(max);
        (min, max)
    };

    let (a1_min, a1_max) = range(a1);
    let (a2_min, a2_max) = range(a2);

    // 3. Build the quad (CCW)
    fn make(axis: Axis, v: f32, x: f32, y: f32, z: f32) -> Vec3f {
        let mut out = Vec3f::new(x, y, z);
        *axis.select_mut(&mut out) = v;
        out
    }

    let axis = dir.axis();
    let v = if dir.is_positive() {
        axis.select(max)
    } else {
        axis.select(min)
    };

    [
        make(axis, v, a1_min, a2_min, 0.0),
        make(axis, v, a1_min, a2_max, 0.0),
        make(axis, v, a1_max, a2_max, 0.0),
        make(axis, v, a1_max, a2_min, 0.0),
    ]
}

pub struct BakedQuad {
    pub vertices: [Vec3f; 4],
    pub uv: Vec4f,
    pub texture: TextureId,
    pub cullface: Option<Direction>,
    pub tintindex: Option<u8>,
    pub shade: bool,
    pub light_emission: u8,
}

pub enum ModelBakeError {
    TextureNotFound(String),
    ParentNotFound(String),
}

impl BlockModelDefinition {
    fn _build_texture_map<F>(
        &self,
        parent_resolver: F,
        texture_map: &mut FxHashMap<ReferenceId, TextureId>,
    ) -> Result<(), ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<BlockModelDefinition>,
    {
        for (name, texture) in &self.textures {
            let texture_id = match texture {
                BlockModelTexture::RefOr(RefOr::Value(id)) => Some(id.clone()),
                BlockModelTexture::Object(obj) => match &obj.sprite {
                    RefOr::Value(id) => Some(id.clone()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(texture_id) = texture_id {
                texture_map.insert(ReferenceId::new(name.clone()), texture_id);
            }
        }

        self.parent.as_ref().map_or(Ok(()), |parent_id| {
            parent_resolver(parent_id)
                .ok_or_else(|| ModelBakeError::ParentNotFound(parent_id.to_string()))
                .and_then(|parent| parent._build_texture_map(parent_resolver, texture_map))
        })
    }

    pub fn build_texture_map<F>(
        &self,
        parent_resolver: F,
    ) -> Result<FxHashMap<ReferenceId, TextureId>, ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<BlockModelDefinition>,
    {
        let mut texture_map = FxHashMap::default();
        self._build_texture_map(parent_resolver, &mut texture_map)?;
        Ok(texture_map)
    }

    pub fn bake<F, E>(&self, parent_resolver: F) -> Result<(), ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<BlockModelDefinition>,
    {
        let texture_map = self.build_texture_map(parent_resolver)?;

        let mut quads = Vec::new();
        for element in &self.elements {
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
                    let uv = face.uv.unwrap_or(Vec4f::new(0.0, 0.0, 16.0, 16.0));
                    let rotated_uv = face.rotation.rotate_uv(uv);

                    let texture = match &face.texture {
                        RefOr::Ref(id) => {
                            if let Some(texture_id) = texture_map.get(id) {
                                texture_id.clone()
                            } else {
                                return Err(ModelBakeError::TextureNotFound(id.to_string()));
                            }
                        }
                        RefOr::Value(id) => id.clone(),
                    };

                    quads.push(BakedQuad {
                        vertices: rotated_quad_vertices,
                        uv: rotated_uv,
                        texture,
                        tintindex: face.tintindex,
                        cullface: face.cullface,
                        shade: element.shade,
                        light_emission: element.light_emission,
                    });
                }
            }
        }

        Ok(())
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
        let root_dir = manifest_dir.join("assets/minecraft/models/block");

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
