# Filesystem Architecture

This document describes the filesystem abstraction used by MCRE. It focuses on how the game accesses files across platforms (native desktop, web/WASM) without leaking platform-specific details into game logic.

The goal of this abstraction is to support:

* native file access via `tokio::fs`
* web file access via the Origin Private File System (OPFS)
* platform-agnostic game code that works on both
* async I/O without blocking the game loop

All examples are written in Rust-like code. Types and APIs are illustrative and may evolve.

## Design goals

The filesystem layer is built around a few constraints:

* Game code never touches `std::fs` or platform APIs directly
* A single `Fs` trait provides all file operations
* The backend is selected at startup, not per-call
* Paths are platform-agnostic (no `/` or `\` assumptions)
* All operations are async

This leads naturally to a trait-based abstraction with platform-specific implementations.

## Core concepts

At a high level there are five concepts:

1. `FsPath`: a platform-agnostic path composed of parts
2. `Fs`: a trait defining all filesystem operations
3. `NativeFs`: backend using `tokio::fs` (desktop)
4. `OpfsFs`: backend using OPFS via the `opfs` crate (web)
5. `FsError`: error type implementing `embedded_io_async::Error`

Game code is generic over `Fs`. It never knows which backend it is using.

## FsPath

A path in MCRE is a sequence of components without separators.

```rust
pub struct FsPath {
    parts: Vec<Box<str>>,
}
```

This avoids the problem of `/` vs `\` vs OPFS conventions. Paths are constructed by appending components:

```rust
let path = FsPath::new(vec!["saves".into(), "world1".into(), "level.dat".into()]);
```

Or from a string (splits on `/`):

```rust
let path = FsPath::from("saves/world1/level.dat");
```

FsPath can convert to:

* `std::path::PathBuf` via `to_std_path()` for native platforms
* `String` via `to_opfs_string()` for OPFS/web (forward slashes)

Important properties:

* Paths are always relative to the filesystem root
* No leading or trailing separators
* No normalization or `..` resolution (callers are responsible)
* Paths are cheap to clone (`Vec<Box<str>>`)

## Fs trait

The `Fs` trait defines all operations available on a filesystem:

```rust
pub trait Fs: Send + Sync {
    type File: Read<Error = FsError>
        + Write<Error = FsError>
        + Seek<Error = FsError>
        + Send
        + Sync;

    async fn open(&self, path: &FsPath, options: &OpenOptions) -> Result<Self::File>;
    async fn create_dir_all(&self, path: &FsPath) -> Result<()>;
    async fn remove_file(&self, path: &FsPath) -> Result<()>;
    async fn remove_dir_all(&self, path: &FsPath) -> Result<()>;
    async fn rename(&self, from: &FsPath, to: &FsPath) -> Result<()>;
    async fn metadata(&self, path: &FsPath) -> Result<Metadata>;
    async fn read_dir(&self, path: &FsPath) -> Result<Vec<DirEntry>>;
    // ... plus provided methods: read, write, read_to_string, etc.
}
```

The trait provides several convenience methods:

* `create()` — opens for writing with create + truncate
* `open_read()` — opens for reading
* `open_write()` — opens for writing with create + truncate
* `read()` — reads entire file into `Vec<u8>`
* `read_to_string()` — reads entire file into `String`
* `write()` — writes bytes to a file
* `write_string()` — writes a string to a file
* `copy()` — reads from one path and writes to another
* `exists()` — checks if a path exists
* `is_file()` / `is_dir()` — checks the type of a path

The `File` associated type must implement `embedded_io_async` traits (`Read`, `Write`, `Seek`), providing a common async I/O interface.

## FsError

Errors use `thiserror` and implement `embedded_io_async::Error`:

```rust
pub enum FsError {
    Io(std::io::Error),
    NotFound { path: String },
    PermissionDenied { path: String },
    AlreadyExists { path: String },
    InvalidPath { reason: String },
    NotSupported(String),
    // ...
}
```

The `embedded_io_async::Error` implementation maps each variant to the appropriate `ErrorKind`.

## NativeFs

The native backend wraps `tokio::fs`:

```rust
pub struct NativeFs {
    root: PathBuf,
}
```

It resolves `FsPath` to an absolute path by joining with `root`:

```rust
fn resolve(&self, path: &FsPath) -> PathBuf {
    self.root.join(path.to_std_path())
}
```

Creation:

```rust
// From the OS-appropriate data directory
let fs = NativeFs::new_app_root("com.mcre").await?;

// From a custom path
let fs = NativeFs::new_custom_path("/tmp/mcre-test").await?;

// From an absolute path (validates absoluteness)
let fs = NativeFs::new_absolute(PathBuf::from("/data/mcre")).await?;
```

The `app_data_dir` function determines the root:

* macOS: `~/Library/Application Support/<app_name>`
* Linux: `~/.local/share/<app_name>`
* Windows: `%APPDATA%\<app_name>`

NativeFs is fully async via tokio. All operations map directly to tokio equivalents.

## OpfsFs

The web backend uses the `opfs` crate, which provides:

* OPFS access on web/WASM platforms
* `tokio::fs` on native platforms (though we use `NativeFs` directly there)

```rust
pub struct OpfsFs {
    root: opfs::persistent::DirectoryHandle,
}
```

Creation:

```rust
let fs = OpfsFs::new().await?;

// Or with a subdirectory
let fs = OpfsFs::with_subdir("mcre-data").await?;
```

The `opfs` crate handles the browser API complexity (navigator, workers, etc.) behind a type-safe Rust API.

Limitations:

* Seek is not supported (returns `FsError::NotSupported`)
* Cross-directory rename is not supported
* Metadata always returns `size: 0` (OPFS does not expose file size easily)
* Directory entries are always marked as files (OPFS does not distinguish)

These limitations are acceptable for MCRE's use case (reading resource packs, config files, save data).

## Platform selection

The backend is selected at application startup. Game code is generic:

```rust
async fn load_world(fs: &impl Fs, name: &str) -> Result<WorldData> {
    let path = FsPath::from("saves").with(name).with("level.dat");
    let data = fs.read(&path).await?;
    Ok(WorldData::parse(&data))
}
```

On desktop:

```rust
let fs = NativeFs::new_app_root("com.mcre").await?;
load_world(&fs, "my-world").await?;
```

On web:

```rust
let fs = OpfsFs::new().await?;
load_world(&fs, "my-world").await?;
```

The same `load_world` function works on both platforms. No conditional compilation is needed in game code.

## File handles

The `File` associated type on each backend implements `embedded_io_async` traits:

```rust
// NativeFile wraps tokio::fs::File
impl Read for NativeFile { ... }
impl Write for NativeFile { ... }
impl Seek for NativeFile { ... }

// OpfsFile wraps opfs::persistent::FileHandle
impl Read for OpfsFile { ... }
impl Write for OpfsFile { ... }
impl Seek for OpfsFile { ... }  // returns NotSupported
```

This means game code can read/write files using standard embedded I/O traits without knowing the backend.

## OpenOptions

File opening is controlled by `OpenOptions`:

```rust
pub struct OpenOptions {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
}
```

Built with a builder pattern:

```rust
let opts = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true);

let file = fs.open(&path, &opts).await?;
```

This maps to:

* `tokio::fs::OpenOptions` on native
* `GetFileHandleOptions { create: true }` on OPFS

## Error handling

All filesystem operations return `Result<T>` where `T` is the operation-specific type and the error is `FsError`.

Callers handle errors explicitly:

```rust
match fs.read(&path).await {
    Ok(data) => process(data),
    Err(FsError::NotFound { path }) => create_default(&path),
    Err(FsError::Io(e)) => log_io_error(e),
    Err(e) => return Err(e.into()),
}
```

Or with the `?` operator for propagation.

## What this layer does not do

Intentionally omitted:

* networking
* in-memory virtual filesystem
* file locking
* file watching / notifications
* atomic writes (beyond what the OS provides)
* caching (callers can cache if needed)

Those concerns sit above or beside the filesystem layer.

## Summary

* `FsPath` is a platform-agnostic path type (no separators)
* `Fs` is the single trait all game code uses
* `NativeFs` wraps `tokio::fs` for desktop
* `OpfsFs` wraps `opfs` for web/WASM
* `FsError` provides structured error handling
* File handles implement `embedded_io_async` traits
* Game code is generic over `Fs`, never touches platform APIs
* The backend is selected once at startup
