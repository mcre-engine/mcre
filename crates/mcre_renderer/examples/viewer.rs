#[cfg(not(target_arch = "wasm32"))]
include!("native/viewer_native.rs");

#[cfg(target_arch = "wasm32")]
include!("web/viewer.rs");
