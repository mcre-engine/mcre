use std::{fs, path::PathBuf};

use indexmap::IndexMap;
use jni::{
    Env, JValue, jni_sig, jni_str,
    objects::{JObject, JString},
    strings::JNIString,
};
use mcje::{get_registry, iterate};
use mcre_core::OffsetType;
use mcre_data::{
    biome::{Biome, GrassColorModifier},
    block::{Block, BlockStateField, BlockStateFieldValues},
    shape::OcclusionShape,
    state::{BlockState, StateValue},
};

const BLOCK_DATA_PATH: &str = "crates/mcre_data/blocks.json";
const BLOCK_STATE_DATA_PATH: &str = "crates/mcre_data/block_states.json";
const BIOME_DATA_PATH: &str = "crates/mcre_data/biomes.json";
const SHAPE_DATA_PATH: &str = "crates/mcre_data/shapes.json";

const ICE_TAG: &str = "face_occluding_ice_blocks";
const BARS_TAG: &str = "bars";
const LEAVES_TAG: &str = "leaves";

const ICE_TAG_FALLBACK: &[&str] = &["ice", "brittle_ice", "frosted_ice"];
const BARS_TAG_FALLBACK: &[&str] = &[
    "iron_bars",
    "copper_bars",
    "exposed_copper_bars",
    "weathered_copper_bars",
    "oxidized_copper_bars",
    "waxed_copper_bars",
    "waxed_exposed_copper_bars",
    "waxed_weathered_copper_bars",
    "waxed_oxidized_copper_bars",
];

fn read_block_tag(jar_version: &str, root: &std::path::Path, tag: &str) -> Option<Vec<String>> {
    let jar_path = root
        .join("target/downloads")
        .join(jar_version)
        .join("mc.jar");
    let file = std::fs::File::open(&jar_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive
        .by_name(&format!("data/minecraft/tags/block/{tag}.json"))
        .ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut entry, &mut text).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let values = json.get("values")?.as_array()?;
    let mut members = Vec::new();
    for value in values {
        let name = value.as_str()?;
        if let Some(name) = name.strip_prefix("minecraft:") {
            members.push(name.to_string());
        } else if !name.starts_with('#') {
            members.push(name.to_string());
        }
    }
    Some(members)
}

fn face_occluding_ice_blocks() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let version = fs::read_to_string(root.join("mc-version")).map(|s| s.trim().to_string());
    let result: Option<Vec<String>> = version
        .ok()
        .and_then(|version| read_block_tag(&version, &root, ICE_TAG));
    match result {
        Some(members) => members,
        None => {
            eprintln!(
                "[WARN] Could not read face_occluding_ice_blocks from mc.jar, using fallback {ICE_TAG_FALLBACK:?}"
            );
            ICE_TAG_FALLBACK.iter().map(|s| s.to_string()).collect()
        }
    }
}

fn bars_tag_members() -> Vec<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let version = fs::read_to_string(root.join("mc-version")).map(|s| s.trim().to_string());
    let result: Option<Vec<String>> = version
        .ok()
        .and_then(|version| read_block_tag(&version, &root, BARS_TAG));
    match result {
        Some(members) => members,
        None => {
            eprintln!(
                "[WARN] Could not read bars tag from mc.jar, using fallback {BARS_TAG_FALLBACK:?}"
            );
            BARS_TAG_FALLBACK.iter().map(|s| s.to_string()).collect()
        }
    }
}

fn direction<'a>(env: &mut Env<'a>, name: &str) -> JObject<'a> {
    let jni_name = JNIString::new(name);
    env.get_static_field(
        jni_str!("net/minecraft/core/Direction"),
        &jni_name,
        jni_sig!("Lnet/minecraft/core/Direction;"),
    )
    .unwrap()
    .l()
    .unwrap()
}

