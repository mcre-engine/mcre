use core::fmt;
use std::{collections::HashMap, slice};

use indexmap::IndexMap;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, Unexpected, Visitor},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum StateValue {
    Int(u8),
    Bool(bool),
    String(String),
}

/// Represents a single variant entry from the "variants" map.
#[derive(Debug, Clone)]
pub struct VariantEntry {
    /// Parsed from the JSON map key (e.g., "face=ceiling,facing=east")
    pub filter: HashMap<String, StateValue>,
    /// The value of the JSON map entry
    pub definition: VariantDefinition,
}

/// A wrapper struct to handle deserializing a JSON Map into a `Vec<VariantEntry>`
/// while preserving the order of entry appearance in the file.
#[derive(Debug, Clone)]
pub struct VariantEntries(pub Vec<VariantEntry>);

// Allow accessing the inner Vec easily
impl std::ops::Deref for VariantEntries {
    type Target = Vec<VariantEntry>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl IntoIterator for VariantEntries {
    type Item = VariantEntry;
    type IntoIter = std::vec::IntoIter<Self::Item>;

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
                    let filter = parse_variant_key(&key_str);
                    variants.push(VariantEntry { filter, definition });
                }

                Ok(VariantEntries(variants))
            }
        }

        deserializer.deserialize_map(VariantsVisitor)
    }
}

/// Helper to parse "face=ceiling,powered=true" into a HashMap
fn parse_variant_key(key: &str) -> HashMap<String, StateValue> {
    let mut map = HashMap::new();

    // Handle empty key or "normal" (legacy/default)
    if key.is_empty() || key == "normal" {
        return map;
    }

    for part in key.split(',') {
        // Split "key=value"
        if let Some((k, v)) = part.split_once('=') {
            map.insert(k.to_string(), parse_state_value(v));
        } else {
            // Fallback for malformed strings or single keys without values (rare in MC)
            // We treat the whole part as a key with an empty string value,
            // or you could ignore it.
            map.insert(part.to_string(), StateValue::String(String::new()));
        }
    }
    map
}

/// Helper to infer type (Int -> Bool -> String)
fn parse_state_value(v: &str) -> StateValue {
    if let Ok(b) = v.parse::<bool>() {
        StateValue::Bool(b)
    } else if let Ok(i) = v.parse::<u8>() {
        StateValue::Int(i)
    } else {
        StateValue::String(v.to_string())
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum VariantDefinition {
    Single(ModelVariant),
    Multiple(Vec<ModelVariant>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockModelId(pub String);

impl<'de> Deserialize<'de> for BlockModelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BlockPatternVisitor;

        impl<'de> Visitor<'de> for BlockPatternVisitor {
            type Value = BlockModelId;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string starting with \"minecraft:block/\"")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if let Some(id) = value.strip_prefix("minecraft:block/") {
                    Ok(BlockModelId(id.to_string()))
                } else if let Some(id) = value.strip_prefix("block/") {
                    Ok(BlockModelId(id.to_string()))
                } else {
                    Err(E::invalid_value(
                        de::Unexpected::Str(value),
                        &"string matching minecraft:block/*",
                    ))
                }
            }
        }

        deserializer.deserialize_str(BlockPatternVisitor)
    }
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

#[derive(Default, Debug, Clone)]
pub enum RotationDegrees {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

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

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MultipartRule {
    pub apply: VariantDefinition,
    #[serde(default)]
    pub when: Option<Condition>,
}

#[derive(Debug, Clone)]
pub enum Condition {
    KeyValue(String, StateValue),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

impl Condition {
    pub fn test(&self, state_values: &IndexMap<String, StateValue>) -> bool {
        match self {
            Condition::KeyValue(key, value) => {
                if let Some(state_value) = state_values.get(key) {
                    state_value == value
                } else {
                    false
                }
            }
            Condition::And(conditions) => conditions
                .iter()
                .all(|condition| condition.test(state_values)),
            Condition::Or(conditions) => conditions
                .iter()
                .any(|condition| condition.test(state_values)),
        }
    }
}

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Debug, Clone)]
        struct SingleCond(String, StateValue);

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
                        let (key, value): (String, StateValue) = map
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
            ImplicitAnd(HashMap<String, StateValue>),
        }

        impl From<Helper> for Condition {
            fn from(value: Helper) -> Self {
                match value {
                    Helper::Single(single) => Condition::KeyValue(single.0, single.1),
                    Helper::ExplicitAnd { and } => {
                        Condition::And(and.into_iter().map(Into::into).collect())
                    }
                    Helper::Or { or } => Condition::Or(or.into_iter().map(Into::into).collect()),
                    Helper::ImplicitAnd(and) => Condition::And(
                        and.into_iter()
                            .map(|(key, val)| Condition::KeyValue(key, val))
                            .collect(),
                    ),
                }
            }
        }

        Ok(Helper::deserialize(deserializer)?.into())
    }
}

pub enum BlockModelResolution<'a> {
    Unified(&'a [ModelVariant]),
    Multipart(Box<[ModelVariant]>),
}

impl BlockStateDefinition {
    pub fn resolve<'a>(
        &'a self,
        state_values: &IndexMap<String, StateValue>,
    ) -> Option<BlockModelResolution<'a>> {
        match self {
            Self::Variants(variants) => {
                'variant_loop: for variant in variants.iter() {
                    for (key, value) in &variant.filter {
                        let Some(state_value) = state_values.get(key) else {
                            continue 'variant_loop;
                        };

                        if state_value != value {
                            continue 'variant_loop;
                        };
                    }

                    let models = match &variant.definition {
                        VariantDefinition::Single(model) => slice::from_ref(model),
                        VariantDefinition::Multiple(models) => models.as_slice(),
                    };

                    return Some(BlockModelResolution::Unified(models));
                }

                None
            }
            Self::Multipart(rules) => {
                let mut resolved_models = Vec::new();

                for rule in rules {
                    let condition_met = if let Some(condition) = &rule.when {
                        condition.test(state_values)
                    } else {
                        true
                    };

                    if condition_met {
                        match rule.apply.clone() {
                            VariantDefinition::Single(model) => resolved_models.push(model),
                            VariantDefinition::Multiple(models) => resolved_models.extend(models),
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
    use crate::blockstates::BlockStateDefinition;
    use std::{
        fs::{self, File},
        path::PathBuf,
    };

    #[tokio::test]
    async fn test_parse_block_state_definition() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let manifest_dir = PathBuf::from(manifest_dir);
        let root_dir = manifest_dir.join("assets/minecraft/blockstates");

        let mut total = 0;
        let mut passed = 0;
        let mut failed = Vec::new();

        for entry in fs::read_dir(&root_dir).unwrap() {
            total += 1;
            let entry = entry.unwrap();
            let path = entry.path();
            let file = File::open(&path).unwrap();
            let result: Result<BlockStateDefinition, _> = serde_json::from_reader(file);
            match result {
                Ok(_) => passed += 1,
                Err(err) => {
                    let file_name = path.file_name().unwrap().to_str().unwrap();
                    let name = file_name.strip_suffix(".json").unwrap().to_string();
                    failed.push((name, err));
                }
            }
        }

        if !failed.is_empty() {
            eprintln!("Failed tests:");
            for (name, err) in failed {
                eprintln!("- {}: {}", name, err);
            }
        }

        assert_eq!(passed, total);
    }
}
