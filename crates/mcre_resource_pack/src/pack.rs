use std::io::Cursor;

use mcre_fs::{FileSystem, PathBuf};
use zip::ZipArchive;

use crate::PackResult;

pub struct ResourcePack {
    pub id: String,
    pub archive: ZipArchive<Cursor<Box<[u8]>>>,
}

impl ResourcePack {
    pub async fn load(fs: &impl FileSystem, id: String) -> PackResult<Self> {
        let path = PathBuf::from(format!("resourcepacks/{}", id));
        let data = fs.read(path.as_ref()).await?;
        let cursor = Cursor::new(data);

        let archive = ZipArchive::new(cursor)?;

        Ok(Self { id, archive })
    }

    pub async fn get_vanilla(fs: &impl FileSystem) -> PackResult<Self> {
        const DOWNLOAD_URL: &str =
            "https://cdn.jsdelivr.net/gh/mcre-project/minecraft-vanilla-pack@main/vanilla.zip";

        let vanilla_path = PathBuf::from("resourcepacks/vanilla.zip".to_string());

        if fs.exists(vanilla_path.as_ref()).await {
            return Self::load(fs, "vanilla".to_string()).await;
        }

        let pack_zip = reqwest::get(DOWNLOAD_URL)
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec()
            .into_boxed_slice();

        fs.write(vanilla_path.as_ref(), &pack_zip).await?;

        let cursor = Cursor::new(pack_zip);

        let archive = ZipArchive::new(cursor)?;

        Ok(Self {
            id: "vanilla".to_string(),
            archive,
        })
    }
}
