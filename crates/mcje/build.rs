use mcje_downloader::RootManifest;
use std::{path::PathBuf, time::Duration};
use tokio::{fs, time};

#[tokio::main]
async fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_dir = PathBuf::from(manifest_dir);
    let root_manifest = RootManifest::fetch().await.unwrap();
    let version_release = root_manifest
        .versions
        .into_iter()
        .find(|ver| ver.id == "26.1-snapshot-4")
        .unwrap();

    let version_manifest = version_release.fetch_manifest().await.unwrap();

    let root_path = manifest_dir.join("../../target/").join("downloads");
    let main_path = root_path.join("mc.jar");

    if !root_path.exists() {
        fs::create_dir_all(&root_path).await.unwrap();
    }

    if !main_path.exists() {
        let client = version_manifest.downloads.client.download().await.unwrap();
        fs::write(&main_path, client).await.unwrap();
    }

    let libs_root = root_path.join("libs");

    let mut classpath = main_path.to_str().unwrap().to_string();
    #[cfg(target_os = "windows")]
    let sep = ";";
    #[cfg(not(target_os = "windows"))]
    let sep = ":";

    for lib in version_manifest.libraries {
        if lib.rules.iter().all(|rule| rule.allow()) {
            let name = lib.name.replace([';', ':'], "-");
            let lib_path = libs_root.join(if lib.name.ends_with(".jar") {
                name
            } else {
                format!("{}.jar", name)
            });
            classpath += &format!("{}{}", sep, lib_path.to_str().unwrap());
            if !lib_path.exists() {
                // avoid rate limit
                time::sleep(Duration::from_millis(100)).await;
                let lib_source = lib.downloads.artifact.download().await.unwrap();
                if let Some(parent) = lib_path.parent()
                    && !parent.exists()
                {
                    fs::create_dir_all(parent).await.unwrap();
                }
                fs::write(&lib_path, lib_source).await.unwrap();
            }
        }
    }

    println!("cargo:rustc-env=MCJE_JVM_CLASSPATH={classpath}");
}
