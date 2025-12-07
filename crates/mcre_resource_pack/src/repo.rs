use mcre_fs::{FileSystem, PathBuf};

use crate::{PackResult, ResourcePack};

pub struct PackRepo {
    pub packs: Vec<ResourcePack>,
}

impl PackRepo {
    pub async fn load(fs: &impl FileSystem, custom_packs: &[String]) -> PackResult<Self> {
        let path = PathBuf::from("resourcepacks".to_string());
        let entries = fs.read_dir(path.as_ref()).await?;

        let mut packs = Vec::with_capacity(custom_packs.len());

        for entry in entries {
            let name = entry.base().unwrap();
            let Some(id) = name.strip_suffix(".zip") else {
                continue;
            };
            let id = id.to_string();
            let meta = fs.metadata(entry.as_ref()).await?;
            if meta.is_dir {
                continue;
            }

            let Some(idx) = custom_packs.iter().position(|id2| id2 == &id) else {
                continue;
            };

            let pack = ResourcePack::load(fs, id).await?;

            packs.push((idx, pack));
        }

        packs.sort_by_key(|(idx, _)| *idx);

        let mut packs: Vec<ResourcePack> = packs.into_iter().map(|(_, pack)| pack).collect();

        packs.push(ResourcePack::get_vanilla(fs).await?);

        Ok(Self { packs })
    }
}
