//! Ensure the vanilla client's asset object store is complete before the
//! embedded client starts (the build only fetches the asset index; objects
//! are fetched lazily here so builds and CI never pay the ~480 MB cost).

use std::path::Path;
use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const CONCURRENCY: usize = 16;

pub async fn ensure() -> Result<(), String> {
    let assets_root = Path::new(mcje::ASSETS_DIR);
    let index_path = assets_root
        .join("indexes")
        .join(format!("{}.json", mcje::ASSET_INDEX));
    let bytes = tokio::fs::read(&index_path)
        .await
        .map_err(|err| format!("read {}: {err}", index_path.display()))?;
    let index = mcje_downloader::AssetIndexFile::from_bytes(&bytes)
        .map_err(|err| format!("parse {}: {err}", index_path.display()))?;

    let objects_root = assets_root.join("objects");
    let missing = index.missing(&objects_root);
    if missing.is_empty() {
        return Ok(());
    }
    eprintln!(
        "vanilla_shot: downloading {} asset objects (one-time per version)",
        missing.len()
    );

    let missing_count = missing.len() as u32;
    let client = mcje_downloader::asset_client().map_err(|err| format!("http client: {err}"))?;
    let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
    let mut tasks = JoinSet::new();
    for (dest, object) in missing {
        let client = client.clone();
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            let _permit = semaphore
                .acquire_owned()
                .await
                .expect("asset semaphore closed");
            object.download_to(&client, &dest).await
        });
    }
    while let Some(joined) = tasks.join_next().await {
        let done = missing_count - tasks.len() as u32;
        if done.is_multiple_of(500) {
            eprintln!("vanilla_shot: {done}/{missing_count} asset objects");
        }
        joined.map_err(|err| format!("asset download task failed: {err}"))??;
    }
    eprintln!("vanilla_shot: asset objects ready");
    Ok(())
}
