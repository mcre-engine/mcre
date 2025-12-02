use std::{fs, path::PathBuf};

use indexmap::IndexMap;
use jni::{
    JNIEnv,
    objects::{JObject, JString, JValueGen},
};
use mcje::{get_registry, iterate};
use mcre_core::OffsetType;
use mcre_data::{
    block::{Block, BlockStateField, BlockStateFieldValues},
    state::{BlockState, StateValue},
};

const BLOCK_DATA_PATH: &str = "crates/mcre_data/blocks.json";
const BLOCK_STATE_DATA_PATH: &str = "crates/mcre_data/block_states.json";

#[mcje::main]
async fn main(env: &mut JNIEnv<'_>) {
    generate_block_data(env);
    generate_block_state_data(env);
}

fn generate_block_data(env: &mut JNIEnv) {
    println!("[DEBUG] Generating block data");
    let block_registry = get_registry(env, "BLOCK", "DefaultedRegistry");

    let block_state_registry = env
        .get_static_field(
            "net/minecraft/world/level/block/Block",
            "BLOCK_STATE_REGISTRY",
            "Lnet/minecraft/core/IdMapper;",
        )
        .unwrap()
        .l()
        .unwrap();

    let mut blocks = Vec::new();

    let mut block_state_id_counter = 0u16;

    iterate(&block_registry, env, |i, block, env| {
        let display_name = get_block_display_name(env, &block);
        let name = get_block_name(&block, &block_registry, env);
        let default_state = env
            .get_field(
                &block,
                "defaultBlockState",
                "Lnet/minecraft/world/level/block/state/BlockState;",
            )
            .unwrap()
            .l()
            .unwrap();

        let default_state_id = env
            .call_static_method(
                "net/minecraft/world/level/block/Block",
                "getId",
                "(Lnet/minecraft/world/level/block/state/BlockState;)I",
                &[JValueGen::Object(&default_state)],
            )
            .unwrap()
            .i()
            .unwrap() as u16;

        let states = get_block_states(&block, env);

        let min_state_id = block_state_id_counter;

        let mut owner = name.clone();

        while owner == name {
            block_state_id_counter += 1;
            let state = env
                .call_method(
                    &block_state_registry,
                    "byId",
                    "(I)Ljava/lang/Object;",
                    &[JValueGen::Int(block_state_id_counter as i32)],
                )
                .unwrap()
                .l()
                .unwrap();
            if state.is_null() {
                break;
            }
            let state_owner = env
                .get_field(state, "owner", "Ljava/lang/Object;")
                .unwrap()
                .l()
                .unwrap();
            let state_owner_name = get_block_name(&state_owner, &block_registry, env);
            owner = state_owner_name;
        }

        let max_state_id = block_state_id_counter - 1;

        blocks.push(Block {
            id: i as u16,
            name,
            display_name,
            default_state_id,
            min_state_id,
            max_state_id,
            states,
        });
    });
    println!("[DEBUG] Done!");

    let json_string = serde_json::to_string_pretty(&blocks).unwrap();
    let root = env!("CARGO_MANIFEST_DIR");
    let root = PathBuf::from(root);
    let data_path = root.join("../../").join(BLOCK_DATA_PATH);
    fs::write(&data_path, json_string).unwrap();

    println!("[DEBUG] Block data saved to `{}`", BLOCK_DATA_PATH);
}

