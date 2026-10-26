use core::{fmt, ops::Deref, slice};

use alloc::{
    boxed::Box,
    vec::{self, Vec},
};
use mcre_core::Direction;
use mcre_world::{BlockState, PropFilter, PropVal};
use serde::{Deserialize, Deserializer};

use crate::{BakedQuad, BlockModelId};

/// Represents a single variant entry from the "variants" map.
#[derive(Debug, Clone)]
pub struct VariantEntry {
    /// Parsed from the JSON map key (e.g., "face=ceiling,facing=east")
    pub filter: Vec<PropVal>,
    /// The value of the JSON map entry
    pub definition: VariantDefinition,
}

/// A wrapper struct to handle deserializing a JSON Map into a `Vec<VariantEntry>`
/// while preserving the order of entry appearance in the file.
#[derive(Debug, Clone)]
pub struct VariantEntries(pub Vec<VariantEntry>);

// Allow accessing the inner Vec easily
impl Deref for VariantEntries {
    type Target = Vec<VariantEntry>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntoIterator for VariantEntries {
    type Item = VariantEntry;
    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum BlockStateDefinition {
    /// Deserializes from a JSON Map, but stores as a `Vec<VariantEntry>`.
    Variants(VariantEntries),
    Multipart(Vec<MultipartRule>),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum VariantDefinition {
    Single(ModelVariant),
    Multiple(Vec<ModelVariant>),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ModelVariant {
    pub model: BlockModelId,
    #[serde(default)]
    pub uvlock: bool,
    #[serde(default = "default_weight")]
    pub weight: u8,
    #[serde(default)]
    pub x: RotationDegrees,
    #[serde(default)]
    pub y: RotationDegrees,
    #[serde(default)]
    pub z: RotationDegrees,
}

fn default_weight() -> u8 {
    1
}

#[derive(Default, Debug, Copy, Clone)]
pub enum RotationDegrees {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MultipartRule {
    pub apply: VariantDefinition,
    #[serde(default)]
    pub when: Option<Condition>,
}

#[derive(Debug, Clone)]
pub enum Condition {
    KeyValue(PropFilter),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

impl RotationDegrees {
    pub fn quadrant(self) -> usize {
        match self {
            Self::R0 => 0,
            Self::R90 => 1,
            Self::R180 => 2,
            Self::R270 => 3,
        }
    }

    fn degrees(self) -> f32 {
        match self {
            Self::R0 => 0.0,
            Self::R90 => 90.0,
            Self::R180 => 180.0,
            Self::R270 => 270.0,
        }
    }
}

type Mat3 = [[f32; 3]; 3];

const IDENTITY: Mat3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn mat3_mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

fn mat3_apply(m: &Mat3, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn mat3_transpose(m: &Mat3) -> Mat3 {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

fn snapped_sin_cos(degrees: f32) -> (f32, f32) {
    // No float methods in `no_std` on this toolchain: libm instead.
    // Cardinal angles stay bit-exact; general angles match platform trig
    // within 1 ulp (good enough for model rotations).
    let bent = degrees - 360.0 * libm::floorf(degrees / 360.0);
    if libm::fabsf(bent) < 1e-6 {
        (0.0, 1.0)
    } else if libm::fabsf(bent - 90.0) < 1e-6 {
        (1.0, 0.0)
    } else if libm::fabsf(bent - 180.0) < 1e-6 {
        (0.0, -1.0)
    } else if libm::fabsf(bent - 270.0) < 1e-6 {
        (-1.0, 0.0)
    } else {
        let radians = degrees * (core::f32::consts::PI / 180.0);
        (libm::sinf(radians), libm::cosf(radians))
    }
}

fn rot_x(degrees: f32) -> Mat3 {
    let (sin, cos) = snapped_sin_cos(degrees);
    [[1.0, 0.0, 0.0], [0.0, cos, -sin], [0.0, sin, cos]]
}

fn rot_y(degrees: f32) -> Mat3 {
    let (sin, cos) = snapped_sin_cos(degrees);
    [[cos, 0.0, sin], [0.0, 1.0, 0.0], [-sin, 0.0, cos]]
}

fn rot_z(degrees: f32) -> Mat3 {
    let (sin, cos) = snapped_sin_cos(degrees);
    [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]]
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

fn local_to_global(dir: Direction) -> Mat3 {
    match dir {
        Direction::South => IDENTITY,
        Direction::East => rot_y(90.0),
        Direction::West => rot_y(-90.0),
        Direction::North => rot_y(180.0),
        Direction::Up => rot_x(-90.0),
        Direction::Down => rot_x(90.0),
    }
}

fn closest_direction(v: [f32; 3]) -> Direction {
    let mut best = Direction::South;
    let mut best_dot = f32::NEG_INFINITY;
    for dir in Direction::ALL {
        let u = dir_unit(dir);
        let dot = v[0] * u[0] + v[1] * u[1] + v[2] * u[2];
        if dot > best_dot {
            best_dot = dot;
            best = dir;
        }
    }
    best
}

impl ModelVariant {
    pub fn transform_quads(&self, quads: &mut [BakedQuad]) {
        let rotation = self.model_rotation();
        if rotation == IDENTITY {
            return;
        }

        let center = [8.0, 8.0, 8.0];
        for quad in quads {
            for vertex in &mut quad.vertices {
                let rel = [
                    vertex[0] - center[0],
                    vertex[1] - center[1],
                    vertex[2] - center[2],
                ];
                let rotated = mat3_apply(&rotation, rel);
                vertex[0] = rotated[0] + center[0];
                vertex[1] = rotated[1] + center[1];
                vertex[2] = rotated[2] + center[2];
            }

            quad.direction = closest_direction(mat3_apply(&rotation, dir_unit(quad.direction)));
            if let Some(cullface) = quad.cullface {
                quad.cullface = Some(closest_direction(mat3_apply(&rotation, dir_unit(cullface))));
            }

            if self.uvlock {
                let uv_transform = Self::uv_transform(rotation, quad.direction);
                for uv in &mut quad.uvs {
                    let rel = [uv[0] - 8.0, uv[1] - 8.0, 0.0];
                    let rotated = mat3_apply(&uv_transform, rel);
                    uv[0] = rotated[0] + 8.0;
                    uv[1] = rotated[1] + 8.0;
                }
            }
        }
    }

    fn model_rotation(&self) -> Mat3 {
        let x = rot_x(-self.x.degrees());
        let y = rot_y(-self.y.degrees());
        let z = rot_z(-self.z.degrees());
        mat3_mul(&z, &mat3_mul(&y, &x))
    }

    fn uv_transform(rotation: Mat3, face: Direction) -> Mat3 {
        let face_action = mat3_mul(&rotation, &local_to_global(face));
        let normal = mat3_apply(&face_action, [0.0, 0.0, 1.0]);
        let new_side = closest_direction(normal);
        let face_mapping = mat3_mul(&mat3_transpose(&local_to_global(new_side)), &face_action);
        mat3_transpose(&face_mapping)
    }
}

impl Condition {
    pub fn test(&self, state: BlockState) -> bool {
        match self {
            Condition::KeyValue(filter) => {
                if let Some(val) = state.get_prop(filter.key()) {
                    filter.test(val)
                } else {
                    false
                }
            }
            Condition::And(conditions) => conditions.iter().all(|condition| condition.test(state)),
            Condition::Or(conditions) => conditions.iter().any(|condition| condition.test(state)),
        }
    }
}

pub enum BlockModelResolution<'a> {
    Unified(&'a [ModelVariant]),
    Multipart(Box<[&'a [ModelVariant]]>),
}

impl BlockStateDefinition {
    pub fn resolve<'a>(&'a self, state: BlockState) -> Option<BlockModelResolution<'a>> {
        match self {
            Self::Variants(variants) => {
                'variant_loop: for variant in variants.iter() {
                    for filter in &variant.filter {
                        let Some(val) = state.get_prop(filter.key()) else {
                            continue 'variant_loop;
                        };

                        if val != *filter {
                            continue 'variant_loop;
                        };
                    }

                    let models = match &variant.definition {
                        VariantDefinition::Single(model) => slice::from_ref(model),
                        VariantDefinition::Multiple(models) => models,
                    };

                    return Some(BlockModelResolution::Unified(models));
                }

                None
            }
            Self::Multipart(rules) => {
                let mut resolved_models = Vec::new();

                for rule in rules {
                    let condition_met = if let Some(condition) = &rule.when {
                        condition.test(state)
                    } else {
                        true
                    };

                    if condition_met {
                        match &rule.apply {
                            VariantDefinition::Single(model) => {
                                resolved_models.push(slice::from_ref(model))
                            }
                            VariantDefinition::Multiple(models) => resolved_models.push(models),
                        }
                    }
                }

                Some(BlockModelResolution::Multipart(
                    resolved_models.into_boxed_slice(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mcre_world::BlockState;

    use crate::blockstates::{BlockModelResolution, BlockStateDefinition};
    use std::{
        collections::HashMap,
        fs::{self, File},
        path::PathBuf,
    };

    #[tokio::test]
    async fn test_parse_and_resolve_block_state_definition() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let manifest_dir = PathBuf::from(manifest_dir);
        let target_mc_version = std::fs::read_to_string(manifest_dir.join("../../mc-version"))
            .unwrap()
            .trim()
            .to_string();
        let root_dir = manifest_dir
            .join("../../target/downloads")
            .join(&target_mc_version)
            .join("assets/minecraft/blockstates");

        let mut total = 0;
        let mut passed = 0;
        let mut failed = Vec::new();

        let mut block_state_definitions = HashMap::new();

        for entry in fs::read_dir(&root_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            // TODO(a-rustacean): are item frames blocks?
            if path.ends_with("item_frame.json") || path.ends_with("glow_item_frame.json") {
                continue;
            }
            total += 1;
            let file = File::open(&path).unwrap();

            let file_name = path.file_name().unwrap().to_str().unwrap();
            let name = file_name.strip_suffix(".json").unwrap().to_string();

            let result: Result<BlockStateDefinition, _> = serde_json::from_reader(file);

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

        // resolution

        for state in BlockState::all() {
            let definition = block_state_definitions.get(state.block().name()).unwrap();
            let resolution = definition.resolve(state).unwrap();

            match resolution {
                BlockModelResolution::Unified(models) => assert!(
                    !models.is_empty(),
                    "Block: {}, StateId: {:?}",
                    state.block().name(),
                    state
                ),
                BlockModelResolution::Multipart(model_lists) => assert!(
                    state.block().name().ends_with("wall") || !model_lists.is_empty(),
                    "Block: {}, StateId: {:?}",
                    state.block().name(),
                    state
                ),
            }
        }
    }

    #[test]
    fn quadrant_rotations_use_exact_trig() {
        use super::snapped_sin_cos;
        assert_eq!(snapped_sin_cos(0.0), (0.0, 1.0));
        assert_eq!(snapped_sin_cos(90.0), (1.0, 0.0));
        assert_eq!(snapped_sin_cos(180.0), (0.0, -1.0));
        assert_eq!(snapped_sin_cos(270.0), (-1.0, 0.0));
        assert_eq!(snapped_sin_cos(-90.0), (-1.0, 0.0));
        assert_eq!(snapped_sin_cos(-180.0), (0.0, -1.0));
        // Non-cardinal angles go through libm, whose rounding can differ
        // from platform trig by 1 ulp; compare approximately.
        let (sin, cos) = snapped_sin_cos(45.0);
        let (expected_sin, expected_cos) = 45.0_f32.to_radians().sin_cos();
        assert!((sin - expected_sin).abs() < 1e-6);
        assert!((cos - expected_cos).abs() < 1e-6);
    }
}

mod de_impl {
    use core::str::FromStr;

    use crate::FxHashMap;

    use super::*;
    use alloc::string::String;
    use mcre_world::PropKey;
    use serde::{
        Deserialize,
        de::{self, MapAccess, Unexpected, Visitor},
    };

    impl<'de> Deserialize<'de> for RotationDegrees {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            struct RotationVisitor;

            impl<'de> Visitor<'de> for RotationVisitor {
                type Value = RotationDegrees;

                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("an integer rotation: 0, 90, 180, or 270")
                }

                fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    match value {
                        0 => Ok(RotationDegrees::R0),
                        90 => Ok(RotationDegrees::R90),
                        180 => Ok(RotationDegrees::R180),
                        270 => Ok(RotationDegrees::R270),
                        _ => Err(E::invalid_value(Unexpected::Signed(value), &self)),
                    }
                }

                fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    match value {
                        0 => Ok(RotationDegrees::R0),
                        90 => Ok(RotationDegrees::R90),
                        180 => Ok(RotationDegrees::R180),
                        270 => Ok(RotationDegrees::R270),
                        _ => Err(E::invalid_value(Unexpected::Unsigned(value), &self)),
                    }
                }
            }

            deserializer.deserialize_i64(RotationVisitor)
        }
    }

    impl<'de> Deserialize<'de> for Condition {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            #[derive(Debug, Clone)]
            struct SingleCond(String, String);

            impl<'de> Deserialize<'de> for SingleCond {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    struct SingleCondVisitor;

                    impl<'de> Visitor<'de> for SingleCondVisitor {
                        type Value = SingleCond;

                        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                            f.write_str("a single key-value condition like {\"facing\": \"north\"}")
                        }

                        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
                        where
                            M: MapAccess<'de>,
                        {
                            let (key, value): (String, String) = map
                                .next_entry()?
                                .ok_or_else(|| de::Error::custom("condition cannot be empty"))?;

                            if map.next_entry::<String, String>()?.is_some() {
                                return Err(de::Error::custom(
                                    "condition must contain exactly one entry",
                                ));
                            }

                            Ok(SingleCond(key, value))
                        }
                    }

                    deserializer.deserialize_map(SingleCondVisitor)
                }
            }

            #[derive(Deserialize)]
            #[serde(untagged)]
            enum Helper {
                ExplicitAnd {
                    #[serde(rename = "AND")]
                    and: Vec<Helper>,
                },
                Or {
                    #[serde(rename = "OR")]
                    or: Vec<Helper>,
                },
                Single(SingleCond),
                ImplicitAnd(FxHashMap<String, String>),
            }

            impl TryFrom<Helper> for Condition {
                type Error = ();

                fn try_from(value: Helper) -> Result<Self, ()> {
                    match value {
                        Helper::Single(single) => Ok(Condition::KeyValue({
                            let key = PropKey::from_str(&single.0)?;
                            PropFilter::parse_with_key(key, &single.1).ok_or(())?
                        })),
                        Helper::ExplicitAnd { and } => Ok(Condition::And(
                            and.into_iter()
                                .map(TryInto::<Condition>::try_into)
                                .collect::<Result<Vec<_>, _>>()?,
                        )),
                        Helper::Or { or } => Ok(Condition::Or(
                            or.into_iter()
                                .map(TryInto::<Condition>::try_into)
                                .collect::<Result<Vec<_>, _>>()?,
                        )),
                        Helper::ImplicitAnd(and) => Ok(Condition::And(
                            and.into_iter()
                                .map(|(key, val)| {
                                    let key = PropKey::from_str(&key)?;
                                    PropFilter::parse_with_key(key, &val)
                                        .ok_or(())
                                        .map(Condition::KeyValue)
                                })
                                .collect::<Result<Vec<_>, _>>()?,
                        )),
                    }
                }
            }

            Helper::deserialize(deserializer)?
                .try_into()
                .map_err(|_| de::Error::custom("failed to parse fields"))
        }
    }

    impl<'de> Deserialize<'de> for VariantEntries {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            struct VariantsVisitor;

            impl<'de> Visitor<'de> for VariantsVisitor {
                type Value = VariantEntries;

                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("a map of blockstate variants")
                }

                fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
                where
                    M: MapAccess<'de>,
                {
                    let mut variants = Vec::with_capacity(access.size_hint().unwrap_or(0));

                    // Iterate over the map entries.
                    // serde_json preserves the order of keys as they appear in the file
                    // when iterating via MapAccess.
                    while let Some((key_str, definition)) =
                        access.next_entry::<String, VariantDefinition>()?
                    {
                        let filter = parse_variant_key(&key_str)
                            .ok_or_else(|| de::Error::custom("failed to parse fields"))?;
                        variants.push(VariantEntry { filter, definition });
                    }

                    Ok(VariantEntries(variants))
                }
            }

            deserializer.deserialize_map(VariantsVisitor)
        }
    }

    /// Helper to parse "face=ceiling,powered=true" into a `Vec<PropVal>`
    fn parse_variant_key(key: &str) -> Option<Vec<PropVal>> {
        let mut list = Vec::new();

        // Handle empty key or "normal" (legacy/default)
        if key.is_empty() || key == "normal" {
            return Some(list);
        }

        for part in key.split(',') {
            let val = PropVal::from_str(part).ok()?;
            list.push(val);
        }
        Some(list)
    }
}
