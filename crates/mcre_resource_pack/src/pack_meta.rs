use serde::Deserialize;
use std::collections::HashMap;

/// Root struct for `pack.mcmeta`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMeta {
    pub pack: PackSection,
    #[serde(default)]
    pub filter: Option<FilterSection>,
    #[serde(default)]
    pub language: Option<HashMap<String, LanguageEntry>>,
    #[serde(default)]
    pub overlays: Option<OverlaysSection>,
}

/// The `pack` section of `pack.mcmeta`.
#[derive(Debug, Clone, Deserialize)]
pub struct PackSection {
    pub description: serde_json::Value,

    /// Legacy single pack_format value (old format).
    #[serde(default)]
    pub pack_format: Option<u32>,

    /// New min_format (major or major.minor).
    #[serde(default)]
    pub min_format: Option<PackVersion>,

    /// New max_format (major or major.minor).
    #[serde(default)]
    pub max_format: Option<PackVersion>,
}

/// A pack version — either a single integer `42` or a pair `[42, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackVersion {
    pub major: u32,
    pub minor: u32,
}

impl PackVersion {
    pub const fn new(major: u32) -> Self {
        Self { major, minor: 0 }
    }

    pub const fn with_minor(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl<'de> Deserialize<'de> for PackVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Unexpected, Visitor};

        struct PackVersionVisitor;

        impl<'de> Visitor<'de> for PackVersionVisitor {
            type Value = PackVersion;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an integer or a two-element array of integers")
            }

            fn visit_u32<E: de::Error>(self, value: u32) -> Result<PackVersion, E> {
                Ok(PackVersion::new(value))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<PackVersion, E> {
                let v = u32::try_from(value).map_err(|_| {
                    E::invalid_value(Unexpected::Unsigned(value), &"value fits in u32")
                })?;
                Ok(PackVersion::new(v))
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<PackVersion, A::Error> {
                let major = seq
                    .next_element::<u32>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"at least one element"))?;
                let minor = seq.next_element::<u32>()?.unwrap_or(0);
                Ok(PackVersion::with_minor(major, minor))
            }
        }

        deserializer.deserialize_any(PackVersionVisitor)
    }
}

/// Filter section for removing entries from lower-priority packs.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterSection {
    pub block: Vec<FilterPattern>,
}

/// A single filter pattern.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterPattern {
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

/// Section for specifying additional languages.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageEntry {
    pub name: String,
    pub region: String,
    #[serde(default)]
    pub bidirectional: bool,
}

/// Section for pack overlays (sub-packs for different versions).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlaysSection {
    pub entries: Vec<OverlayEntry>,
}

/// A single overlay entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayEntry {
    pub directory: String,
    #[serde(default)]
    pub min_format: Option<PackVersion>,
    #[serde(default)]
    pub max_format: Option<PackVersion>,
}

impl PackMeta {
    /// Returns the effective min format version.
    /// Prefers `min_format`, falls back to `pack_format`, then defaults to 0.
    pub fn effective_min_format(&self) -> u32 {
        self.pack
            .min_format
            .map_or_else(|| self.pack.pack_format.unwrap_or(0), |v| v.major)
    }

    /// Returns the effective max format version.
    /// Prefers `max_format`, falls back to `pack_format`, then defaults to 0.
    pub fn effective_max_format(&self) -> u32 {
        self.pack
            .max_format
            .map_or_else(|| self.pack.pack_format.unwrap_or(0), |v| v.major)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_legacy_format() {
        let json = r#"{
            "pack": {
                "pack_format": 18,
                "description": "Test pack"
            }
        }"#;

        let meta: PackMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.effective_min_format(), 18);
        assert_eq!(meta.effective_max_format(), 18);
    }

    #[test]
    fn test_parse_new_format() {
        let json = r#"{
            "pack": {
                "description": {"text": "Test pack"},
                "min_format": 88,
                "max_format": 88
            }
        }"#;

        let meta: PackMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.effective_min_format(), 88);
        assert_eq!(meta.effective_max_format(), 88);
    }

    #[test]
    fn test_parse_version_pair() {
        let json = r#"{
            "pack": {
                "description": "Test",
                "min_format": [88, 1],
                "max_format": [88, 5]
            }
        }"#;

        let meta: PackMeta = serde_json::from_str(json).unwrap();
        let min = meta.pack.min_format.unwrap();
        let max = meta.pack.max_format.unwrap();
        assert_eq!(min.major, 88);
        assert_eq!(min.minor, 1);
        assert_eq!(max.major, 88);
        assert_eq!(max.minor, 5);
    }

    #[test]
    fn test_parse_with_filter() {
        let json = r#"{
            "pack": {
                "pack_format": 18,
                "description": "Filtered pack"
            },
            "filter": {
                "block": [
                    {"namespace": "minecraft", "path": ".*_wall\\.json"}
                ]
            }
        }"#;

        let meta: PackMeta = serde_json::from_str(json).unwrap();
        assert!(meta.filter.is_some());
        let patterns = &meta.filter.as_ref().unwrap().block;
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].namespace.as_deref(), Some("minecraft"));
    }
}