fn generate_block_state_data(env: &mut JNIEnv) {
    println!("[DEBUG] Generating block state data");
    let block_registry = get_registry(env, "BLOCK", "DefaultedRegistry");

    let block_state_registry = env
        .get_static_field(
            "net/minecraft/world/level/block/Block",
            "BLOCK_STATE_REGISTRY",
            "Lnet/minecraft/core/IdMapper;",
        )
        .unwrap()
        .l()
        .unwrap();

    let mut block_state = env
        .call_method(
            &block_state_registry,
            "byId",
            "(I)Ljava/lang/Object;",
            &[JValueGen::Int(0)],
        )
        .unwrap()
        .l()
        .unwrap();

    let mut block_state_id = 0u16;

    let mut states = Vec::new();

    while !block_state.is_null() {
        let state = process_block_state(&block_registry, block_state_id, &block_state, env);
        states.push(state);
        block_state_id += 1;
        block_state = env
            .call_method(
                &block_state_registry,
                "byId",
                "(I)Ljava/lang/Object;",
                &[JValueGen::Int(block_state_id.into())],
            )
            .unwrap()
            .l()
            .unwrap();
    }

    println!("[DEBUG] Done!");

    let json_string = serde_json::to_string_pretty(&states).unwrap();
    let root = env!("CARGO_MANIFEST_DIR");
    let root = PathBuf::from(root);
    let data_path = root.join("../../").join(BLOCK_STATE_DATA_PATH);
    fs::write(&data_path, json_string).unwrap();

    println!(
        "[DEBUG] Block state data saved to `{}`",
        BLOCK_STATE_DATA_PATH
    );
}

fn process_block_state(
    block_registry: &JObject,
    id: u16,
    block_state: &JObject,
    env: &mut JNIEnv,
) -> BlockState {
    let block = env
        .get_field(block_state, "owner", "Ljava/lang/Object;")
        .unwrap()
        .l()
        .unwrap();

    let block_id: u16 = env
        .call_method(
            block_registry,
            "getId",
            "(Ljava/lang/Object;)I",
            &[JValueGen::Object(&block)],
        )
        .unwrap()
        .i()
        .unwrap()
        .try_into()
        .unwrap();

    let block_name = get_block_name(&block, block_registry, env);

    let light_emission: u8 = env
        .get_field(block_state, "lightEmission", "I")
        .unwrap()
        .i()
        .unwrap()
        .try_into()
        .unwrap();

    let use_shape_for_light_occlusion = env
        .get_field(block_state, "useShapeForLightOcclusion", "Z")
        .unwrap()
        .z()
        .unwrap();

    let propagates_skylight_down = env
        .get_field(block_state, "propagatesSkylightDown", "Z")
        .unwrap()
        .z()
        .unwrap();

    let light_block: u8 = env
        .get_field(block_state, "lightBlock", "I")
        .unwrap()
        .i()
        .unwrap()
        .try_into()
        .unwrap();

    let solid_render = env
        .get_field(block_state, "solidRender", "Z")
        .unwrap()
        .z()
        .unwrap();

    let is_air = env
        .get_field(block_state, "isAir", "Z")
        .unwrap()
        .z()
        .unwrap();

    let ignited_by_lava = env
        .get_field(block_state, "ignitedByLava", "Z")
        .unwrap()
        .z()
        .unwrap();

    let can_occlude = env
        .get_field(block_state, "canOcclude", "Z")
        .unwrap()
        .z()
        .unwrap();

    let is_randomly_ticking = env
        .get_field(block_state, "isRandomlyTicking", "Z")
        .unwrap()
        .z()
        .unwrap();

    let replaceable = env
        .get_field(block_state, "replaceable", "Z")
        .unwrap()
        .z()
        .unwrap();

    let spawn_terrain_particles = env
        .get_field(block_state, "spawnTerrainParticles", "Z")
        .unwrap()
        .z()
        .unwrap();

    let requires_correct_tool_for_drops = env
        .get_field(block_state, "requiresCorrectToolForDrops", "Z")
        .unwrap()
        .z()
        .unwrap();

    let destroy_speed = env
        .get_field(block_state, "destroySpeed", "F")
        .unwrap()
        .f()
        .unwrap();

    let offset_type = determine_offset_type(block_state, env);

    let max_horizontal_offset = env
        .call_method(&block, "getMaxHorizontalOffset", "()F", &[])
        .unwrap()
        .f()
        .unwrap();

    let max_vertical_offset = env
        .call_method(&block, "getMaxVerticalOffset", "()F", &[])
        .unwrap()
        .f()
        .unwrap();

    let state_values = get_state_values(block_state, env);

    BlockState {
        id,
        block_id,
        block_name,
        light_emission,
        use_shape_for_light_occlusion,
        propagates_skylight_down,
        light_block,
        solid_render,
        is_air,
        ignited_by_lava,
        can_occlude,
        is_randomly_ticking,
        replaceable,
        spawn_terrain_particles,
        requires_correct_tool_for_drops,
        destroy_speed,
        offset_type,
        max_horizontal_offset,
        max_vertical_offset,
        state_values,
    }
}

