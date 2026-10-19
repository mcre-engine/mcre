use jni::{Env, JValue, jni_sig, jni_str, objects::JObject, strings::JNIString};

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

fn shapes_singleton<'a>(env: &mut Env<'a>, name: &str) -> JObject<'a> {
    env.call_static_method(
        jni_str!("net/minecraft/world/phys/shapes/Shapes"),
        JNIString::new(name),
        jni_sig!("()Lnet/minecraft/world/phys/shapes/VoxelShape;"),
        &[],
    )
    .unwrap()
    .l()
    .unwrap()
}

#[mcje::main]
async fn main(env: &mut Env<'_>) {
    let registry = env
        .get_static_field(
            jni_str!("net/minecraft/world/level/block/Block"),
            jni_str!("BLOCK_STATE_REGISTRY"),
            jni_sig!("Lnet/minecraft/core/IdMapper;"),
        )
        .unwrap()
        .l()
        .unwrap();

    let labels: &[(u16, &str)] = &[
        (1, "stone"),
        (661, "glass"),
        (268, "oak_leaves"),
        (5534, "oak_stairs"),
        (8589, "snow_1"),
        (8668, "oak_fence"),
        (8801, "oak_trapdoor"),
        (10067, "glass_pane"),
        (13082, "hopper"),
        (15180, "oak_slab_top"),
        (15182, "oak_slab_bottom"),
        (30537, "powder_snow"),
        (32896, "copper_grate"),
    ];

    let mut states = Vec::new();
    for (id, label) in labels {
        let state = env
            .call_method(
                &registry,
                jni_str!("byId"),
                jni_sig!("(I)Ljava/lang/Object;"),
                &[JValue::Int((*id).into())],
            )
            .unwrap()
            .l()
            .unwrap();
        assert!(!state.is_null(), "state {id} ({label}) not found");
        states.push((state, *label));
    }

    let directions = [
        direction(env, "DOWN"),
        direction(env, "UP"),
        direction(env, "NORTH"),
        direction(env, "SOUTH"),
        direction(env, "WEST"),
        direction(env, "EAST"),
    ];

    let block_cube = shapes_singleton(env, "block");
    let empty_shape = shapes_singleton(env, "empty");

    println!("[PROBE] face shapes");
    for (state, label) in &states {
        for (dir_idx, dir) in directions.iter().enumerate() {
            let face = env
                .call_method(
                    state,
                    jni_str!("getFaceOcclusionShape"),
                    jni_sig!(
                        "(Lnet/minecraft/core/Direction;)Lnet/minecraft/world/phys/shapes/VoxelShape;"
                    ),
                    &[JValue::Object(dir)],
                )
                .unwrap()
                .l()
                .unwrap();
            let kind = if env.is_same_object(&face, &block_cube).unwrap() {
                "cube"
            } else if env.is_same_object(&face, &empty_shape).unwrap() {
                "empty"
            } else {
                "other"
            };
            println!("[PROBE] face {label} {dir_idx} {kind}");
        }
    }

    println!("[PROBE] shouldRenderFace");
    for (state, state_label) in &states {
        for (neighbor, neighbor_label) in &states {
            let mut mask = 0u8;
            for (dir_idx, dir) in directions.iter().enumerate() {
                let result = env
                    .call_static_method(
                        jni_str!("net/minecraft/world/level/block/Block"),
                        jni_str!("shouldRenderFace"),
                        jni_sig!(
                            "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/core/Direction;)Z"
                        ),
                        &[
                            JValue::Object(state),
                            JValue::Object(neighbor),
                            JValue::Object(dir),
                        ],
                    )
                    .unwrap()
                    .z()
                    .unwrap();
                if result {
                    mask |= 1 << dir_idx;
                }
            }
            println!("[PROBE] srf {state_label} {neighbor_label} {mask:02x}");
        }
    }

    println!("[PROBE] fluid occlusion");
    let heights = [0.5_f64, 0.8888889, 1.0];
    for (state, label) in &states {
        for (dir_idx, dir) in directions.iter().enumerate() {
            let opposite = env
                .call_method(
                    dir,
                    jni_str!("getOpposite"),
                    jni_sig!("()Lnet/minecraft/core/Direction;"),
                    &[],
                )
                .unwrap()
                .l()
                .unwrap();
            let occluder = env
                .call_method(
                    state,
                    jni_str!("getFaceOcclusionShape"),
                    jni_sig!(
                        "(Lnet/minecraft/core/Direction;)Lnet/minecraft/world/phys/shapes/VoxelShape;"
                    ),
                    &[JValue::Object(&opposite)],
                )
                .unwrap()
                .l()
                .unwrap();
            let empty = env.is_same_object(&occluder, &empty_shape).unwrap();
            let cube = env.is_same_object(&occluder, &block_cube).unwrap();
            let mut mask = 0u8;
            for (height_idx, height) in heights.iter().enumerate() {
                let occluded = if empty {
                    false
                } else if cube {
                    dir_idx != 1 || *height == 1.0
                } else {
                    let shape = env
                        .call_static_method(
                            jni_str!("net/minecraft/world/phys/shapes/Shapes"),
                            jni_str!("box"),
                            jni_sig!("(DDDDDD)Lnet/minecraft/world/phys/shapes/VoxelShape;"),
                            &[
                                JValue::Double(0.0),
                                JValue::Double(0.0),
                                JValue::Double(0.0),
                                JValue::Double(1.0),
                                JValue::Double(*height),
                                JValue::Double(1.0),
                            ],
                        )
                        .unwrap()
                        .l()
                        .unwrap();
                    env.call_static_method(
                        jni_str!("net/minecraft/world/phys/shapes/Shapes"),
                        jni_str!("blockOccludes"),
                        jni_sig!(
                            "(Lnet/minecraft/world/phys/shapes/VoxelShape;Lnet/minecraft/world/phys/shapes/VoxelShape;Lnet/minecraft/core/Direction;)Z"
                        ),
                        &[
                            JValue::Object(&shape),
                            JValue::Object(&occluder),
                            JValue::Object(dir),
                        ],
                    )
                    .unwrap()
                    .z()
                    .unwrap()
                };
                if occluded {
                    mask |= 1 << height_idx;
                }
            }
            println!("[PROBE] fluid-occ {label} {dir_idx} {mask:02x}");
        }
    }
}
