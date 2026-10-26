use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use mcje_downloader::{Library, RootManifest, sha1_hex, write_atomic};
use tokio::{fs, time};

#[tokio::main]
async fn main() {
    println!("cargo:rerun-if-changed=../../mc-version");

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_dir = PathBuf::from(manifest_dir);

    let target_mc_version = fs::read_to_string(manifest_dir.join("../../mc-version"))
        .await
        .unwrap()
        .trim()
        .to_string();

    let root_manifest = RootManifest::fetch().await.unwrap();
    let version_release = root_manifest
        .versions
        .into_iter()
        .find(|ver| ver.id == target_mc_version)
        .unwrap();

    let version_manifest = version_release.fetch_manifest().await.unwrap();

    let download_dir = manifest_dir
        .join("../../target/")
        .join("downloads")
        .join(&target_mc_version);
    let main_path = download_dir.join("mc.jar");

    if !download_dir.exists() {
        fs::create_dir_all(&download_dir).await.unwrap();
    }

    if !cache_matches(&main_path, &version_manifest.downloads.client.sha1).await {
        let client = version_manifest.downloads.client.download().await.unwrap();
        verify_sha1(&client, &version_manifest.downloads.client.sha1, &main_path);
        write_atomic(&main_path, &client).unwrap();
    }

    let libs_root = download_dir.join("libs");
    let natives_root = download_dir.join("natives");
    let assets_root = download_dir.join("assets");

    let mut classpath = main_path.to_str().unwrap().to_string();
    #[cfg(target_os = "windows")]
    let sep = ";";
    #[cfg(not(target_os = "windows"))]
    let sep = ":";

    let libraries = version_manifest.libraries;
    for lib in &libraries {
        if lib.rules.iter().all(|rule| rule.allow()) {
            let name = lib.name.replace([';', ':'], "-");
            let lib_path = libs_root.join(if lib.name.ends_with(".jar") {
                name
            } else {
                format!("{}.jar", name)
            });
            classpath += &format!("{}{}", sep, lib_path.to_str().unwrap());
            let artifact = lib.artifact();
            if !cache_matches(&lib_path, &artifact.sha1).await {
                time::sleep(Duration::from_millis(100)).await;
                let lib_source = artifact.download().await.unwrap();
                verify_sha1(&lib_source, &artifact.sha1, &lib_path);
                write_atomic(&lib_path, &lib_source).unwrap();
            }
        }
    }

    println!(
        "cargo:rustc-env=MCJE_VERSION={}",
        version_manifest.id.clone()
    );

    if !natives_root.exists() {
        fs::create_dir_all(&natives_root).await.unwrap();
    }
    println!(
        "cargo:rustc-env=MCJE_NATIVES_DIR={}",
        natives_root.to_str().unwrap()
    );

    // Native libraries ship as plain `:natives-<os>[-<arch>]` library entries
    // whose rules only filter by OS; pick the arch-matching variant per
    // artifact and extract the already-downloaded jar.
    let os = os_key();
    let arch = arch_key();
    let mut natives: BTreeMap<String, (bool, PathBuf, Vec<String>)> = BTreeMap::new();
    for lib in &libraries {
        if !lib.rules.iter().all(|rule| rule.allow()) {
            continue;
        }
        let Some(index) = lib.name.find(":natives-") else {
            continue;
        };
        let classifier = &lib.name[index + ":natives-".len()..];
        if !classifier_allowed(classifier, os, arch) {
            continue;
        }
        let coords = lib.name[..index].to_string();
        let entry = (
            classifier != os,
            libs_path(&libs_root, lib),
            lib.extract.exclude.clone(),
        );
        match natives.entry(coords) {
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                // Prefer the arch-specific jar over the generic one.
                if entry.0 && !slot.get().0 {
                    slot.insert(entry);
                }
            }
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(entry);
            }
        }
    }
    for (_coords, (_arch_specific, jar_path, exclude)) in natives {
        extract_natives(&jar_path, &natives_root, &exclude).await;
    }

    println!(
        "cargo:rustc-env=MCJE_ASSETS_DIR={}",
        assets_root.to_str().unwrap()
    );

    if let Some(asset_index) = version_manifest.asset_index {
        let index_root = assets_root.join("indexes");
        let index_path = index_root.join(format!("{}.json", asset_index.id));
        if !cache_matches(&index_path, &asset_index.sha1).await {
            time::sleep(Duration::from_millis(100)).await;
            let index_bytes = asset_index.download().await.unwrap();
            verify_sha1(&index_bytes, &asset_index.sha1, &index_path);
            write_atomic(&index_path, &index_bytes).unwrap();
        }
        println!("cargo:rustc-env=MCJE_ASSET_INDEX={}", asset_index.id);
    } else {
        println!("cargo:rustc-env=MCJE_ASSET_INDEX=");
    }

    // Compile the render-thread hook class against the client jar and add it
    // to the baked-in classpath so `mcre.ShotDriver` is always available.
    // Sources under java/ also include compile-time stubs for the jspecify
    // annotations referenced by the obfuscated client's class files.
    let java_root = manifest_dir.join("java");
    let mut java_sources = Vec::new();
    collect_java_sources(&java_root, &mut java_sources).await;
    java_sources.sort();
    for source in &java_sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }
    let hooks_out = manifest_dir.join("../../target/mcje-hooks");
    let hooks_out = hooks_out.canonicalize().unwrap_or(hooks_out);
    fs::create_dir_all(&hooks_out).await.unwrap();
    let javac = find_javac();
    let status = Command::new(&javac)
        .args([
            "--release",
            "21",
            "-cp",
            &classpath,
            "-d",
            hooks_out.to_str().unwrap(),
        ])
        .args(&java_sources)
        .status()
        .expect("failed to spawn javac; a JDK is required to build mcje (set JAVA_HOME)");
    assert!(status.success(), "javac failed to compile mcje hooks");
    classpath += &format!("{}{}", sep, hooks_out.to_str().unwrap());
    println!("cargo:rustc-env=MCJE_JVM_CLASSPATH={classpath}");
}

