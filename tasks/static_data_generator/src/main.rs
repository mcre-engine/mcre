use std::{fmt::Write, path::PathBuf};

use convert_case::ccase;
use indexmap::IndexMap;
use mcre_core::StateValue;
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
        root_path.join("data/name.rs"),
        format!(
            "static VALUES: [&str; {}] = {:#?};

pub(crate) fn get(idx: u16) -> &'static str {{
    VALUES[idx as usize]
}}",
            blocks.len(),
            name_array
        ),
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("data/display_name.rs"),
        format!(
            "static VALUES: [&str; {}] = {:#?};

pub(crate) fn get(idx: u16) -> &'static str {{
    VALUES[idx as usize]
}}",
            blocks.len(),
            display_name_array
        ),
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("data/default_state_id.bin"),
        default_state_array,
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("data/default_state_id.rs"),
        format!(
            "static VALUES: [u16; {}] = unsafe {{
    core::mem::transmute(*include_bytes!(\"./default_state_id.bin\"))
}};

pub(crate) fn get(idx: u16) -> u16 {{
    VALUES[idx as usize]
}}",
            blocks.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(root_path.join("data/min_state_id.bin"), min_state_id_array)
        .await
        .unwrap();

    fs::write(
        root_path.join("data/min_state_id.rs"),
        format!(
            "static VALUES: [u16; {}] = unsafe {{
    core::mem::transmute(*include_bytes!(\"./min_state_id.bin\"))
}};

pub(crate) fn get(idx: u16) -> u16 {{
    VALUES[idx as usize]
}}",
            blocks.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(root_path.join("data/max_state_id.bin"), max_state_id_array)
        .await
        .unwrap();

    fs::write(
        root_path.join("data/max_state_id.rs"),
        format!(
            "static VALUES: [u16; {}] = unsafe {{
    core::mem::transmute(*include_bytes!(\"./max_state_id.bin\"))
}};

pub(crate) fn get(idx: u16) -> u16 {{
    VALUES[idx as usize]
}}",
            blocks.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("data/mod.rs"),
        "pub(crate) mod name;
pub(crate) mod display_name;
pub(crate) mod default_state_id;
pub(crate) mod min_state_id;
pub(crate) mod max_state_id;",
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("mod.rs"),
        "mod data;

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
        data::name::get(self.0)
    }

    pub fn display_name(self) -> &'static str {
        data::display_name::get(self.0)
    }

    pub fn default_state_id(self) -> StateId {
        data::default_state_id::get(self.0).into()
    }

    pub fn min_state_id(self) -> StateId {
        data::min_state_id::get(self.0).into()
    }

    pub fn max_state_id(self) -> StateId {
        data::max_state_id::get(self.0).into()
    }
}
",
    )
    .await
    .unwrap();
}