pub fn determine_offset_type(block_state: &JObject, env: &mut JNIEnv) -> OffsetType {
    let offset_function = env
        .get_field(
            block_state,
            "offsetFunction",
            "Lnet/minecraft/world/level/block/state/BlockBehaviour$OffsetFunction;",
        )
        .unwrap()
        .l()
        .unwrap();

    if offset_function.is_null() {
        return OffsetType::None;
    }

    let block_pos_class = "net/minecraft/core/BlockPos";

    for i in 0..10 {
        // BlockPos(i, i, i)
        let pos_obj = env
            .new_object(
                block_pos_class,
                "(III)V",
                &[JValueGen::Int(i), JValueGen::Int(i), JValueGen::Int(i)],
            )
            .unwrap();

        // Call offsetFunction.evaluate(state, pos)
        // Signature: (LBlockState;LBlockPos;)LVec3;
        let vec3_obj = env.call_method(
            &offset_function,
            "evaluate",
            "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/phys/Vec3;",
            &[JValueGen::Object(block_state), JValueGen::Object(&pos_obj)]
        ).unwrap().l().unwrap();

        let y_val = env.get_field(&vec3_obj, "y", "D").unwrap().d().unwrap();

        if y_val.abs() > 0.00001 {
            return OffsetType::XYZ;
        }
    }

    OffsetType::XZ
}

