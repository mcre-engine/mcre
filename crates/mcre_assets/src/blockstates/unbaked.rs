use core::{error, fmt, ops::Deref};

use alloc::{
    boxed::Box,
    vec::{self, Vec},
};
use mcre_core::{BlockState, PropFilter, PropVal, Vec4f, Weighted};
use serde::{Deserialize, Deserializer};

use crate::{
    BakedBlockStateDefinition, BakedModelVariant, BakedMultipartEntry, BakedVariantDefinition,
    BakedVariantEntry, BlockModelId, BlockModelRegistry,
};

/// Represents a single variant entry from the "variants" map.
#[derive(Debug, Clone)]
pub struct UnbakedVariantEntry {
    /// Parsed from the JSON map key (e.g., "face=ceiling,facing=east")
    pub filter: Vec<PropVal>,
    /// The value of the JSON map entry
    pub definition: UnbakedVariantDefinition,
}

/// A wrapper struct to handle deserializing a JSON Map into a `Vec<VariantEntry>`
/// while preserving the order of entry appearance in the file.
#[derive(Debug, Clone)]
pub struct UnbakedVariantEntries(pub Vec<UnbakedVariantEntry>);

// Allow accessing the inner Vec easily
impl Deref for UnbakedVariantEntries {
    type Target = Vec<UnbakedVariantEntry>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntoIterator for UnbakedVariantEntries {
    type Item = UnbakedVariantEntry;
    type IntoIter = vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum UnbakedBlockStateDefinition {
    /// Deserializes from a JSON Map, but stores as a `Vec<VariantEntry>`.
    Variants(UnbakedVariantEntries),
    Multipart(Vec<UnbakedMultipartEntry>),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum UnbakedVariantDefinition {
    Single(UnbakedModelVariant),
    Multiple(Vec<UnbakedModelVariant>),
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UnbakedModelVariant {
    pub model: BlockModelId,
    #[serde(default)]
    pub uvlock: bool,
    #[serde(default = "default_weight")]
    pub weight: u8,
    #[serde(default)]
    pub x: Quadrant,
    #[serde(default)]
    pub y: Quadrant,
    #[serde(default)]
    pub z: Quadrant,
}

fn default_weight() -> u8 {
    1
}

#[derive(Default, Debug, Copy, Clone)]
pub enum Quadrant {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct UnbakedMultipartEntry {
    pub apply: UnbakedVariantDefinition,
    #[serde(default)]
    pub when: Option<Condition>,
}

#[derive(Debug, Clone)]
pub enum Condition {
    Simple(PropFilter),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

impl Quadrant {
    pub fn rotate_uv(self, uv: Vec4f) -> Vec4f {
        let [u1, v1, u2, v2] = *uv;

        // UV corners in Minecraft order:
        // 0 = top-left, 1 = top-right, 2 = bottom-right, 3 = bottom-left
        // but you can treat them as a loop
        let mut pts = [(u1, v1), (u2, v1), (u2, v2), (u1, v2)];

        match self {
            Self::R0 => {}
            Self::R90 => pts.rotate_right(1), // 1-step rotate
            Self::R180 => pts.rotate_right(2),
            Self::R270 => pts.rotate_right(3),
        }

        // After rotation, recompute bounding box
        let min_u = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let max_u = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let min_v = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_v = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);

        Vec4f::new(min_u, min_v, max_u, max_v)
    }
}

impl Condition {
    pub fn test(&self, state: BlockState) -> bool {
        match self {
            Condition::Simple(filter) => {
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

#[derive(Debug, Clone)]
pub struct BlockStateBakeError {
    missing_model: BlockModelId,
}

impl fmt::Display for BlockStateBakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Missing model: {}", self.missing_model)
    }
}

impl error::Error for BlockStateBakeError {}

impl UnbakedMultipartEntry {
    pub fn bake(
        self,
        registry: &BlockModelRegistry,
    ) -> Result<BakedMultipartEntry, BlockStateBakeError> {
        Ok(BakedMultipartEntry {
            apply: self.apply.bake(registry)?,
            when: self.when,
        })
    }
}

impl UnbakedVariantDefinition {
    pub fn bake(
        self,
        registry: &BlockModelRegistry,
    ) -> Result<BakedVariantDefinition, BlockStateBakeError> {
        match self {
            Self::Single(variant) => Ok(BakedVariantDefinition::Single(BakedModelVariant {
                model: registry.get_key_by_id(&variant.model).ok_or_else(|| {
                    BlockStateBakeError {
                        missing_model: variant.model.clone(),
                    }
                })?,
                uvlock: variant.uvlock,
                x: variant.x,
                y: variant.y,
            })),
            Self::Multiple(variants) => Ok(BakedVariantDefinition::Multiple(
                variants
                    .into_iter()
                    .map(|variant| {
                        Ok(Weighted {
                            value: BakedModelVariant {
                                model: registry.get_key_by_id(&variant.model).ok_or_else(|| {
                                    BlockStateBakeError {
                                        missing_model: variant.model.clone(),
                                    }
                                })?,
                                uvlock: variant.uvlock,
                                x: variant.x,
                                y: variant.y,
                            },
                            weight: variant.weight,
                        })
                    })
                    .collect::<Result<Box<[_]>, BlockStateBakeError>>()?
                    .into(),
            )),
        }
    }
}

impl UnbakedVariantEntry {
    pub fn bake(
        self,
        registry: &BlockModelRegistry,
    ) -> Result<BakedVariantEntry, BlockStateBakeError> {
        Ok(BakedVariantEntry {
            filter: self.filter.into_boxed_slice(),
            definition: self.definition.bake(registry)?,
        })
    }
}

impl UnbakedBlockStateDefinition {
    pub fn bake(
        self,
        registry: &BlockModelRegistry,
    ) -> Result<BakedBlockStateDefinition, BlockStateBakeError> {
        match self {
            Self::Variants(entries) => {
                let entries = entries
                    .into_iter()
                    .map(|entry| entry.bake(registry))
                    .collect::<Result<Box<[_]>, _>>()?;
                Ok(BakedBlockStateDefinition::Variants(entries))
            }
            Self::Multipart(entries) => {
                let entries = entries
                    .into_iter()
                    .map(|entry| entry.bake(registry))
                    .collect::<Result<Box<[_]>, _>>()?;
                Ok(BakedBlockStateDefinition::Multipart(entries))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        fs::{self, File},
        path::PathBuf,
    };

    #[tokio::test]
    async fn test_parse_and_resolve_block_state_definition() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let manifest_dir = PathBuf::from(manifest_dir);
        let root_dir = manifest_dir.join("assets/minecraft/blockstates");

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

            let result: Result<UnbakedBlockStateDefinition, _> = serde_json::from_reader(file);

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
    use core::str::FromStr;

    use crate::FxHashMap;

    use super::*;
    use alloc::string::String;
    use mcre_core::PropKey;
    use serde::{
        Deserialize,
        de::{self, MapAccess, Unexpected, Visitor},
    };

    impl<'de> Deserialize<'de> for Quadrant {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            struct RotationVisitor;

            impl<'de> Visitor<'de> for RotationVisitor {
                type Value = Quadrant;

                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("an integer rotation: 0, 90, 180, or 270")
                }

                fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    match value {
                        0 => Ok(Quadrant::R0),
                        90 => Ok(Quadrant::R90),
                        180 => Ok(Quadrant::R180),
                        270 => Ok(Quadrant::R270),
                        _ => Err(E::invalid_value(Unexpected::Signed(value), &self)),
                    }
                }

                fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
                where
                    E: de::Error,
                {
                    match value {
                        0 => Ok(Quadrant::R0),
                        90 => Ok(Quadrant::R90),
                        180 => Ok(Quadrant::R180),
                        270 => Ok(Quadrant::R270),
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
                        Helper::Single(single) => Ok(Condition::Simple({
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
                                        .map(Condition::Simple)
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

    impl<'de> Deserialize<'de> for UnbakedVariantEntries {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            struct VariantsVisitor;

            impl<'de> Visitor<'de> for VariantsVisitor {
                type Value = UnbakedVariantEntries;

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
                        access.next_entry::<String, UnbakedVariantDefinition>()?
                    {
                        let filter = parse_variant_key(&key_str)
                            .ok_or_else(|| de::Error::custom("failed to parse fields"))?;
                        variants.push(UnbakedVariantEntry { filter, definition });
                    }

                    Ok(UnbakedVariantEntries(variants))
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
