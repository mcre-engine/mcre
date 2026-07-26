mod file;
mod fs;

pub use file::NativeFile;
pub use fs::NativeFs;

use std::path::PathBuf;

pub fn app_data_dir(app_name: &str) -> crate::Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| crate::FsError::NotSupported(
        "Could not determine data directory".into(),
    ))?;
    Ok(base.join(app_name))
}
