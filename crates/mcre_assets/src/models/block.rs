use core::{array, error, fmt};

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use mcre_core::{Axis, Direction, Vec3f, Vec4f};
use serde::Deserialize;

use crate::{BlockModelId, FxHashMap, Quadrant, RefOr, ReferenceId, TextureId};

#[derive(Debug, Clone)]
pub struct BlockModelDefinition {
    pub gui_light: Option<GuiLight>,
    pub parent: Option<BlockModelId>,
    pub ambientocclusion: bool,
    pub elements: Vec<BlockModelElement>,
    pub textures: FxHashMap<String, RefOr<TextureId>>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct Transform {
    pub translation: Vec3f,
    pub rotation: Option<Vec3f>,
    pub scale: Option<Vec3f>,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockModelElementRotation {
    pub origin: Vec3f,
    pub axis: Axis,
    pub angle: f32,
    #[serde(default)]
    pub rescale: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockModelFace {
    pub texture: RefOr<TextureId>,
    #[serde(default)]
    pub rotation: Quadrant,
    pub uv: Option<Vec4f>,
    pub tintindex: Option<u8>,
    pub cullface: Option<Direction>,
}

impl BlockModelElementRotation {
    pub fn apply_on_point(&self, point: Vec3f) -> Vec3f {
        let mut point = point - self.origin;
        let (sin, cos) = self.angle.sin_cos();

        // Minecraft Java Edition Rescale Logic
        // Formula: 1.0 / (|cos(angle)| + |sin(angle)|)
        let scale = if self.rescale {
            1.0 / (cos.abs() + sin.abs())
        } else {
            1.0
        };

        match self.axis {
            Axis::X => {
                let y = point[1];
                let z = point[2];
                // Rotate Y and Z, then apply scale
                point[1] = (y * cos - z * sin) * scale;
                point[2] = (y * sin + z * cos) * scale;
            }
            Axis::Y => {
                let x = point[0];
                let z = point[2];
                // Rotate X and Z, then apply scale
                point[0] = (x * cos + z * sin) * scale;
                point[2] = (z * cos - x * sin) * scale;
            }
            Axis::Z => {
                let x = point[0];
                let y = point[1];
                // Rotate X and Y, then apply scale
                point[0] = (x * cos - y * sin) * scale;
                point[1] = (x * sin + y * cos) * scale;
            }
        }
        point + self.origin
    }

    // apply_on_quad remains the same, but must now also pass 'from' and 'to'
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

#[derive(Debug, Clone)]
pub enum ModelBakeError {
    TextureNotFound(String),
    ParentNotFound(String),
}

impl fmt::Display for ModelBakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelBakeError::TextureNotFound(name) => write!(f, "Texture not found: {}", name),
            ModelBakeError::ParentNotFound(id) => write!(f, "Parent not found: {}", id),
        }
    }
}

impl error::Error for ModelBakeError {}

impl BlockModelDefinition {
    fn _build_texture_map<'a, F>(
        &self,
        parent_resolver: F,
        texture_map: &mut FxHashMap<ReferenceId, TextureId>,
    ) -> Result<(), ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<&'a Self>,
    {
        for (name, texture) in &self.textures {
            if let RefOr::Value(texture_id) = texture {
                texture_map.insert(ReferenceId::new(name.clone()), texture_id.clone());
            }
        }

        self.parent.as_ref().map_or(Ok(()), |parent_id| {
            parent_resolver(parent_id)
                .ok_or_else(|| ModelBakeError::ParentNotFound(parent_id.to_string()))
                .and_then(|parent| parent._build_texture_map(parent_resolver, texture_map))
        })
    }

    pub fn build_texture_map<'a, F>(
        &self,
        parent_resolver: F,
    ) -> Result<FxHashMap<ReferenceId, TextureId>, ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<&'a Self>,
    {
        let mut texture_map = FxHashMap::default();
        self._build_texture_map(parent_resolver, &mut texture_map)?;
        Ok(texture_map)
    }

    pub fn bake<'a, F>(&self, parent_resolver: F) -> Result<Box<[BakedQuad]>, ModelBakeError>
    where
        F: Fn(&BlockModelId) -> Option<&'a Self>,
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

        Ok(quads.into_boxed_slice())
    }
}

pub struct BlockModelRegistry {
    baked_quads: Box<[Box<[BakedQuad]>]>,
    definitions: Box<[BlockModelDefinition]>,
    id_mappings: FxHashMap<String, u16>,
}

impl BlockModelRegistry {
    pub fn new(
        named_definitions: Box<[(String, BlockModelDefinition)]>,
    ) -> Result<Self, ModelBakeError> {
        let mut id_mappings = FxHashMap::default();
        let mut definitions = Vec::with_capacity(named_definitions.len());
        let mut baked_quads = Vec::with_capacity(named_definitions.len());

        for (i, (name, definition)) in named_definitions.into_iter().enumerate() {
            id_mappings.insert(name, i as u16);
            definitions.push(definition);
        }

        for definition in &definitions {
            let baked = definition.bake(|id| {
                id_mappings
                    .get(&id.id)
                    .map(|&idx| &definitions[idx as usize])
            })?;
            baked_quads.push(baked);
        }

        Ok(Self {
            baked_quads: baked_quads.into_boxed_slice(),
            definitions: definitions.into_boxed_slice(),
            id_mappings,
        })
    }

    pub fn get_definition(&self, id: BlockModelRegistryKey) -> &BlockModelDefinition {
        &self.definitions[id.0 as usize]
    }

    pub fn get_quads(&self, id: BlockModelRegistryKey) -> &[BakedQuad] {
        &self.baked_quads[id.0 as usize]
    }

    pub fn get_key_by_id(&self, id: &BlockModelId) -> Option<BlockModelRegistryKey> {
        self.id_mappings
            .get(&id.id)
            .map(|&id| BlockModelRegistryKey(id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockModelRegistryKey(u16);

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
        BlockModelId, FxHashMap, RefOr, TextureId,
        block::{
            BlockModelDefinition, BlockModelElement, GuiLight, Transform, default_ambientocclusion,
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
                    let mut textures: Option<FxHashMap<String, RefOr<TextureId>>> = None;
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

                                    let ref_or_texture: RefOr<TextureId> =
                                        serde::Deserialize::deserialize(value)
                                            .map_err(de::Error::custom)?;

                                    filtered_map.insert(key, ref_or_texture);
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
