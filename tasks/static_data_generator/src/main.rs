use std::{collections::HashSet, fmt::Write, path::PathBuf};

use convert_case::ccase;
use indexmap::IndexMap;
use mcre_data::{
    block::{Block, BlockStateFieldValues},
    state::BlockState,
};
use tokio::fs;

#[tokio::main]
async fn main() {
    let blocks = Block::all().await.unwrap();
    let block_states = BlockState::all().await.unwrap();

    generate_blocks(&blocks).await;

    let state_fields_data = generate_state_enums(&blocks).await;

    generate_states(&block_states, &state_fields_data).await;

    fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/mcre_static_data/src/lib.rs"),
        "mod block;
mod state;

pub use block::*;
pub use state::*;
",
    )
    .await
    .unwrap();
}

async fn generate_blocks(blocks: &[Block]) {
    let root_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/mcre_static_data/src/block");
    let mut name_array = Vec::new();
    let mut display_name_array = Vec::new();
    let mut default_state_array = Vec::new();
    let mut min_state_id_array = Vec::new();
    let mut max_state_id_array = Vec::new();

    for block in blocks {
        name_array.push(&block.name);
        display_name_array.push(&block.display_name);
        default_state_array.extend(block.default_state_id.to_ne_bytes());
        min_state_id_array.extend(block.min_state_id.to_ne_bytes());
        max_state_id_array.extend(block.max_state_id.to_ne_bytes());
    }

    fs::write(
        root_path.join("name.rs"),
        format!(
            "pub static NAME_VALUES: [&'static str; {}] = {:#?};",
            blocks.len(),
            name_array
        ),
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("display_name.rs"),
        format!(
            "pub static DISPLAY_NAME_VALUES: [&'static str; {}] = {:#?};",
            blocks.len(),
            display_name_array
        ),
    )
    .await
    .unwrap();

    fs::write(root_path.join("default_state_id.bin"), default_state_array)
        .await
        .unwrap();

    fs::write(
        root_path.join("default_state_id.rs"),
        format!(
            "pub static DEFAULT_STATE_ID_VALUES: [u16; {}] = unsafe {{
    core::mem::transmute(*include_bytes!(\"./default_state_id.bin\"))
}};",
            blocks.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(root_path.join("min_state_id.bin"), min_state_id_array)
        .await
        .unwrap();

    fs::write(
        root_path.join("min_state_id.rs"),
        format!(
            "pub static MIN_STATE_ID_VALUES: [u16; {}] = unsafe {{
    core::mem::transmute(*include_bytes!(\"./min_state_id.bin\"))
}};",
            blocks.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(root_path.join("max_state_id.bin"), max_state_id_array)
        .await
        .unwrap();

    fs::write(
        root_path.join("max_state_id.rs"),
        format!(
            "pub static MAX_STATE_ID_VALUES: [u16; {}] = unsafe {{
    core::mem::transmute(*include_bytes!(\"./max_state_id.bin\"))
}};",
            blocks.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("mod.rs"),
        "mod name;
mod display_name;
mod default_state_id;
mod min_state_id;
mod max_state_id;

use crate::StateId;

#[derive(Debug, Copy, Clone, Hash)]
pub struct BlockId(u16);

impl From<u16> for BlockId {
    fn from(id: u16) -> Self {
        Self(id)
    }
}

impl From<BlockId> for u16 {
    fn from(id: BlockId) -> Self {
        id.0
    }
}

impl BlockId {
    pub fn name(self) -> &'static str {
        name::NAME_VALUES[self.0 as usize]
    }

    pub fn display_name(self) -> &'static str {
        display_name::DISPLAY_NAME_VALUES[self.0 as usize]
    }

    pub fn default_state_id(self) -> StateId {
        default_state_id::DEFAULT_STATE_ID_VALUES[self.0 as usize].into()
    }

    pub fn min_state_id(self) -> StateId {
        min_state_id::MIN_STATE_ID_VALUES[self.0 as usize].into()
    }

    pub fn max_state_id(self) -> StateId {
        max_state_id::MAX_STATE_ID_VALUES[self.0 as usize].into()
    }
}
",
    )
    .await
    .unwrap();
}

enum StateValueSchema<'a> {
    Bool,
    Int(u8, u8),
    Enum(&'a String),
}

struct StateFieldsData<'a> {
    enums: IndexMap<&'a String, Vec<String>>,
    field_rename: IndexMap<(&'a String, &'a String), String>,
    field_schemas: IndexMap<String, StateValueSchema<'a>>,
}

async fn generate_state_enums<'a>(blocks: &'a [Block]) -> StateFieldsData<'a> {
    let mut enums: IndexMap<&String, Vec<String>> = IndexMap::new();
    let mut field_name_to_value_schemas: IndexMap<&String, Vec<(&String, Vec<&Block>)>> =
        IndexMap::new();

    let mut fields = IndexMap::new();

    for block in blocks {
        for state_field in &block.states {
            if let BlockStateFieldValues::Enum { enum_name, values } = &state_field.values {
                let schemas = field_name_to_value_schemas
                    .entry(&state_field.name)
                    .or_insert(Vec::new());
                if let Some(pos) = schemas
                    .iter()
                    .position(|(enum_name1, _)| enum_name1 == &enum_name)
                {
                    schemas[pos].1.push(block);
                } else {
                    schemas.push((enum_name, vec![block]));
                }
                if let Some(existing_enum_values) = enums.get_mut(&enum_name) {
                    for value in values {
                        if !existing_enum_values.contains(value) {
                            existing_enum_values.push(value.clone());
                        }
                    }
                } else if enum_name == "Direction" {
                    enums.insert(
                        enum_name,
                        vec![
                            "down".to_string(),
                            "up".to_string(),
                            "north".to_string(),
                            "south".to_string(),
                            "west".to_string(),
                            "east".to_string(),
                        ],
                    );
                } else if enum_name == "Axis" {
                    enums.insert(
                        enum_name,
                        vec!["x".to_string(), "y".to_string(), "z".to_string()],
                    );
                } else {
                    enums.insert(enum_name, values.clone());
                }
                if let Some(value_schema) = fields.get_mut(&state_field.name) {
                    match value_schema {
                        StateValueSchema::Bool => {
                            assert!(state_field.values.is_bool());
                        }
                        StateValueSchema::Int(min, max) => {
                            let (cur_min, cur_max) = state_field.values.as_int().unwrap();
                            *min = (*min).min(cur_min);
                            *max = (*max).max(cur_max);
                        }
                        StateValueSchema::Enum(_) => {
                            assert!(state_field.values.is_enum());
                        }
                    }
                } else {
                    let schema = match &state_field.values {
                        BlockStateFieldValues::Bool => StateValueSchema::Bool,
                        BlockStateFieldValues::Int { min, max } => {
                            StateValueSchema::Int(*min, *max)
                        }
                        BlockStateFieldValues::Enum { enum_name, .. } => {
                            StateValueSchema::Enum(enum_name)
                        }
                    };
                    fields.insert(state_field.name.clone(), schema);
                }
            }
        }
    }

    let mut field_mappings = IndexMap::new();

    for (field_name, enum_names) in field_name_to_value_schemas {
        let mapped_field_prefixes = strip_common_suffix(
            enum_names
                .iter()
                .map(|(enum_name, _)| enum_name.as_str())
                .collect(),
        );
        fields.swap_remove(field_name);
        for ((enum_name, blocks), mapped_field_prefix) in
            enum_names.iter().zip(mapped_field_prefixes)
        {
            let mapped_field_name = if mapped_field_prefix.is_empty() {
                field_name.clone()
            } else {
                format!("{}_{}", mapped_field_prefix.to_lowercase(), field_name)
            };

            for block in blocks {
                field_mappings.insert((&block.name, field_name), mapped_field_name.clone());
            }

            fields.insert(mapped_field_name, StateValueSchema::Enum(enum_name));
        }
    }

    let mut enums_out = String::new();

    write!(enums_out, "pub use mcre_core::{{Axis, Direction}};\n\n").unwrap();

    for (enum_name, values) in &enums {
        if ["Direction", "Axis"].contains(&enum_name.as_str()) {
            continue;
        }
        write!(
            enums_out,
            "#[derive(Debug, Copy, Clone)]\npub enum {} {{",
            enum_name
        )
        .unwrap();
        for (i, value) in values.iter().enumerate() {
            write!(enums_out, "\n    {} = {},", ccase!(pascal, value), i).unwrap();
        }
        write!(enums_out, "\n}}\n\n").unwrap();
    }

    fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/mcre_static_data/src/state/enums.rs"),
        enums_out,
    )
    .await
    .unwrap();

    StateFieldsData {
        enums,
        field_rename: field_mappings,
        field_schemas: fields,
    }
}

