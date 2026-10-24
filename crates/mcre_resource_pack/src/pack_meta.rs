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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

    pub const CURRENT: Self = Self {
        major: 100,
        minor: 2,
    };
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
pub struct OverlayEntry {
    pub directory: String,
    #[serde(default)]
    pub min_format: Option<PackVersion>,
    #[serde(default)]
    pub max_format: Option<PackVersion>,
    #[serde(default)]
    pub formats: Option<(u32, u32)>,
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

    /// Overlay directories applicable to the current game version, in listed
    /// order (later entries win, mirroring vanilla's reversed overlay stack).
    /// Missing bounds are unbounded on that side.
    pub fn applicable_overlays(&self) -> Vec<String> {
        let Some(overlays) = &self.overlays else {
            return Vec::new();
        };
        overlays
            .entries
            .iter()
            .filter(|entry| {
                entry
                    .min_format
                    .is_none_or(|min| min <= PackVersion::CURRENT)
                    && entry
                        .max_format
                        .is_none_or(|max| PackVersion::CURRENT <= max)
                    && entry.formats.is_none_or(|(lo, hi)| {
                        lo <= PackVersion::CURRENT.major && PackVersion::CURRENT.major <= hi
                    })
            })
            .map(|entry| entry.directory.clone())
            .collect()
    }

    /// Whether this pack declares support for the current game version.
    /// Packs with no format declaration at all are treated as compatible.
    pub fn supports_current_version(&self) -> bool {
        if self.pack.pack_format.is_none()
            && self.pack.min_format.is_none()
            && self.pack.max_format.is_none()
        {
            return true;
        }
        let min = self
            .pack
            .min_format
            .or(self.pack.pack_format.map(PackVersion::new))
            .unwrap_or(PackVersion::new(0));
        let max = self
            .pack
            .max_format
            .or(self.pack.pack_format.map(PackVersion::new))
            .unwrap_or(PackVersion::new(u32::MAX));
        min <= PackVersion::CURRENT && PackVersion::CURRENT <= max
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

    #[test]
    fn test_applicable_overlays() {
        let json = r#"{
            "pack": {"description": "Test"},
            "overlays": {
                "entries": [
                    {"directory": "old", "max_format": 50},
                    {"directory": "current", "min_format": 90},
                    {"directory": "exact", "min_format": [100, 2], "max_format": [100, 2]},
                    {"directory": "legacy", "formats": [90, 200]},
                    {"directory": "legacy_old", "formats": [10, 50]},
                    {"directory": "future", "min_format": 200},
                    {"directory": "unbounded"}
                ]
            }
        }"#;

        let meta: PackMeta = serde_json::from_str(json).unwrap();
        assert_eq!(
            meta.applicable_overlays(),
            vec![
                "current".to_string(),
                "exact".to_string(),
                "legacy".to_string(),
                "unbounded".to_string()
            ]
        );
    }

    #[test]
    fn test_supports_current_version() {
        let bare: PackMeta =
            serde_json::from_str(r#"{"pack": {"description": "No format declared"}}"#).unwrap();
        assert!(bare.supports_current_version());

        let matching: PackMeta = serde_json::from_str(
            r#"{"pack": {"description": "Test", "min_format": 90, "max_format": 200}}"#,
        )
        .unwrap();
        assert!(matching.supports_current_version());

        let ancient: PackMeta =
            serde_json::from_str(r#"{"pack": {"description": "Test", "pack_format": 18}}"#)
                .unwrap();
        assert!(!ancient.supports_current_version());

        let future: PackMeta =
            serde_json::from_str(r#"{"pack": {"description": "Test", "min_format": 1000}}"#)
                .unwrap();
        assert!(!future.supports_current_version());
    }

    #[test]
    #[cfg(feature = "download")]
    fn test_current_format_matches_game_jar() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let version = std::fs::read_to_string(manifest_dir.join("../../mc-version"))
            .unwrap()
            .trim()
            .to_string();
        let jar_path = manifest_dir
            .join("../../target/downloads")
            .join(&version)
            .join("mc.jar");
        let file = std::fs::File::open(jar_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut entry = archive.by_name("version.json").unwrap();
        let mut text = String::new();
        std::io::Read::read_to_string(&mut entry, &mut text).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let pack_version = &json["pack_version"];
        assert_eq!(
            pack_version["resource_major"].as_u64().unwrap(),
            u64::from(PackVersion::CURRENT.major)
        );
        assert_eq!(
            pack_version["resource_minor"].as_u64().unwrap(),
            u64::from(PackVersion::CURRENT.minor)
        );
    }
}
