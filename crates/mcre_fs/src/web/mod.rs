mod file;
mod fs;

pub use file::OpfsFile;
pub use fs::OpfsFs;

#[cfg(target_arch = "wasm32")]
pub(crate) fn map_error(error: wasm_bindgen::JsValue) -> crate::FsError {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;

    let name = js_sys::Reflect::get(&error, &JsValue::from_str("name"))
        .ok()
        .and_then(|value| value.as_string())
        .unwrap_or_default();
    let message = js_sys::Reflect::get(&error, &JsValue::from_str("message"))
        .ok()
        .and_then(|value| value.as_string())
        .or_else(|| error.as_string())
        .unwrap_or_else(|| format!("{error:?}"));

    match name.as_str() {
        "NotFoundError" => crate::FsError::NotFound { path: message },
        "QuotaExceededError" => crate::FsError::QuotaExceeded,
        "NotAllowedError" | "SecurityError" => crate::FsError::PermissionDenied { path: message },
        "TypeMismatchError" => crate::FsError::NotADirectory { path: message },
        "InvalidModificationError" => crate::FsError::DirectoryNotEmpty { path: message },
        _ if error.is_instance_of::<js_sys::Error>() => {
            crate::FsError::Io(std::io::Error::other(message))
        }
        _ => crate::FsError::Custom(format!("OPFS {name}: {message}")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn map_error(error: std::io::Error) -> crate::FsError {
    match error.kind() {
        std::io::ErrorKind::NotFound => crate::FsError::NotFound {
            path: error.to_string(),
        },
        std::io::ErrorKind::PermissionDenied => crate::FsError::PermissionDenied {
            path: error.to_string(),
        },
        std::io::ErrorKind::AlreadyExists => crate::FsError::AlreadyExists {
            path: error.to_string(),
        },
        _ => crate::FsError::Io(error),
    }
}