fn strip_common_suffix(strings: Vec<&str>) -> Vec<String> {
    if strings.is_empty() {
        return vec![];
    }

    // Find longest common suffix
    let min_len = strings.iter().map(|s| s.len()).min().unwrap();
    let mut suffix_len = 0;

    for i in 0..min_len {
        let c = strings[0].as_bytes()[strings[0].len() - 1 - i];
        if strings.iter().all(|s| s.as_bytes()[s.len() - 1 - i] == c) {
            suffix_len += 1;
        } else {
            break;
        }
    }

    strings
        .iter()
        .map(|s| s[..s.len() - suffix_len].to_string())
        .collect()
}

async fn generate_states<'a>(states: &'a [BlockState], state_fields_data: &StateFieldsData<'a>) {
    let mut root_mod = String::new();
    let mut imported_enums = HashSet::new();

    if !state_fields_data.field_schemas.is_empty() {
        write!(root_mod, "mod enums;\nuse enums::{{\n").unwrap();
        for (_field, schema) in &state_fields_data.field_schemas {
            if let StateValueSchema::Enum(enum_name) = schema {
                if imported_enums.insert(enum_name) {
                    write!(root_mod, "    {},\n", enum_name).unwrap();
                }
            }
        }
        write!(root_mod, "}};\n\n").unwrap();
    }

    write!(
        root_mod,
        "#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]\npub struct StateId(u16);\n\nimpl StateId {{"
    )
    .unwrap();

    for (field, schema) in &state_fields_data.field_schemas {
        match schema {
            StateValueSchema::Bool => write!(
                root_mod,
                "\n    pub fn is_{}(self) -> bool {{ todo!() }}\n",
                field
            )
            .unwrap(),
            StateValueSchema::Int(_, _) => write!(
                root_mod,
                "\n    pub fn {}(self) -> bool {{ todo!() }}\n",
                field
            )
            .unwrap(),
            StateValueSchema::Enum(enum_name) => write!(
                root_mod,
                "\n    pub fn {}(self) -> {} {{ todo!() }}\n",
                field, enum_name
            )
            .unwrap(),
        }
    }

    write!(root_mod, "}}").unwrap();

    fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/mcre_static_data/src/state/mod.rs"),
        root_mod,
    )
    .await
    .unwrap();
}