fn get_state_values(block_state: &JObject, env: &mut JNIEnv) -> IndexMap<String, StateValue> {
    let mut values = IndexMap::new();

    let properties = env
        .call_method(
            block_state,
            "getProperties",
            "()Ljava/util/Collection;",
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    iterate(&properties, env, |_i, property, env| {
        let property_clazz = env
            .get_field(&property, "clazz", "Ljava/lang/Class;")
            .unwrap()
            .l()
            .unwrap();

        let property_clazz_name = env
            .call_method(property_clazz, "getSimpleName", "()Ljava/lang/String;", &[])
            .unwrap()
            .l()
            .unwrap();

        let property_clazz_name = obj_to_str(property_clazz_name, env);

        let value = env.call_method(block_state, "getValue", "(Lnet/minecraft/world/level/block/state/properties/Property;)Ljava/lang/Comparable;", &[JValueGen::Object(&property)]).unwrap().l().unwrap();
        let key_obj = env
            .get_field(&property, "name", "Ljava/lang/String;")
            .unwrap()
            .l()
            .unwrap();
        let key = obj_to_str(key_obj, env);
        let value_obj = env
            .call_method(
                &property,
                "getName",
                "(Ljava/lang/Comparable;)Ljava/lang/String;",
                &[JValueGen::Object(&value)],
            )
            .unwrap()
            .l()
            .unwrap();
        let value_string = obj_to_str(value_obj, env);

        let value = match property_clazz_name.as_str() {
            "Integer" => StateValue::Int(value_string.parse().unwrap()),
            "Boolean" => StateValue::Bool(value_string.parse().unwrap()),
            "String" => {
                panic!("No schema");
            }
            _ => StateValue::String(value_string),
        };
        values.insert(key, value);
    });

    values
}

fn get_block_display_name(env: &mut JNIEnv, block: &JObject) -> String {
    let display_name_component = env
        .call_method(
            block,
            "getName",
            "()Lnet/minecraft/network/chat/MutableComponent;",
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    let display_name_obj = env
        .call_method(
            display_name_component,
            "getString",
            "()Ljava/lang/String;",
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    obj_to_str(display_name_obj, env)
}

fn get_block_name(block: &JObject, block_registry: &JObject, env: &mut JNIEnv) -> String {
    let block_resource_key = env
        .call_method(
            block_registry,
            "getKey",
            "(Ljava/lang/Object;)Lnet/minecraft/resources/ResourceLocation;",
            &[JValueGen::Object(block)],
        )
        .unwrap()
        .l()
        .unwrap();
    get_resource_location_key_name(&block_resource_key, env)
}

fn get_resource_location_key_name(resource_location: &JObject, env: &mut JNIEnv) -> String {
    let name_obj = env
        .get_field(resource_location, "path", "Ljava/lang/String;")
        .unwrap()
        .l()
        .unwrap();
    obj_to_str(name_obj, env)
}

fn obj_to_str(obj: JObject, env: &mut JNIEnv) -> String {
    let jstr = JString::from(obj);
    env.get_string(&jstr).unwrap().into()
}

fn get_block_states(block: &JObject, env: &mut JNIEnv) -> Vec<BlockStateField> {
    let state_definition = env
        .get_field(
            block,
            "stateDefinition",
            "Lnet/minecraft/world/level/block/state/StateDefinition;",
        )
        .unwrap()
        .l()
        .unwrap();

    let properties_map = env
        .get_field(
            state_definition,
            "propertiesByName",
            "Lcom/google/common/collect/ImmutableSortedMap;",
        )
        .unwrap()
        .l()
        .unwrap();

    let properties_map_entry_set = env
        .call_method(
            properties_map,
            "entrySet",
            "()Lcom/google/common/collect/ImmutableSet;",
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    let mut states = Vec::new();

    iterate(&properties_map_entry_set, env, |_i, entry, env| {
        let name_obj = env
            .call_method(&entry, "getKey", "()Ljava/lang/Object;", &[])
            .unwrap()
            .l()
            .unwrap();

        let name = obj_to_str(name_obj, env);

        let property = env
            .call_method(entry, "getValue", "()Ljava/lang/Object;", &[])
            .unwrap()
            .l()
            .unwrap();

        let property_clazz = env
            .get_field(&property, "clazz", "Ljava/lang/Class;")
            .unwrap()
            .l()
            .unwrap();

        let property_clazz_name = env
            .call_method(property_clazz, "getSimpleName", "()Ljava/lang/String;", &[])
            .unwrap()
            .l()
            .unwrap();

        let property_clazz_name = obj_to_str(property_clazz_name, env);

        let values = match property_clazz_name.as_str() {
            "Integer" => {
                let min: u8 = env
                    .get_field(&property, "min", "I")
                    .unwrap()
                    .i()
                    .unwrap()
                    .try_into()
                    .unwrap();
                let max: u8 = env
                    .get_field(&property, "max", "I")
                    .unwrap()
                    .i()
                    .unwrap()
                    .try_into()
                    .unwrap();

                BlockStateFieldValues::Int { min, max }
            }
            "Boolean" => BlockStateFieldValues::Bool,
            "String" => {
                panic!("No schema");
            }
            _ => {
                let possible_values = env
                    .call_method(&property, "getPossibleValues", "()Ljava/util/List;", &[])
                    .unwrap()
                    .l()
                    .unwrap();

                let mut values = Vec::new();

                iterate(&possible_values, env, |_i, value, env| {
                    let value_name_obj = env
                        .call_method(
                            &property,
                            "getName",
                            "(Ljava/lang/Comparable;)Ljava/lang/String;",
                            &[JValueGen::Object(&value)],
                        )
                        .unwrap()
                        .l()
                        .unwrap();

                    let value_name = obj_to_str(value_name_obj, env);

                    values.push(value_name);
                });

                BlockStateFieldValues::Enum {
                    enum_name: property_clazz_name,
                    values,
                }
            }
        };

        states.push(BlockStateField { name, values })
    });

    states
}