async fn collect_java_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries = match fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return,
    };
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(collect_java_sources(&path, out)).await;
        } else if path.extension().is_some_and(|ext| ext == "java") {
            out.push(path);
        }
    }
}

fn libs_path(libs_root: &Path, lib: &Library) -> PathBuf {
    let name = lib.name.replace([';', ':'], "-");
    libs_root.join(if lib.name.ends_with(".jar") {
        name
    } else {
        format!("{}.jar", name)
    })
}

fn os_key() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

fn arch_key() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    }
}

fn classifier_allowed(classifier: &str, os: &str, arch: &str) -> bool {
    classifier == os || classifier == format!("{os}-{arch}")
}

fn find_javac() -> PathBuf {
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let javac = PathBuf::from(java_home).join("bin").join("javac");
        if javac.exists() {
            return javac;
        }
    }
    PathBuf::from("javac")
}

async fn extract_natives(jar_path: &Path, natives_root: &Path, exclude: &[String]) {
    let bytes = fs::read(jar_path).await.unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("read natives jar");
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).expect("natives jar entry");
        let Some(name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let name_str = name.to_string_lossy().replace('\\', "/");
        if entry.is_dir()
            || name_str.starts_with("META-INF/")
            || exclude.iter().any(|prefix| name_str.starts_with(prefix))
        {
            continue;
        }
        let dest = natives_root.join(&name);
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).expect("read natives file");
        write_atomic(&dest, &bytes).expect("extract natives file");
    }
}

fn verify_sha1(bytes: &[u8], expected: &str, path: &Path) {
    if expected.is_empty() {
        return;
    }
    let actual = sha1_hex(bytes);
    assert_eq!(actual, expected, "SHA-1 mismatch for {}", path.display());
}

async fn cache_matches(path: &Path, expected: &str) -> bool {
    if expected.is_empty() {
        return path.exists();
    }
    match fs::read(path).await {
        Ok(bytes) => sha1_hex(&bytes) == expected,
        Err(_) => false,
    }
}
