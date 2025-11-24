use std::collections::HashMap;

use mcre_core::{Axis, Direction, Vec3f, Vec4f};
use serde::Deserialize;

use crate::{BlockModelId, BlockTextureId, RefOr, RotationDegrees, TextureId};

#[derive(Debug, Clone)]
pub struct BlockModelDefinition {
    pub gui_light: Option<GuiLight>,
    pub parent: Option<BlockModelId>,
    pub ambientocclusion: bool,
    pub elements: Vec<BlockModelElement>,
    pub textures: HashMap<String, RefOr<TextureId>>,
    pub display: HashMap<String, Transform>,
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
    pub faces: HashMap<Direction, BlockModelFace>,
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
    pub texture: RefOr<BlockTextureId>,
    #[serde(default)]
    pub rotation: RotationDegrees,
    pub uv: Option<Vec4f>,
    pub tintindex: Option<u8>,
    pub cullface: Option<Direction>,
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs::{self, File},
        path::PathBuf,
    };

    use crate::block::BlockModelDefinition;

    #[tokio::test]
    async fn test_parse_block_model_definition() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let manifest_dir = PathBuf::from(manifest_dir);
        let root_dir = manifest_dir.join("assets/minecraft/models/block");

        let mut total = 0;
        let mut passed = 0;
        let mut failed = Vec::new();

        let mut block_state_definitions = HashMap::new();

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
    use std::collections::HashMap;

    use serde::{Deserialize, Deserializer, de};
    use serde_json::Value;

    use crate::{
        BlockModelId, RefOr, TextureId,
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

                fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
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
                    let mut textures: Option<HashMap<String, RefOr<TextureId>>> = None;
                    let mut display: Option<HashMap<String, Transform>> = None;

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

                                let raw_map: HashMap<String, Value> = map.next_value()?;

                                let mut filtered_map = HashMap::new();

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