fn fluid_name<'a>(
    state: &JObject<'a>,
    fluid_registry: &JObject<'a>,
    env: &mut Env<'a>,
) -> Option<String> {
    let fluid_state = env
        .call_method(
            state,
            jni_str!("getFluidState"),
            jni_sig!("()Lnet/minecraft/world/level/material/FluidState;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    let empty = env
        .call_method(&fluid_state, jni_str!("isEmpty"), jni_sig!("()Z"), &[])
        .unwrap()
        .z()
        .unwrap();
    if empty {
        return None;
    }

    let fluid = env
        .call_method(
            &fluid_state,
            jni_str!("getType"),
            jni_sig!("()Lnet/minecraft/world/level/material/Fluid;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();

    let identifier = env
        .call_method(
            fluid_registry,
            jni_str!("getKey"),
            jni_sig!("(Ljava/lang/Object;)Lnet/minecraft/resources/Identifier;"),
            &[JValue::Object(&fluid)],
        )
        .unwrap()
        .l()
        .unwrap();

    Some(get_identifier_path(&identifier, env))
}

fn skip_rendering<'a>(
    state: &JObject<'a>,
    neighbor: &JObject<'a>,
    dir: &JObject<'a>,
    env: &mut Env<'a>,
) -> bool {
    env.call_method(
        state,
        jni_str!("skipRendering"),
        jni_sig!(
            "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/core/Direction;)Z"
        ),
        &[JValue::Object(neighbor), JValue::Object(dir)],
    )
    .unwrap()
    .z()
    .unwrap()
}

#[mcje::main]
async fn main(env: &mut Env<'_>) {
    generate_block_data(env);
    generate_block_state_data(env);
    generate_biome_data();
}

fn parse_hex_color(json: &serde_json::Value) -> Option<[u8; 3]> {
    let hex = json.as_str()?.strip_prefix('#')?;
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    Some([
        ((rgb >> 16) & 0xFF) as u8,
        ((rgb >> 8) & 0xFF) as u8,
        (rgb & 0xFF) as u8,
    ])
}

fn generate_biome_data() {
    println!("[DEBUG] Generating biome data");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../");
    let version = fs::read_to_string(root.join("mc-version"))
        .map(|s| s.trim().to_string())
        .expect("mc-version readable");
    let jar_path = root.join("target/downloads").join(&version).join("mc.jar");
    let file = std::fs::File::open(&jar_path).expect("mc.jar readable");
    let mut archive = zip::ZipArchive::new(file).expect("mc.jar is a zip");

    let mut names: Vec<String> = archive
        .file_names()
        .filter_map(|name| {
            name.strip_prefix("data/minecraft/worldgen/biome/")
                .and_then(|rest| rest.strip_suffix(".json"))
                .map(str::to_string)
        })
        .collect();
    names.sort();

    let mut biomes = Vec::with_capacity(names.len());
    for (id, name) in names.iter().enumerate() {
        let path = format!("data/minecraft/worldgen/biome/{name}.json");
        let mut entry = archive.by_name(&path).expect("biome entry readable");
        let mut text = String::new();
        std::io::Read::read_to_string(&mut entry, &mut text).expect("biome entry text");
        let json: serde_json::Value = serde_json::from_str(&text).expect("biome json parses");
        let effects = json.get("effects").expect("biome effects present");
        biomes.push(Biome {
            id: id as u16,
            name: name.clone(),
            temperature: json
                .get("temperature")
                .and_then(|v| v.as_f64())
                .expect("biome temperature") as f32,
            downfall: json
                .get("downfall")
                .and_then(|v| v.as_f64())
                .expect("biome downfall") as f32,
            has_precipitation: json
                .get("has_precipitation")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            water_color: effects
                .get("water_color")
                .and_then(parse_hex_color)
                .expect("biome water_color"),
            foliage_color_override: effects.get("foliage_color").and_then(parse_hex_color),
            dry_foliage_color_override: effects.get("dry_foliage_color").and_then(parse_hex_color),
            grass_color_override: effects.get("grass_color").and_then(parse_hex_color),
            grass_color_modifier: match effects.get("grass_color_modifier").and_then(|v| v.as_str())
            {
                None => GrassColorModifier::None,
                Some("dark_forest") => GrassColorModifier::DarkForest,
                Some("swamp") => GrassColorModifier::Swamp,
                Some(other) => panic!("unknown grass_color_modifier: {other}"),
            },
        });
    }
    println!("[DEBUG] Done! {} biomes", biomes.len());

    let json_string = serde_json::to_string_pretty(&biomes).unwrap();
    let root = env!("CARGO_MANIFEST_DIR");
    let root = PathBuf::from(root);
    let data_path = root.join("../../").join(BIOME_DATA_PATH);
    fs::write(&data_path, json_string).unwrap();

    println!("[DEBUG] Biome data saved to `{BIOME_DATA_PATH}`");
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

    // `IceBlock.skipRendering` is the only override that consults the
    // `face_occluding_ice_blocks` tag, so `instanceof IceBlock` exactly detects
    // blocks that skip rendering against tag members.
    let ice_class = env
        .find_class(jni_str!("net/minecraft/world/level/block/IceBlock"))
        .unwrap();
    let leaves_class = env
        .find_class(jni_str!("net/minecraft/world/level/block/LeavesBlock"))
        .unwrap();
    let ice_tag_members = face_occluding_ice_blocks();
    let bars_tag_members = bars_tag_members();
    let fluid_registry = get_registry(env, "FLUID", "DefaultedRegistry");

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

        let liquid = env
            .call_method(&default_state, jni_str!("liquid"), jni_sig!("()Z"), &[])
            .unwrap()
            .z()
            .unwrap();
        let liquid_fluid = if liquid {
            fluid_name(&default_state, &fluid_registry, env)
        } else {
            None
        };

        let skip_against_tags = if env.is_instance_of(&block, &ice_class).unwrap() {
            vec![ICE_TAG.to_string()]
        } else {
            Vec::new()
        };
        let mut culling_tags = Vec::new();
        if ice_tag_members.contains(&name) {
            culling_tags.push(ICE_TAG.to_string());
        }
        if bars_tag_members.contains(&name) {
            culling_tags.push(BARS_TAG.to_string());
        }
        if env.is_instance_of(&block, &leaves_class).unwrap() {
            culling_tags.push(LEAVES_TAG.to_string());
        }

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
            liquid_fluid,
            skip_against_tags,
            culling_tags,
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
    let fluid_registry = get_registry(env, "FLUID", "DefaultedRegistry");

    let block_state_registry = env
        .get_static_field(
            jni_str!("net/minecraft/world/level/block/Block"),
            jni_str!("BLOCK_STATE_REGISTRY"),
            jni_sig!("Lnet/minecraft/core/IdMapper;"),
        )
        .unwrap()
        .l()
        .unwrap();

    let empty_getter = env
        .get_static_field(
            jni_str!("net/minecraft/world/level/EmptyBlockGetter"),
            jni_str!("INSTANCE"),
            jni_sig!("Lnet/minecraft/world/level/EmptyBlockGetter;"),
        )
        .unwrap()
        .l()
        .unwrap();

    let block_pos_zero = env
        .get_static_field(
            jni_str!("net/minecraft/core/BlockPos"),
            jni_str!("ZERO"),
            jni_sig!("Lnet/minecraft/core/BlockPos;"),
        )
        .unwrap()
        .l()
        .unwrap();

    let directions = [
        direction(env, "DOWN"),
        direction(env, "UP"),
        direction(env, "NORTH"),
        direction(env, "SOUTH"),
        direction(env, "WEST"),
        direction(env, "EAST"),
    ];
    for (i, dir) in directions.iter().enumerate() {
        let idx = env
            .call_method(dir, jni_str!("get3DDataValue"), jni_sig!("()I"), &[])
            .unwrap()
            .i()
            .unwrap();
        assert_eq!(idx as usize, i, "vanilla Direction order mismatch");
    }

    let mut ctx = StateGenCtx {
        block_registry,
        fluid_registry,
        empty_getter,
        block_pos_zero,
        directions,
        shapes: std::collections::HashMap::new(),
        shape_list: Vec::new(),
    };

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
        let state = process_block_state(&mut ctx, block_state_id, &block_state, env);
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

    let shapes: Vec<OcclusionShape> = ctx
        .shape_list
        .into_iter()
        .map(|boxes| OcclusionShape { boxes })
        .collect();
    println!("[DEBUG] {} distinct occlusion shapes", shapes.len());
    let json_string = serde_json::to_string_pretty(&shapes).unwrap();
    let data_path = root.join("../../").join(SHAPE_DATA_PATH);
    fs::write(&data_path, json_string).unwrap();

    println!("[DEBUG] Shape data saved to `{SHAPE_DATA_PATH}`");
}

struct StateGenCtx<'a> {
    block_registry: JObject<'a>,
    fluid_registry: JObject<'a>,
    empty_getter: JObject<'a>,
    block_pos_zero: JObject<'a>,
    directions: [JObject<'a>; 6],
    shapes: std::collections::HashMap<Vec<[u64; 6]>, u16>,
    shape_list: Vec<Vec<[f64; 6]>>,
}

fn process_block_state<'a>(
    ctx: &mut StateGenCtx<'a>,
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
            &ctx.block_registry,
            jni_str!("getId"),
            jni_sig!("(Ljava/lang/Object;)I"),
            &[JValue::Object(&block)],
        )
        .unwrap()
        .i()
        .unwrap()
        .try_into()
        .unwrap();

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

    let mut self_skip = 0u8;
    for (i, dir) in ctx.directions.iter().enumerate() {
        if skip_rendering(block_state, block_state, dir, env) {
            self_skip |= 1 << i;
        }
    }

    let collision_full = env
        .call_method(
            block_state,
            jni_str!("isCollisionShapeFullBlock"),
            jni_sig!("(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;)Z"),
            &[
                JValue::Object(&ctx.empty_getter),
                JValue::Object(&ctx.block_pos_zero),
            ],
        )
        .unwrap()
        .z()
        .unwrap();

    let fluid = fluid_name(block_state, &ctx.fluid_registry, env);

    let occlusion_shape = occlusion_shape_id(block_state, ctx, env);

    let emissive_rendering = env
        .call_method(
            block_state,
            jni_str!("emissiveRendering"),
            jni_sig!("()Z"),
            &[],
        )
        .unwrap()
        .z()
        .unwrap();

    let state_values = get_state_values(block_state, env);

    BlockState {
        id,
        block_id,
        light_emission,
        use_shape_for_light_occlusion,
        propagates_skylight_down,
        light_dampening,
        solid_render,
        is_randomly_ticking,
        self_skip,
        collision_full,
        fluid,
        occlusion_shape,
        emissive_rendering,
        state_values,
    }
}

fn occlusion_shape_id<'a>(
    block_state: &JObject<'a>,
    ctx: &mut StateGenCtx<'a>,
    env: &mut Env<'a>,
) -> u16 {
    let shape = env
        .call_method(
            block_state,
            jni_str!("getOcclusionShape"),
            jni_sig!("()Lnet/minecraft/world/phys/shapes/VoxelShape;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();
    let aabbs = env
        .call_method(
            &shape,
            jni_str!("toAabbs"),
            jni_sig!("()Ljava/util/List;"),
            &[],
        )
        .unwrap()
        .l()
        .unwrap();
    let mut boxes = Vec::new();
    iterate(&aabbs, env, |_, aabb, env| {
        let mut field = |name: &str| {
            let jni_name = JNIString::new(name);
            env.get_field(&aabb, &jni_name, jni_sig!("D"))
                .unwrap()
                .d()
                .unwrap()
                + 0.0
        };
        boxes.push([
            field("minX"),
            field("minY"),
            field("minZ"),
            field("maxX"),
            field("maxY"),
            field("maxZ"),
        ]);
    });
    boxes.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let key: Vec<[u64; 6]> = boxes
        .iter()
        .map(|b| {
            [
                b[0].to_bits(),
                b[1].to_bits(),
                b[2].to_bits(),
                b[3].to_bits(),
                b[4].to_bits(),
                b[5].to_bits(),
            ]
        })
        .collect();
    if let Some(id) = ctx.shapes.get(&key) {
        return *id;
    }
    let id = ctx.shape_list.len() as u16;
    assert!(
        ctx.shape_list.len() < u16::MAX as usize,
        "too many distinct occlusion shapes"
    );
    ctx.shapes.insert(key, id);
    ctx.shape_list.push(boxes);
    id
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