#[derive(Debug)]
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
                    .or_default();
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
            }

            let field_name = if state_field.values.is_bool() {
                format!("is_{}", state_field.name)
            } else {
                state_field.name.clone()
            };

            if let Some(value_schema) = fields.get_mut(&field_name) {
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
                    BlockStateFieldValues::Int { min, max } => StateValueSchema::Int(*min, *max),
                    BlockStateFieldValues::Enum { enum_name, .. } => {
                        StateValueSchema::Enum(enum_name)
                    }
                };
                fields.insert(field_name, schema);
            }
        }
    }

    let mut field_mappings = IndexMap::new();

    for (field_name, enum_names) in field_name_to_value_schemas {
        if enum_names.len() == 1 {
            continue;
        }
        let mapped_field_prefixes = strip_common_suffix(
            enum_names
                .iter()
                .map(|(enum_name, _)| enum_name.as_str())
                .collect(),
        );
        fields.shift_remove(field_name);
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

async fn generate_states(states: &[BlockState], state_fields_data: &StateFieldsData<'_>) {
    let root_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/mcre_static_data/src/state");
    let mut block_id_array = Vec::new();
    let mut light_emission_array = SubByteArray::new(0, 2, states.len());
    let mut use_shape_for_light_occlusion_array = SubByteArray::new(0, 0, states.len());
    let mut propagates_skylight_down_array = SubByteArray::new(0, 0, states.len());
    let mut light_block_array = SubByteArray::new(0, 2, states.len());
    let mut solid_render_array = SubByteArray::new(0, 0, states.len());
    let mut is_air_array = SubByteArray::new(0, 0, states.len());
    let mut ignited_by_lava_array = SubByteArray::new(0, 0, states.len());
    let mut can_occlude_array = SubByteArray::new(0, 0, states.len());
    let mut is_randomly_ticking_array = SubByteArray::new(0, 0, states.len());
    let mut replaceable_array = SubByteArray::new(0, 0, states.len());
    let mut spawn_terrain_particles_array = SubByteArray::new(0, 0, states.len());
    let mut requires_correct_tool_for_drops_array = SubByteArray::new(0, 0, states.len());
    let mut destroy_speed_array = Vec::new();
    let mut offset_type_array = SubByteArray::new(0, 1, states.len());
    let mut max_horizontal_offset_array = Vec::new();
    let mut max_vertical_offset_array = Vec::new();

    let mut state_value_arrays = IndexMap::new();

    for (field, schema) in &state_fields_data.field_schemas {
        let (num_values, min) = match schema {
            StateValueSchema::Bool => (2, 0),
            StateValueSchema::Int(min, max) => (*max as u32 - *min as u32, *min),
            StateValueSchema::Enum(enum_name) => (
                state_fields_data.enums.get(enum_name).unwrap().len() as u32,
                0,
            ),
        };

        let num_bits = (num_values as f32).log2().ceil() as u32;
        let pow = (num_bits as f32).log2().ceil() as u8;

        let array = SubByteArray::new(min, pow, states.len());

        state_value_arrays.insert(field, array);
    }

    for (i, state) in states.iter().enumerate() {
        block_id_array.extend(state.block_id.to_ne_bytes());
        light_emission_array.set(i, state.light_emission);
        use_shape_for_light_occlusion_array.set(i, state.use_shape_for_light_occlusion as u8);
        propagates_skylight_down_array.set(i, state.propagates_skylight_down as u8);
        light_block_array.set(i, state.light_block);
        solid_render_array.set(i, state.solid_render as u8);
        is_air_array.set(i, state.is_air as u8);
        ignited_by_lava_array.set(i, state.ignited_by_lava as u8);
        can_occlude_array.set(i, state.can_occlude as u8);
        is_randomly_ticking_array.set(i, state.is_randomly_ticking as u8);
        replaceable_array.set(i, state.replaceable as u8);
        spawn_terrain_particles_array.set(i, state.spawn_terrain_particles as u8);
        requires_correct_tool_for_drops_array.set(i, state.requires_correct_tool_for_drops as u8);
        destroy_speed_array.extend(state.destroy_speed.to_ne_bytes());
        offset_type_array.set(i, state.offset_type as u8);
        max_horizontal_offset_array.extend(state.max_horizontal_offset.to_ne_bytes());
        max_vertical_offset_array.extend(state.max_vertical_offset.to_ne_bytes());

        for (field_name, field_value) in &state.state_values {
            let field_name = if let Some(rename) = state_fields_data
                .field_rename
                .get(&(&state.block_name, field_name))
            {
                rename.clone()
            } else if matches!(field_value, StateValue::Bool(_)) {
                format!("is_{}", field_name)
            } else {
                field_name.clone()
            };

            let field_schema = state_fields_data.field_schemas.get(&field_name).unwrap();

            let value_int = match field_value {
                StateValue::Bool(bool) => *bool as u8,
                StateValue::Int(int) => *int,
                StateValue::String(string) => match field_schema {
                    StateValueSchema::Enum(enum_name) => state_fields_data
                        .enums
                        .get(enum_name)
                        .unwrap()
                        .iter()
                        .position(|variant| variant == string)
                        .unwrap()
                        .try_into()
                        .unwrap(),
                    _ => {
                        unreachable!()
                    }
                },
            };

            state_value_arrays
                .get_mut(&field_name)
                .unwrap()
                .set(i, value_int);
        }
    }

    let mut root_mod = String::new();

    if !state_fields_data.field_schemas.is_empty() {
        write!(root_mod, "mod enums;\nmod data;\n\nuse enums::*;\n").unwrap();
    }

    write!(
        root_mod,
        "use crate::BlockId;\nuse mcre_core::OffsetType;\n\n"
    )
    .unwrap();

    write!(
        root_mod,
        "#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct StateId(u16);

impl From<u16> for StateId {{
    fn from(id: u16) -> Self {{
        Self(id)
    }}
}}

impl From<StateId> for u16 {{
    fn from(id: StateId) -> Self {{
        id.0
    }}
}}

impl StateId {{
    pub fn block_id(self) -> BlockId {{
        data::block_id::get(self.0).into()
    }}

    pub fn light_emission(self) -> u8 {{
        data::light_emission::get(self.0)
    }}

    pub fn use_shape_for_light_occlusion(self) -> bool {{
        data::use_shape_for_light_occlusion::get(self.0)
    }}

    pub fn propagates_skylight_down(self) -> bool {{
        data::propagates_skylight_down::get(self.0)
    }}

    pub fn light_block(self) -> u8 {{
        data::light_block::get(self.0)
    }}

    pub fn solid_render(self) -> bool {{
        data::solid_render::get(self.0)
    }}

    pub fn is_air(self) -> bool {{
        data::is_air::get(self.0)
    }}

    pub fn ignited_by_lava(self) -> bool {{
        data::ignited_by_lava::get(self.0)
    }}

    pub fn can_occlude(self) -> bool {{
        data::can_occlude::get(self.0)
    }}

    pub fn is_randomly_ticking(self) -> bool {{
        data::is_randomly_ticking::get(self.0)
    }}

    pub fn replaceable(self) -> bool {{
        data::replaceable::get(self.0)
    }}

    pub fn spawn_terrain_particles(self) -> bool {{
        data::spawn_terrain_particles::get(self.0)
    }}

    pub fn requires_correct_tool_for_drops(self) -> bool {{
        data::requires_correct_tool_for_drops::get(self.0)
    }}

    pub fn destroy_speed(self) -> f32 {{
        data::destroy_speed::get(self.0)
    }}

    pub fn offset_type(self) -> OffsetType {{
        unsafe {{ core::mem::transmute::<u8, OffsetType>(data::offset_type::get(self.0)) }}
    }}

    pub fn max_horizontal_offset(self) -> f32 {{
        data::max_horizontal_offset::get(self.0)
    }}

    pub fn max_vertical_offset(self) -> f32 {{
        data::max_vertical_offset::get(self.0)
    }}\n"
    )
    .unwrap();

    let mut state_fields_mod = String::new();

    for (field, schema) in &state_fields_data.field_schemas {
        let arr = state_value_arrays.swap_remove(field).unwrap();
        let (src, data) = arr.compile(field, matches!(schema, StateValueSchema::Bool));
        fs::write(root_path.join(format!("data/state_fields/{field}.rs")), src)
            .await
            .unwrap();
        fs::write(
            root_path.join(format!("data/state_fields/{field}.bin")),
            data,
        )
        .await
        .unwrap();
        writeln!(state_fields_mod, "pub(crate) mod {field};").unwrap();
        match schema {
            StateValueSchema::Bool => write!(
                root_mod,
                "\n    pub fn {0}(self) -> bool {{
        data::state_fields::{0}::get(self.0)
    }}\n",
                field
            )
            .unwrap(),
            StateValueSchema::Int(_, _) => write!(
                root_mod,
                "\n    pub fn {0}(self) -> u8 {{
        data::state_fields::{0}::get(self.0)
    }}\n",
                field
            )
            .unwrap(),
            StateValueSchema::Enum(enum_name) => write!(
                root_mod,
                "\n    pub fn {0}(self) -> {1} {{
        unsafe {{ core::mem::transmute::<u8, {1}>(data::state_fields::{0}::get(self.0)) }}
    }}\n",
                field, enum_name
            )
            .unwrap(),
        }
    }

    write!(root_mod, "}}").unwrap();

    fs::write(root_path.join("data/block_id.bin"), block_id_array)
        .await
        .unwrap();
    fs::write(
        root_path.join("data/block_id.rs"),
        format!(
            "static VALUES: [u16; {}] = unsafe {{ core::mem::transmute(*include_bytes!(\"./block_id.bin\")) }};

pub(crate) fn get(idx: u16) -> u16 {{
    VALUES[idx as usize]
}}",
            states.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("data/destroy_speed.bin"),
        destroy_speed_array,
    )
    .await
    .unwrap();
    fs::write(
        root_path.join("data/destroy_speed.rs"),
        format!(
            "static VALUES: [f32; {}] = unsafe {{ core::mem::transmute(*include_bytes!(\"./destroy_speed.bin\")) }};

pub(crate) fn get(idx: u16) -> f32 {{
    VALUES[idx as usize]
}}",
            states.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("data/max_horizontal_offset.bin"),
        max_horizontal_offset_array,
    )
    .await
    .unwrap();
    fs::write(
        root_path.join("data/max_horizontal_offset.rs"),
        format!(
            "static VALUES: [f32; {}] = unsafe {{ core::mem::transmute(*include_bytes!(\"./max_horizontal_offset.bin\")) }};

pub(crate) fn get(idx: u16) -> f32 {{
    VALUES[idx as usize]
}}",
            states.len(),
        ),
    )
    .await
    .unwrap();

    fs::write(
        root_path.join("data/max_vertical_offset.bin"),
        max_vertical_offset_array,
    )
    .await
    .unwrap();
    fs::write(
        root_path.join("data/max_vertical_offset.rs"),
        format!(
            "static VALUES: [f32; {}] = unsafe {{ core::mem::transmute(*include_bytes!(\"./max_vertical_offset.bin\")) }};

pub(crate) fn get(idx: u16) -> f32 {{
    VALUES[idx as usize]
}}",
            states.len(),
        ),
    )
    .await
    .unwrap();

    let props = [
        ("light_emission", light_emission_array),
        (
            "use_shape_for_light_occlusion",
            use_shape_for_light_occlusion_array,
        ),
        ("propagates_skylight_down", propagates_skylight_down_array),
        ("light_block", light_block_array),
        ("solid_render", solid_render_array),
        ("is_air", is_air_array),
        ("ignited_by_lava", ignited_by_lava_array),
        ("can_occlude", can_occlude_array),
        ("is_randomly_ticking", is_randomly_ticking_array),
        ("replaceable", replaceable_array),
        ("spawn_terrain_particles", spawn_terrain_particles_array),
        (
            "requires_correct_tool_for_drops",
            requires_correct_tool_for_drops_array,
        ),
        ("offset_type", offset_type_array),
    ];

    for (name, arr) in props {
        let is_bool = arr.pow == 0;
        let (src, data) = arr.compile(name, is_bool);
        fs::write(root_path.join(format!("data/{name}.bin")), data)
            .await
            .unwrap();
        fs::write(root_path.join(format!("data/{name}.rs")), src)
            .await
            .unwrap();
    }

    fs::write(root_path.join("mod.rs"), root_mod).await.unwrap();
    fs::write(
        root_path.join("data/mod.rs"),
        "pub(crate) mod block_id;
pub(crate) mod light_emission;
pub(crate) mod use_shape_for_light_occlusion;
pub(crate) mod propagates_skylight_down;
pub(crate) mod light_block;
pub(crate) mod solid_render;
pub(crate) mod is_air;
pub(crate) mod ignited_by_lava;
pub(crate) mod can_occlude;
pub(crate) mod is_randomly_ticking;
pub(crate) mod replaceable;
pub(crate) mod spawn_terrain_particles;
pub(crate) mod requires_correct_tool_for_drops;
pub(crate) mod destroy_speed;
pub(crate) mod offset_type;
pub(crate) mod max_horizontal_offset;
pub(crate) mod max_vertical_offset;

pub(crate) mod state_fields;",
    )
    .await
    .unwrap();
    fs::write(root_path.join("data/state_fields/mod.rs"), state_fields_mod)
        .await
        .unwrap();
}

pub struct SubByteArray {
    min: u8,
    pow: u8,
    #[allow(unused)]
    len: usize,
    values: Box<[u8]>,
}

impl SubByteArray {
    fn new(min: u8, pow: u8, len: usize) -> Self {
        let bits_per_value = 1 << pow;
        let total_bits = bits_per_value * len;
        let total_bytes = total_bits.div_ceil(8);

        Self {
            min,
            pow,
            len,
            values: vec![0; total_bytes].into_boxed_slice(),
        }
    }

    fn set(&mut self, index: usize, val: u8) {
        let bits_per_value = 1 << self.pow;

        let values_per_byte = 8 / bits_per_value;

        let value_index = index / values_per_byte;
        let shift = (index % values_per_byte) * bits_per_value;

        let mask = ((1u16 << bits_per_value) - 1) as u8;

        self.values[value_index] |= ((val - self.min) & mask) << shift;
    }

    #[allow(unused)]
    fn get(&self, index: usize) -> Option<u8> {
        let bits_per_value = 1 << self.pow;

        let values_per_byte = 8 / bits_per_value;

        let value_index = index / values_per_byte;
        let shift = (index % values_per_byte) * bits_per_value;

        let mask = ((1u16 << bits_per_value) - 1) as u8;

        self.values
            .get(value_index)
            .map(|&value| ((value >> shift) & mask) + self.min)
    }

    pub fn compile(self, name: &str, is_bool: bool) -> (String, Box<[u8]>) {
        if is_bool {
            (
                format!(
                    "static VALUES: [u8; {}] = *include_bytes!(\"./{name}.bin\");

pub(crate) fn get(idx: u16) -> bool {{
    let byte = idx / 8;
    let bit = idx % 8;

    let byte = VALUES[byte as usize];

    ((byte >> bit) & 1) == 1
}}",
                    self.values.len(),
                ),
                self.values,
            )
        } else {
            let bits_per_value = 1 << self.pow;

            let values_per_byte = 8 / bits_per_value;

            let mask = ((1u16 << bits_per_value) - 1) as u8;

            let src = if self.pow == 3 {
                format!(
                    "static VALUES: [u8; {}] = *include_bytes!(\"./{name}.bin\");

pub(crate) fn get(idx: u16) -> u8 {{
    VALUES[idx as usize]
}}",
                    self.values.len(),
                )
            } else if self.min == 0 {
                format!(
                    "static VALUES: [u8; {}] = *include_bytes!(\"./{name}.bin\");

pub(crate) fn get(idx: u16) -> u8 {{
    let byte = idx / {values_per_byte};
    let bit = idx % {values_per_byte};

    let byte = VALUES[byte as usize];

    (byte >> bit) & {mask}
}}",
                    self.values.len(),
                )
            } else {
                let min = self.min;
                format!(
                    "static VALUES: [u8; {}] = *include_bytes!(\"./{name}.bin\");

pub(crate) fn get(idx: u16) -> u8 {{
    let byte = idx / {values_per_byte};
    let bit = idx % {values_per_byte};

    let byte = VALUES[byte as usize];

    ((byte >> bit) & {mask}) + {min}
}}",
                    self.values.len(),
                )
            };

            (src, self.values)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::SubByteArray;

    #[test]
    fn test_sub_byte_array() {
        let mut arr = SubByteArray::new(1, 2, 100);

        for i in 0..15 {
            arr.set(i, i as u8 + 1);
        }

        for i in 0..15 {
            assert_eq!(arr.get(i).unwrap(), i as u8 + 1);
        }
    }
}
