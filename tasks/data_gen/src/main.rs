use std::{fs, path::PathBuf};

use indexmap::IndexMap;
use jni::{
    Env, JValue, jni_sig, jni_str,
    objects::{JObject, JString},
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
async fn main(env: &mut Env<'_>) {
    generate_block_data(env);
    generate_block_state_data(env);
}

fn generate_block_data<'a>(env: &mut Env<'a>) {
    println!("[DEBUG] Generating block data");
    let block_registry = get_registry(env, "BLOCK", "DefaultedRegistry");

    let block_state_registry = env
        .get_static_field(
            jni_str!("net/minecraft/world/level/block/Block"),
            jni_str!("BLOCK_STATE_REGISTRY"),
            jni_sig!("Lnet/minecraft/core/IdMapper;"),
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
                jni_str!("defaultBlockState"),
                jni_sig!("Lnet/minecraft/world/level/block/state/BlockState;"),
            )
            .unwrap()
            .l()
            .unwrap();

        let default_state_id = env
            .call_static_method(
                jni_str!("net/minecraft/world/level/block/Block"),
                jni_str!("getId"),
                jni_sig!("(Lnet/minecraft/world/level/block/state/BlockState;)I"),
                &[JValue::Object(&default_state)],
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
                    jni_str!("byId"),
                    jni_sig!("(I)Ljava/lang/Object;"),
                    &[JValue::Int(block_state_id_counter as i32)],
                )
                .unwrap()
                .l()
                .unwrap();
            if state.is_null() {
                break;
            }
            let state_owner = env
                .get_field(state, jni_str!("owner"), jni_sig!("Ljava/lang/Object;"))
                .unwrap()
                .l()
                .unwrap();
            let state_owner_name = get_block_name(&state_owner, &block_registry, env);
            owner = state_owner_name;
        }

        let max_state_id = block_state_id_counter - 1;

        let has_collision = env
            .get_field(&block, jni_str!("hasCollision"), jni_sig!("Z"))
            .unwrap()
            .z()
            .unwrap();

        let explosion_resistance = env
            .get_field(&block, jni_str!("explosionResistance"), jni_sig!("F"))
            .unwrap()
            .f()
            .unwrap();

        let friction = env
            .get_field(&block, jni_str!("friction"), jni_sig!("F"))
            .unwrap()
            .f()
            .unwrap();

        let speed_factor = env
            .get_field(&block, jni_str!("speedFactor"), jni_sig!("F"))
            .unwrap()
            .f()
            .unwrap();

        let jump_factor = env
            .get_field(&block, jni_str!("jumpFactor"), jni_sig!("F"))
            .unwrap()
            .f()
            .unwrap();

        let bounce_restitution = env
            .get_field(&block, jni_str!("bounceRestitution"), jni_sig!("F"))
            .unwrap()
            .f()
            .unwrap();

        let fall_distance_reduction = env
            .get_field(&block, jni_str!("fallDistanceReduction"), jni_sig!("F"))
            .unwrap()
            .f()
            .unwrap();

        let properties = env
            .get_field(
                &block,
                jni_str!("properties"),
                jni_sig!("Lnet/minecraft/world/level/block/state/BlockBehaviour$Properties;"),
            )
            .unwrap()
            .l()
            .unwrap();

        let push_reaction_obj = env
            .get_field(
                &properties,
                jni_str!("pushReaction"),
                jni_sig!("Lnet/minecraft/world/level/material/PushReaction;"),
            )
            .unwrap()
            .l()
            .unwrap();
        let push_reaction = {
            let name_obj = env
                .call_method(
                    &push_reaction_obj,
                    jni_str!("name"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            obj_to_str(name_obj, env).to_lowercase()
        };

        let instrument_obj = env
            .get_field(
                &properties,
                jni_str!("instrument"),
                jni_sig!("Lnet/minecraft/world/level/block/state/properties/NoteBlockInstrument;"),
            )
            .unwrap()
            .l()
            .unwrap();
        let instrument = if instrument_obj.is_null() {
            String::new()
        } else {
            let serializable = env
                .call_method(
                    &instrument_obj,
                    jni_str!("getSerializedName"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            obj_to_str(serializable, env)
        };

        let destroy_speed = env
            .get_field(&default_state, jni_str!("destroySpeed"), jni_sig!("F"))
            .unwrap()
            .f()
            .unwrap();

        let requires_correct_tool_for_drops = env
            .get_field(
                &default_state,
                jni_str!("requiresCorrectToolForDrops"),
                jni_sig!("Z"),
            )
            .unwrap()
            .z()
            .unwrap();

        let can_occlude = env
            .get_field(&default_state, jni_str!("canOcclude"), jni_sig!("Z"))
            .unwrap()
            .z()
            .unwrap();

        let ignited_by_lava = env
            .get_field(&default_state, jni_str!("ignitedByLava"), jni_sig!("Z"))
            .unwrap()
            .z()
            .unwrap();

        let is_air = env
            .get_field(&default_state, jni_str!("isAir"), jni_sig!("Z"))
            .unwrap()
            .z()
            .unwrap();

        let spawn_terrain_particles = env
            .get_field(
                &default_state,
                jni_str!("spawnTerrainParticles"),
                jni_sig!("Z"),
            )
            .unwrap()
            .z()
            .unwrap();

        let replaceable = env
            .get_field(&default_state, jni_str!("replaceable"), jni_sig!("Z"))
            .unwrap()
            .z()
            .unwrap();

        let offset_type = determine_offset_type(&default_state, env);

        let max_horizontal_offset = env
            .call_method(
                &block,
                jni_str!("getMaxHorizontalOffset"),
                jni_sig!("()F"),
                &[],
            )
            .unwrap()
            .f()
            .unwrap();

        let max_vertical_offset = env
            .call_method(
                &block,
                jni_str!("getMaxVerticalOffset"),
                jni_sig!("()F"),
                &[],
            )
            .unwrap()
            .f()
            .unwrap();

        blocks.push(Block {
            id: i as u16,
            name,
            display_name,
            default_state_id,
            min_state_id,
            max_state_id,
            states,
            has_collision,
            explosion_resistance,
            friction,
            speed_factor,
            jump_factor,
            bounce_restitution,
            fall_distance_reduction,
            push_reaction,
            instrument,
            destroy_speed,
            requires_correct_tool_for_drops,
            can_occlude,
            ignited_by_lava,
            is_air,
            spawn_terrain_particles,
            replaceable,
            offset_type,
            max_horizontal_offset,
            max_vertical_offset,
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

fn generate_block_state_data<'a>(env: &mut Env<'a>) {
    println!("[DEBUG] Generating block state data");
    let block_registry = get_registry(env, "BLOCK", "DefaultedRegistry");

    let block_state_registry = env
        .get_static_field(
            jni_str!("net/minecraft/world/level/block/Block"),
            jni_str!("BLOCK_STATE_REGISTRY"),
            jni_sig!("Lnet/minecraft/core/IdMapper;"),
        )
        .unwrap()
        .l()
        .unwrap();

    let mut block_state = env
        .call_method(
            &block_state_registry,
            jni_str!("byId"),
            jni_sig!("(I)Ljava/lang/Object;"),
            &[JValue::Int(0)],
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
                jni_str!("byId"),
                jni_sig!("(I)Ljava/lang/Object;"),
                &[JValue::Int(block_state_id.into())],
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

fn process_block_state<'a>(
    block_registry: &JObject<'a>,
    id: u16,
    block_state: &JObject<'a>,
    env: &mut Env<'a>,
) -> BlockState {
    let block = env
        .get_field(
            block_state,
            jni_str!("owner"),
            jni_sig!("Ljava/lang/Object;"),
        )
        .unwrap()
        .l()
        .unwrap();

    let block_id: u16 = env
        .call_method(
            block_registry,
            jni_str!("getId"),
            jni_sig!("(Ljava/lang/Object;)I"),
            &[JValue::Object(&block)],
        )
        .unwrap()
        .i()
        .unwrap()
        .try_into()
        .unwrap();

    let block_name = get_block_name(&block, block_registry, env);

    let light_emission: u8 = env
        .get_field(block_state, jni_str!("lightEmission"), jni_sig!("I"))
        .unwrap()
        .i()
        .unwrap()
        .try_into()
        .unwrap();

    let use_shape_for_light_occlusion = env
        .get_field(
            block_state,
            jni_str!("useShapeForLightOcclusion"),
            jni_sig!("Z"),
        )
        .unwrap()
        .z()
        .unwrap();

    let propagates_skylight_down = env
        .get_field(
            block_state,
            jni_str!("propagatesSkylightDown"),
            jni_sig!("Z"),
        )
        .unwrap()
        .z()
        .unwrap();

    let light_dampening: u8 = env
        .get_field(block_state, jni_str!("lightDampening"), jni_sig!("I"))
        .unwrap()
        .i()
        .unwrap()
        .try_into()
        .unwrap();

    let solid_render = env
        .get_field(block_state, jni_str!("solidRender"), jni_sig!("Z"))
        .unwrap()
        .z()
        .unwrap();

    let is_randomly_ticking = env
        .get_field(block_state, jni_str!("isRandomlyTicking"), jni_sig!("Z"))
        .unwrap()
        .z()
        .unwrap();

    let state_values = get_state_values(block_state, env);

    BlockState {
        id,
        block_id,
        block_name,
        light_emission,
        use_shape_for_light_occlusion,
        propagates_skylight_down,
        light_dampening,
        solid_render,
        is_randomly_ticking,
        state_values,
    }
}

pub fn determine_offset_type(block_state: &JObject, env: &mut Env) -> OffsetType {
    let offset_function = env
        .get_field(
            block_state,
            jni_str!("offsetFunction"),
            jni_sig!("Lnet/minecraft/world/level/block/state/BlockBehaviour$OffsetFunction;"),
        )
        .unwrap()
        .l()
        .unwrap();

    if offset_function.is_null() {
        return OffsetType::None;
    }

    for i in 0..10 {
        // BlockPos(i, i, i)
        let pos_obj = env
            .new_object(
                jni_str!("net/minecraft/core/BlockPos"),
                jni_sig!("(III)V"),
                &[JValue::Int(i), JValue::Int(i), JValue::Int(i)],
            )
            .unwrap();

        // Call offsetFunction.evaluate(state, pos)
        // Signature: (LBlockState;LBlockPos;)LVec3;
        let vec3_obj = env.call_method(
            &offset_function,
            jni_str!("evaluate"),
            jni_sig!("(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/phys/Vec3;"),
            &[JValue::Object(block_state), JValue::Object(&pos_obj)]
        ).unwrap().l().unwrap();

        let y_val = env
            .get_field(&vec3_obj, jni_str!("y"), jni_sig!("D"))
            .unwrap()
            .d()
            .unwrap();

        if y_val.abs() > 0.00001 {
            return OffsetType::XYZ;
        }
    }

    OffsetType::XZ
}

fn get_state_values<'a>(
    block_state: &JObject<'a>,
    env: &mut Env<'a>,
) -> IndexMap<String, StateValue> {
    let mut values = IndexMap::new();

    let properties = env
        .call_method(
            block_state,
            jni_str!("getProperties"),
            jni_sig!("()Ljava/util/Collection;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    iterate(&properties, env, |_i, property, env| {
        let property_clazz = env
            .get_field(&property, jni_str!("clazz"), jni_sig!("Ljava/lang/Class;"))
            .unwrap()
            .l()
            .unwrap();

        let property_clazz_name = env
            .call_method(
                property_clazz,
                jni_str!("getSimpleName"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .unwrap()
            .l()
            .unwrap();

        let property_clazz_name = obj_to_str(property_clazz_name, env);

        let value = env.call_method(block_state, jni_str!("getValue"), jni_sig!("(Lnet/minecraft/world/level/block/state/properties/Property;)Ljava/lang/Comparable;"), &[JValue::Object(&property)]).unwrap().l().unwrap();
        let key_obj = env
            .get_field(&property, jni_str!("name"), jni_sig!("Ljava/lang/String;"))
            .unwrap()
            .l()
            .unwrap();
        let key = obj_to_str(key_obj, env);
        let value_obj = env
            .call_method(
                &property,
                jni_str!("getName"),
                jni_sig!("(Ljava/lang/Comparable;)Ljava/lang/String;"),
                &[JValue::Object(&value)],
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

fn get_block_display_name(env: &mut Env, block: &JObject) -> String {
    let display_name_component = env
        .call_method(
            block,
            jni_str!("getName"),
            jni_sig!("()Lnet/minecraft/network/chat/MutableComponent;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    let display_name_obj = env
        .call_method(
            display_name_component,
            jni_str!("getString"),
            jni_sig!("()Ljava/lang/String;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    obj_to_str(display_name_obj, env)
}

fn get_block_name(block: &JObject, block_registry: &JObject, env: &mut Env) -> String {
    let block_identifier = env
        .call_method(
            block_registry,
            jni_str!("getKey"),
            jni_sig!("(Ljava/lang/Object;)Lnet/minecraft/resources/Identifier;"),
            &[JValue::Object(block)],
        )
        .unwrap()
        .l()
        .unwrap();
    get_identifier_path(&block_identifier, env)
}

fn get_identifier_path(identifier: &JObject, env: &mut Env) -> String {
    let path = env
        .get_field(identifier, jni_str!("path"), jni_sig!("Ljava/lang/String;"))
        .unwrap()
        .l()
        .unwrap();
    obj_to_str(path, env)
}

fn obj_to_str(obj: JObject, env: &mut Env) -> String {
    let jstr = env.as_cast::<JString>(&obj).unwrap();
    jstr.try_to_string(env).unwrap()
}

fn get_block_states<'a>(block: &JObject<'a>, env: &mut Env<'a>) -> Vec<BlockStateField> {
    let state_definition = env
        .get_field(
            block,
            jni_str!("stateDefinition"),
            jni_sig!("Lnet/minecraft/world/level/block/state/StateDefinition;"),
        )
        .unwrap()
        .l()
        .unwrap();

    let properties_map = env
        .get_field(
            state_definition,
            jni_str!("propertiesByName"),
            jni_sig!("Lcom/google/common/collect/ImmutableSortedMap;"),
        )
        .unwrap()
        .l()
        .unwrap();

    let properties_map_entry_set = env
        .call_method(
            properties_map,
            jni_str!("entrySet"),
            jni_sig!("()Lcom/google/common/collect/ImmutableSet;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    let mut states = Vec::new();

    iterate(&properties_map_entry_set, env, |_i, entry, env| {
        let name_obj = env
            .call_method(
                &entry,
                jni_str!("getKey"),
                jni_sig!("()Ljava/lang/Object;"),
                &[],
            )
            .unwrap()
            .l()
            .unwrap();

        let name = obj_to_str(name_obj, env);

        let property = env
            .call_method(
                entry,
                jni_str!("getValue"),
                jni_sig!("()Ljava/lang/Object;"),
                &[],
            )
            .unwrap()
            .l()
            .unwrap();

        let property_clazz = env
            .get_field(&property, jni_str!("clazz"), jni_sig!("Ljava/lang/Class;"))
            .unwrap()
            .l()
            .unwrap();

        let property_clazz_name = env
            .call_method(
                property_clazz,
                jni_str!("getSimpleName"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )
            .unwrap()
            .l()
            .unwrap();

        let property_clazz_name = obj_to_str(property_clazz_name, env);

        let values = match property_clazz_name.as_str() {
            "Integer" => {
                let min: u8 = env
                    .get_field(&property, jni_str!("min"), jni_sig!("I"))
                    .unwrap()
                    .i()
                    .unwrap()
                    .try_into()
                    .unwrap();
                let max: u8 = env
                    .get_field(&property, jni_str!("max"), jni_sig!("I"))
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
                    .call_method(
                        &property,
                        jni_str!("getPossibleValues"),
                        jni_sig!("()Ljava/util/List;"),
                        &[],
                    )
                    .unwrap()
                    .l()
                    .unwrap();

                let mut values = Vec::new();

                let iterable = env
                    .call_method(
                        &possible_values,
                        jni_str!("iterator"),
                        jni_sig!("()Ljava/util/Iterator;"),
                        &[],
                    )
                    .unwrap()
                    .l()
                    .unwrap();
                loop {
                    let has_next = env
                        .call_method(&iterable, jni_str!("hasNext"), jni_sig!("()Z"), &[])
                        .unwrap()
                        .z()
                        .unwrap();
                    if !has_next {
                        break;
                    }
                    let value = env
                        .call_method(
                            &iterable,
                            jni_str!("next"),
                            jni_sig!("()Ljava/lang/Object;"),
                            &[],
                        )
                        .unwrap()
                        .l()
                        .unwrap();
                    let value_name_obj = env
                        .call_method(
                            &property,
                            jni_str!("getName"),
                            jni_sig!("(Ljava/lang/Comparable;)Ljava/lang/String;"),
                            &[JValue::Object(&value)],
                        )
                        .unwrap()
                        .l()
                        .unwrap();

                    let value_name = obj_to_str(value_name_obj, env);

                    values.push(value_name);
                }

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
