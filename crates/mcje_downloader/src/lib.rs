use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LatestReleases {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseKind {
    Snapshot,
    Release,
    OldBeta,
    OldAlpha,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VersionRelease {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: ReleaseKind,
    pub url: Url,
    pub time: DateTime<Utc>,
    pub release_time: DateTime<Utc>,
    pub sha1: String,
    pub compliance_level: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct RootManifest {
    pub latest: LatestReleases,
    pub versions: Vec<VersionRelease>,
}

// many fields omitted
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct VersionManifest {
    pub downloads: VersionDownloads,
    pub id: String,
    pub libraries: Vec<Library>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct VersionDownloads {
    pub client: DownloadInfo,
    pub client_mappings: DownloadInfo,
    pub server: DownloadInfo,
    pub server_mappings: DownloadInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct DownloadInfo {
    pub sha1: String,
    pub size: u64,
    pub url: Url,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Library {
    pub downloads: LibraryDownloads,
    #[serde(skip_serializing_if = "LibraryExtractInstructions::is_empty", default)]
    pub extract: LibraryExtractInstructions,
    pub name: String,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub natives: HashMap<OsName, String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LibraryExtractInstructions {
    pub exclude: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LibraryDownloads {
    pub artifact: LibraryDownload,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub classifiers: HashMap<String, LibraryDownload>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LibraryDownload {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: Url,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OsName {
    Linux,
    Windows,
    Osx,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct OsRule {
    pub name: OsName,
}

impl RootManifest {
    pub async fn fetch() -> reqwest::Result<Self> {
        Self::fetch_from_url(MANIFEST_URL).await
    }

    pub async fn fetch_from_url(url: &str) -> reqwest::Result<Self> {
        reqwest::get(url).await?.error_for_status()?.json().await
    }
}

impl VersionRelease {
    pub async fn fetch_manifest(&self) -> reqwest::Result<VersionManifest> {
        reqwest::get(self.url.clone())
            .await?
            .error_for_status()?
            .json()
            .await
    }
}

impl DownloadInfo {
    pub async fn download(&self) -> reqwest::Result<Bytes> {
        reqwest::get(self.url.clone())
            .await?
            .error_for_status()?
            .bytes()
            .await
    }

    pub async fn download_as_stream(
        &self,
    ) -> reqwest::Result<impl Stream<Item = reqwest::Result<Bytes>>> {
        Ok(reqwest::get(self.url.clone())
            .await?
            .error_for_status()?
            .bytes_stream())
    }

    pub async fn download_as_string(&self) -> reqwest::Result<String> {
        reqwest::get(self.url.clone())
            .await?
            .error_for_status()?
            .text()
            .await
    }
}

impl OsName {
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        let current = Self::Windows;
        #[cfg(target_os = "linux")]
        let current = Self::Linux;
        #[cfg(target_os = "macos")]
        let current = Self::Osx;

        current
    }

    pub fn is_current(&self) -> bool {
        self == &Self::current()
    }
}

impl OsRule {
    pub fn allow(&self) -> bool {
        self.name.is_current()
    }
}

impl Rule {
    pub fn allow(&self) -> bool {
        match self.action {
            RuleAction::Allow => self.os.as_ref().is_none_or(|os| os.allow()),
            RuleAction::Disallow => self.os.as_ref().is_some_and(|os| !os.allow()),
        }
    }
}

impl Library {
    pub async fn download(
        &self,
    ) -> reqwest::Result<Option<((String, Bytes), Option<(String, Bytes)>)>> {
        if !self.rules.iter().all(Rule::allow) {
            return Ok(None);
        }
        let artifact = (
            self.downloads.artifact.path.clone(),
            self.downloads.artifact.download().await?,
        );
        let natives = if let Some(natives_key) = self.natives.get(&OsName::current()) {
            if let Some(natives_artifact) = self.downloads.classifiers.get(natives_key) {
                Some((
                    natives_artifact.path.clone(),
                    natives_artifact.download().await?,
                ))
            } else {
                None
            }
        } else {
            None
        };
        Ok(Some((artifact, natives)))
    }

    pub async fn download_as_stream(
        &self,
    ) -> reqwest::Result<
        Option<(
            (String, impl Stream<Item = reqwest::Result<Bytes>>),
            Option<(String, impl Stream<Item = reqwest::Result<Bytes>>)>,
        )>,
    > {
        if !self.rules.iter().all(Rule::allow) {
            return Ok(None);
        }
        let artifact = (
            self.downloads.artifact.path.clone(),
            self.downloads.artifact.download_as_stream().await?,
        );
        let native = if let Some(native_artifact) = self.native() {
            Some((
                native_artifact.path.clone(),
                native_artifact.download_as_stream().await?,
            ))
        } else {
            None
        };
        Ok(Some((artifact, native)))
    }

    pub fn artifact(&self) -> &LibraryDownload {
        &self.downloads.artifact
    }

    pub fn native(&self) -> Option<&LibraryDownload> {
        self.natives
            .get(&OsName::current())
            .and_then(|natives_key| self.downloads.classifiers.get(natives_key))
    }
}

impl LibraryDownload {
    pub async fn download(&self) -> reqwest::Result<Bytes> {
        reqwest::get(self.url.clone())
            .await?
            .error_for_status()?
            .bytes()
            .await
    }

    pub async fn download_as_stream(
        &self,
    ) -> reqwest::Result<impl Stream<Item = reqwest::Result<Bytes>>> {
        Ok(reqwest::get(self.url.clone())
            .await?
            .error_for_status()?
            .bytes_stream())
    }
}

impl LibraryExtractInstructions {
    pub fn is_empty(&self) -> bool {
        self.exclude.is_empty()
    }
}

/// Remove array brackets and count them
fn remove_brackets(line: &str) -> (&str, usize) {
    let mut result = line;
    let mut count = 0;

    while result.ends_with("[]") {
        count += 1;
        result = &result[..result.len() - 2];
    }

    (result, count)
}

/// Remap file path according to Java class signature conventions
fn remap_file_path(path: &str) -> String {
    let remap_primitives: HashMap<&str, &str> = [
        ("int", "I"),
        ("double", "D"),
        ("boolean", "Z"),
        ("float", "F"),
        ("long", "J"),
        ("byte", "B"),
        ("short", "S"),
        ("char", "C"),
        ("void", "V"),
    ]
    .iter()
    .cloned()
    .collect();

    if let Some(&primitive) = remap_primitives.get(path) {
        primitive.to_string()
    } else {
        format!("L{};", path.replace(".", "/"))
    }
}

/// Convert mappings from one format to another
pub fn convert_mappings(mappings: &str) -> String {
    // First pass: collect file names
    let mut file_name: HashMap<String, String> = HashMap::new();

    for line in mappings.lines() {
        if line.starts_with('#') || line.starts_with("    ") {
            continue;
        }

        let parts: Vec<&str> = line.split(" -> ").collect();
        if parts.len() < 2 {
            continue;
        }

        let deobf_name = parts[0];
        let obf_name = parts[1].split(':').next().unwrap_or("");

        file_name.insert(remap_file_path(deobf_name), obf_name.to_string());
    }

    // Second pass: process and store output
    let mut output = String::new();

    for line in mappings.lines() {
        if line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split(" -> ").collect();
        if parts.len() < 2 {
            continue;
        }

        let deobf_name = parts[0];
        let obf_name = parts[1].trim();

        if line.starts_with("    ") {
            let deobf_name = deobf_name.trim_start();
            let method_parts: Vec<&str> = deobf_name.split_whitespace().collect();
            if method_parts.len() < 2 {
                continue;
            }

            let mut method_type = method_parts[0]
                .split(':')
                .next_back()
                .unwrap_or("")
                .to_string();
            let method_name = method_parts[1];

            if method_name.contains('(') && method_name.contains(')') {
                // Process function
                let function_name = method_name.split('(').next().unwrap_or("");
                let variables_str = if let Some(v) = method_name.split('(').next_back() {
                    v.split(')').next().unwrap_or("")
                } else {
                    ""
                };

                let (method_type_clean, array_length_type) = remove_brackets(&method_type);
                method_type = remap_file_path(method_type_clean);

                // Get obfuscated class name if available
                if let Some(obf_class) = file_name.get(&method_type) {
                    method_type = format!("L{};", obf_class);
                }

                // Handle dots in class names
                if method_type.contains('.') {
                    method_type = method_type.replace(".", "/");
                }

                // Restore array brackets
                for _ in 0..array_length_type {
                    if method_type.ends_with(';') {
                        method_type = format!("[{}", &method_type[..method_type.len() - 1]) + ";";
                    } else {
                        method_type = format!("[{}", method_type);
                    }
                }

                let mut processed_variables = String::new();

                if !variables_str.is_empty() {
                    let variables: Vec<&str> = variables_str.split(',').collect();
                    let mut processed_vars = Vec::new();

                    for variable in variables {
                        let (var_clean, array_count) = remove_brackets(variable);
                        let mut remapped = remap_file_path(var_clean);

                        // Get obfuscated class name if available
                        if file_name.contains_key(&remapped) {
                            let obf_class = file_name.get(&remapped).unwrap();
                            remapped = format!("L{};", obf_class);
                        }

                        // Handle dots in class names
                        if remapped.contains('.') {
                            remapped = remapped.replace(".", "/");
                        }

                        // Restore array brackets
                        for _ in 0..array_count {
                            if remapped.ends_with(';') {
                                remapped = format!("[{}", &remapped[..remapped.len() - 1]) + ";";
                            } else {
                                remapped = format!("[{}", remapped);
                            }
                        }

                        processed_vars.push(remapped);
                    }

                    processed_variables = processed_vars.join("");
                }

                output.push_str(&format!(
                    "\t{} ({}){} {}\n",
                    obf_name, processed_variables, method_type, function_name
                ));
            } else {
                // Process field
                output.push_str(&format!("\t{} {}\n", obf_name, method_name));
            }
        } else {
            // Process class
            let obf_name = obf_name.split(':').next().unwrap_or("");
            let deobf_remapped = remap_file_path(deobf_name);
            let obf_remapped = remap_file_path(obf_name);

            // Remove the L and ; from class names
            let deobf_clean = &deobf_remapped[1..deobf_remapped.len() - 1];
            let obf_clean = &obf_remapped[1..obf_remapped.len() - 1];

            output.push_str(&format!("{} {}\n", obf_clean, deobf_clean));
        }
    }

    output
}
