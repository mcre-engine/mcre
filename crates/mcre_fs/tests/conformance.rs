use mcre_fs::{Fs, FsPath, OpenOptions};

async fn exercise_fs<F: Fs>(fs: &F) {
    let root = FsPath::from("mcre_fs_conformance");
    if fs.exists(&root).await {
        fs.remove_dir_all(&root).await.unwrap();
    }
    let nested = root.join("nested");
    fs.create_dir_all(&nested).await.unwrap();

    let file = nested.join("data.bin");
    fs.write(&file, b"hello").await.unwrap();
    let mut handle = fs
        .open(&file, &OpenOptions::new().read(true).write(true))
        .await
        .unwrap();
    use embedded_io_async::{Read as _, Seek as _, Write as _};
    handle
        .seek(embedded_io_async::SeekFrom::Start(2))
        .await
        .unwrap();
    handle.write_all(b"X").await.unwrap();
    handle.flush().await.unwrap();
    handle
        .seek(embedded_io_async::SeekFrom::End(-2))
        .await
        .unwrap();
    let mut suffix = [0; 2];
    handle.read_exact(&mut suffix).await.unwrap();
    assert_eq!(&suffix, b"lo");
    drop(handle);

    assert_eq!(fs.read(&file).await.unwrap(), b"heXlo");
    let metadata = fs.metadata(&file).await.unwrap();
    assert!(metadata.is_file);
    assert_eq!(metadata.size, 5);

    let appended = root.join("append.bin");
    fs.write(&appended, b"one").await.unwrap();
    let mut append_handle = fs
        .open(&appended, &OpenOptions::new().append(true))
        .await
        .unwrap();
    append_handle.write_all(b" two").await.unwrap();
    append_handle.flush().await.unwrap();
    assert_eq!(fs.read(&appended).await.unwrap(), b"one two");

    let entries = fs.read_dir(&root).await.unwrap();
    assert!(entries.iter().any(|entry| {
        entry.name == "nested" && entry.metadata.is_dir && !entry.metadata.is_file
    }));
    let nested_entries = fs.read_dir(&nested).await.unwrap();
    assert!(nested_entries.iter().any(|entry| {
        entry.name == "data.bin" && entry.metadata.is_file && entry.metadata.size == 5
    }));

    let copied = root.join("copy.bin");
    fs.copy(&file, &copied).await.unwrap();
    let renamed = root.join("renamed.bin");
    fs.rename(&copied, &renamed).await.unwrap();
    assert_eq!(fs.read(&renamed).await.unwrap(), b"heXlo");
    let moved = nested.join("moved.bin");
    fs.rename(&renamed, &moved).await.unwrap();
    assert_eq!(fs.read(&moved).await.unwrap(), b"heXlo");
    fs.remove_file(&moved).await.unwrap();

    let moved_dir = root.join("renamed-dir");
    fs.rename(&nested, &moved_dir).await.unwrap();
    assert!(fs.is_dir(&moved_dir).await);
    assert_eq!(
        fs.read(&moved_dir.join("data.bin")).await.unwrap(),
        b"heXlo"
    );
    fs.remove_file(&appended).await.unwrap();
    fs.remove_dir_all(&root).await.unwrap();
    assert!(!fs.exists(&root).await);
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn native_filesystem_conforms() {
    let temp = tempfile::tempdir().unwrap();
    let fs = mcre_fs::native::NativeFs::new_custom_path(temp.path())
        .await
        .unwrap();
    exercise_fs(&fs).await;
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test(async)]
async fn opfs_filesystem_conforms() {
    let fs = mcre_fs::web::OpfsFs::new().await.unwrap();
    exercise_fs(&fs).await;
}
