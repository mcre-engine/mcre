use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use url::Url;

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const ASSET_OBJECTS_BASE: &str = "https://resources.download.minecraft.net";

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
    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndex>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AssetIndex {
    pub id: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    #[serde(rename = "totalSize", default)]
    pub total_size: u64,
    pub url: Url,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct VersionDownloads {
    pub client: DownloadInfo,
    pub server: DownloadInfo,
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

impl AssetIndex {
    pub async fn download(&self) -> reqwest::Result<Bytes> {
        reqwest::get(self.url.clone())
            .await?
            .error_for_status()?
            .bytes()
            .await
    }
}

/// Parsed contents of an asset index (`assets/indexes/<id>.json`): virtual
/// asset paths mapped to their content-addressed object files.
#[derive(Debug, Deserialize)]
pub struct AssetIndexFile {
    pub objects: HashMap<String, AssetObject>,
}

/// One object of an asset index: the SHA-1 hash naming its file under
/// `assets/objects/` and its byte size.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    #[serde(default)]
    pub size: u64,
}

impl AssetIndexFile {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Objects not stored under `objects_root` or stored with the wrong
    /// size or content, deduplicated by hash (several logical paths share
    /// content).
    pub fn missing(&self, objects_root: &Path) -> Vec<(PathBuf, AssetObject)> {
        let mut missing: HashMap<&str, (PathBuf, AssetObject)> = HashMap::new();
        for object in self.objects.values() {
            let dest = object.path_in(objects_root);
            let stored = std::fs::metadata(&dest).is_ok_and(|meta| meta.len() == object.size)
                && std::fs::read(&dest).is_ok_and(|bytes| sha1_hex(&bytes) == object.hash);
            if !stored {
                missing
                    .entry(&object.hash)
                    .or_insert_with(|| (dest, object.clone()));
            }
        }
        missing.into_values().collect()
    }
}

impl AssetObject {
    /// Content-addressed location of this object under an `objects` root.
    pub fn path_in(&self, objects_root: &Path) -> PathBuf {
        let prefix = self.hash.get(..2).unwrap_or(&self.hash);
        objects_root.join(prefix).join(&self.hash)
    }

    /// Download this object to `dest`, retrying transient failures and
    /// verifying the transferred size against the index.
    pub async fn download_to(&self, client: &reqwest::Client, dest: &Path) -> Result<(), String> {
        let prefix = self.hash.get(..2).unwrap_or(&self.hash);
        let url = format!("{ASSET_OBJECTS_BASE}/{prefix}/{}", self.hash);
        let mut last = String::new();
        for attempt in 1..=3u64 {
            match fetch(client, &url).await {
                Ok(bytes) if bytes.len() as u64 == self.size => {
                    let actual = sha1_hex(&bytes);
                    if actual != self.hash {
                        last = format!("{}: SHA-1 mismatch (got {actual})", self.hash);
                    } else {
                        return write_atomic(dest, &bytes)
                            .map_err(|err| format!("{}: {err}", dest.display()));
                    }
                }
                Ok(bytes) => {
                    last = format!(
                        "{}: got {} bytes, want {}",
                        self.hash,
                        bytes.len(),
                        self.size
                    );
                }
                Err(err) => last = err,
            }
            eprintln!("asset {} attempt {attempt}/3 failed: {last}", self.hash);
            if attempt < 3 {
                std::thread::sleep(std::time::Duration::from_millis(250 * attempt));
            }
        }
        Err(last)
    }
}

/// Shared HTTP client for bulk asset downloads (one connection pool).
pub fn asset_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder().build()
}

/// Write `bytes` to `path` through a temporary file in the same directory.
///
/// The rename replaces the target atomically, so an interrupted write leaves
/// the previous file intact instead of a truncated one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    std::fs::create_dir_all(parent)?;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "download".to_string());
    let temp = parent.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));

    let result = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        rename_replace(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(not(windows))]
fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

#[cfg(windows)]
fn rename_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        #[link_name = "MoveFileExW"]
        fn move_file_ex_w(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    let from: Vec<u16> = from.as_os_str().encode_wide().chain([0]).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain([0]).collect();
    let replaced = unsafe { move_file_ex_w(from.as_ptr(), to.as_ptr(), 0x1 | 0x8) };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Lowercase hex SHA-1 digest of `bytes`.
pub fn sha1_hex(bytes: &[u8]) -> String {
    let mut state: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let bit_len = (bytes.len() as u64).wrapping_mul(8);

    let mut padded = Vec::with_capacity((bytes.len() + 72) & !63);
    padded.extend_from_slice(bytes);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.as_chunks::<64>().0 {
        let mut words = [0u32; 80];
        for (index, chunk) in block.as_chunks::<4>().0.iter().enumerate() {
            words[index] = u32::from_be_bytes(*chunk);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (f, k) = match index {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
    }

    state.iter().map(|word| format!("{word:08x}")).collect()
}

async fn fetch(client: &reqwest::Client, url: &str) -> Result<Bytes, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?;
    response.bytes().await.map_err(|err| err.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_known_vectors() {
        assert_eq!(sha1_hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            sha1_hex(b"The quick brown fox jumps over the lazy dog"),
            "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12"
        );
    }

    #[test]
    fn asset_cache_rejects_same_size_wrong_content() {
        let dir = temp_dir("asset-hash");
        let bytes = b"good";
        let hash = sha1_hex(bytes);
        let object = AssetObject {
            hash: hash.clone(),
            size: bytes.len() as u64,
        };
        let index = AssetIndexFile {
            objects: HashMap::from([("asset".to_string(), object.clone())]),
        };
        let path = object.path_in(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"evil").unwrap();
        assert_eq!(index.missing(&dir).len(), 1);

        std::fs::write(&path, bytes).unwrap();
        assert!(index.missing(&dir).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_replaces_existing_file() {
        let dir = temp_dir("replace");
        let path = dir.join("file.bin");
        std::fs::write(&path, b"old").unwrap();

        write_atomic(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_no_temp_files(&dir);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_atomic_leaves_no_temp_file() {
        let dir = temp_dir("clean");
        let path = dir.join("nested").join("file.bin");

        write_atomic(&path, b"data").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"data");
        assert_no_temp_files(path.parent().unwrap());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn temp_dir(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mcre_downloader_{name}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn assert_no_temp_files(dir: &Path) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let name = entry.file_name();
            assert!(
                !name.to_string_lossy().ends_with(".tmp"),
                "{name:?} left behind"
            );
        }
    }
}
