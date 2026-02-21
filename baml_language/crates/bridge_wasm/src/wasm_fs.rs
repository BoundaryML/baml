// vfs::VfsMetadata and vfs::FileSystem use std::time::SystemTime; helpers convert to/from it.
#[allow(clippy::disallowed_types)]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use js_sys::{Array, Uint8Array};
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsValue, prelude::*};

use crate::send_wrapper::SendWrapper;

/// Converts from `vfs::FileSystem` trait's `std::time::SystemTime` to millis for JS.
#[allow(clippy::disallowed_types)]
fn system_time_to_millis(t: SystemTime) -> u64 {
    u64::try_from(t.duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap_or(u64::MAX)
}

/// Converts from JS millis to `vfs::VfsMetadata`'s `std::time::SystemTime`.
#[allow(clippy::disallowed_types)]
fn millis_to_system_time(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = r#"{
        readDir: (path: string) => string[];
        createDir: (path: string) => void;
        exists: (path: string) => boolean;
        readFile: (path: string) => Uint8Array;
        writeFile: (path: string, data: Uint8Array) => void;
        metadata: (path: string) => WasmVfsMetadata;
        removeFile: (path: string) => void;
        removeDir: (path: string) => void;
        setTime: (type_: "creation" | "modification" | "access", path: string, time: number) => void;
        copyFile: (src: string, dest: string) => void;
        moveFile: (src: string, dest: string) => void;
        moveDir: (src: string, dest: string) => void;
        readMany: (glob: string) => [string, Uint8Array][];
    }"#)]
    pub type WasmVfs;

    // readDir(path) -> string[]
    #[wasm_bindgen(method, catch, structural, js_name = readDir)]
    fn read_dir(this: &WasmVfs, path: &str) -> Result<Array, JsValue>;

    // createDir(path) -> void
    #[wasm_bindgen(method, catch, structural, js_name = createDir)]
    fn create_dir(this: &WasmVfs, path: &str) -> Result<(), JsValue>;

    // exists(path) -> boolean
    #[wasm_bindgen(method, catch, structural, js_name = exists)]
    fn exists(this: &WasmVfs, path: &str) -> Result<bool, JsValue>;

    // readFile(path) -> Uint8Array
    #[wasm_bindgen(method, catch, structural, js_name = readFile)]
    fn read_file(this: &WasmVfs, path: &str) -> Result<Uint8Array, JsValue>;

    // writeFile(path, data) -> void
    #[wasm_bindgen(method, catch, structural, js_name = writeFile)]
    fn write_file(this: &WasmVfs, path: &str, data: &Uint8Array) -> Result<(), JsValue>;

    // metadata(path) -> { fileType: "file" | "directory", len: number, created?: number, modified?: number, accessed?: number }
    #[wasm_bindgen(method, catch, structural, js_name = metadata)]
    fn metadata(this: &WasmVfs, path: &str) -> Result<WasmVfsMetadata, JsValue>;

    // removeFile(path) -> void
    #[wasm_bindgen(method, catch, structural, js_name = removeFile)]
    fn remove_file(this: &WasmVfs, path: &str) -> Result<(), JsValue>;

    // removeDir(path) -> void
    #[wasm_bindgen(method, catch, structural, js_name = removeDir)]
    fn remove_dir(this: &WasmVfs, path: &str) -> Result<(), JsValue>;

    // setTime(type_: "creation" | "modification" | "access", path: string, time: SystemTime) -> void
    #[wasm_bindgen(method, catch, structural, js_name = setTime)]
    fn set_time(this: &WasmVfs, type_: &str, path: &str, time: u64) -> Result<(), JsValue>;

    // copyFile(src: string, dest: string) -> void
    #[wasm_bindgen(method, catch, structural, js_name = copyFile)]
    fn copy_file(this: &WasmVfs, src: &str, dest: &str) -> Result<(), JsValue>;

    // moveFile(src: string, dest: string) -> void
    #[wasm_bindgen(method, catch, structural, js_name = moveFile)]
    fn move_file(this: &WasmVfs, src: &str, dest: &str) -> Result<(), JsValue>;

    // moveDir(src: string, dest: string) -> void
    #[wasm_bindgen(method, catch, structural, js_name = moveDir)]
    fn move_dir(this: &WasmVfs, src: &str, dest: &str) -> Result<(), JsValue>;

    // readMany(glob) -> [string, Uint8Array][]
    #[wasm_bindgen(method, catch, structural, js_name = readMany)]
    fn read_many(this: &WasmVfs, glob: &str) -> Result<Array, JsValue>;
}

#[derive(tsify::Tsify, Serialize, Deserialize)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct WasmVfsMetadata {
    pub file_type: String,
    pub len: u64,
    pub created: Option<u64>,
    pub modified: Option<u64>,
    pub accessed: Option<u64>,
}

#[derive(Clone)]
pub(super) struct WasmFs {
    vfs: SendWrapper<std::sync::Arc<WasmVfs>>,
}

impl std::fmt::Debug for WasmFs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmFs").finish()
    }
}

impl WasmFs {
    #[allow(clippy::new_ret_no_self, clippy::arc_with_non_send_sync)]
    pub(super) fn new(wasm_vfs: WasmVfs) -> Box<dyn bex_project::BulkReadFileSystem> {
        Box::new(Self {
            vfs: SendWrapper::new(std::sync::Arc::new(wasm_vfs)),
        })
    }
}

fn js_err_to_vfs(e: &JsValue) -> vfs::VfsError {
    vfs::VfsError::from(vfs::error::VfsErrorKind::Other(format!("{e:?}")))
}

/// Write buffer that flushes accumulated bytes to the JS VFS on drop.
struct VfsWriteBuffer {
    path: String,
    cursor: std::io::Cursor<Vec<u8>>,
    vfs: SendWrapper<std::sync::Arc<WasmVfs>>,
}

impl std::io::Write for VfsWriteBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.cursor.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let data = Uint8Array::from(self.cursor.get_ref().as_slice());
        self.vfs
            .write_file(&self.path, &data)
            .map_err(|e| std::io::Error::other(format!("{e:?}")))
    }
}

impl std::io::Seek for VfsWriteBuffer {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(pos)
    }
}

impl Drop for VfsWriteBuffer {
    fn drop(&mut self) {
        let _ = std::io::Write::flush(self);
    }
}

// SAFETY: wasm32-unknown-unknown is single-threaded.
#[allow(unsafe_code)]
unsafe impl Send for VfsWriteBuffer {}

impl vfs::FileSystem for WasmFs {
    fn read_dir(&self, path: &str) -> vfs::VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        let result = self
            .vfs
            .read_dir(path)
            .map_err(|e| js_err_to_vfs(&e))?
            .into_iter()
            .map(|s| {
                s.as_string().ok_or_else(|| {
                    vfs::VfsError::from(vfs::error::VfsErrorKind::Other(
                        "String is not a string".to_string(),
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Box::new(result.into_iter()))
    }

    fn create_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.vfs.create_dir(path).map_err(|e| js_err_to_vfs(&e))
    }

    fn open_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndRead + Send>> {
        let bytes_js = self.vfs.read_file(path).map_err(|e| js_err_to_vfs(&e))?;
        let bytes = bytes_js.to_vec();
        Ok(Box::new(std::io::Cursor::new(bytes)))
    }

    fn create_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        Ok(Box::new(VfsWriteBuffer {
            path: path.to_string(),
            cursor: std::io::Cursor::new(Vec::new()),
            vfs: self.vfs.clone(),
        }))
    }

    fn append_file(&self, path: &str) -> vfs::VfsResult<Box<dyn vfs::SeekAndWrite + Send>> {
        let existing = match self.vfs.read_file(path) {
            Ok(bytes_js) => bytes_js.to_vec(),
            Err(_) => Vec::new(),
        };
        let len = existing.len() as u64;
        let mut cursor = std::io::Cursor::new(existing);
        cursor.set_position(len);
        Ok(Box::new(VfsWriteBuffer {
            path: path.to_string(),
            cursor,
            vfs: self.vfs.clone(),
        }))
    }

    fn metadata(&self, path: &str) -> vfs::VfsResult<vfs::VfsMetadata> {
        let result = self.vfs.metadata(path).map_err(|e| js_err_to_vfs(&e))?;
        Ok(vfs::VfsMetadata {
            file_type: match result.file_type.as_str() {
                "file" => vfs::path::VfsFileType::File,
                "directory" => vfs::path::VfsFileType::Directory,
                other => {
                    return Err(vfs::VfsError::from(vfs::error::VfsErrorKind::Other(
                        format!("Invalid file type: {other}"),
                    )));
                }
            },
            len: result.len,
            created: result.created.map(millis_to_system_time),
            modified: result.modified.map(millis_to_system_time),
            accessed: result.accessed.map(millis_to_system_time),
        })
    }

    fn exists(&self, path: &str) -> vfs::VfsResult<bool> {
        self.vfs.exists(path).map_err(|e| js_err_to_vfs(&e))
    }

    fn remove_file(&self, path: &str) -> vfs::VfsResult<()> {
        self.vfs.remove_file(path).map_err(|e| js_err_to_vfs(&e))
    }

    fn remove_dir(&self, path: &str) -> vfs::VfsResult<()> {
        self.vfs.remove_dir(path).map_err(|e| js_err_to_vfs(&e))
    }

    /// [`vfs::FileSystem`] trait requires [`std::time::SystemTime`] in the signature.
    #[allow(clippy::disallowed_types)]
    fn set_creation_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.vfs
            .set_time("creation", path, system_time_to_millis(time))
            .map_err(|e| js_err_to_vfs(&e))
    }

    /// [`vfs::FileSystem`] trait requires [`std::time::SystemTime`] in the signature.
    #[allow(clippy::disallowed_types)]
    fn set_modification_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.vfs
            .set_time("modification", path, system_time_to_millis(time))
            .map_err(|e| js_err_to_vfs(&e))
    }

    /// [`vfs::FileSystem`] trait requires [`std::time::SystemTime`] in the signature.
    #[allow(clippy::disallowed_types)]
    fn set_access_time(&self, path: &str, time: std::time::SystemTime) -> vfs::VfsResult<()> {
        self.vfs
            .set_time("access", path, system_time_to_millis(time))
            .map_err(|e| js_err_to_vfs(&e))
    }

    fn copy_file(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.vfs.copy_file(src, dest).map_err(|e| js_err_to_vfs(&e))
    }

    fn move_file(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.vfs.move_file(src, dest).map_err(|e| js_err_to_vfs(&e))
    }

    fn move_dir(&self, src: &str, dest: &str) -> vfs::VfsResult<()> {
        self.vfs.move_dir(src, dest).map_err(|e| js_err_to_vfs(&e))
    }
}

impl bex_project::BulkReadFileSystem for WasmFs {
    fn read_many(&self, glob: &str) -> vfs::VfsResult<Vec<(String, Vec<u8>)>> {
        use wasm_bindgen::JsCast;

        let entries = self.vfs.read_many(glob).map_err(|e| js_err_to_vfs(&e))?;
        let mut results = Vec::new();
        for item in entries.iter() {
            let tuple: Array = item.dyn_into().map_err(|_| {
                vfs::VfsError::from(vfs::error::VfsErrorKind::Other(
                    "readMany entry is not an array".into(),
                ))
            })?;
            let path = tuple.get(0).as_string().ok_or_else(|| {
                vfs::VfsError::from(vfs::error::VfsErrorKind::Other(
                    "readMany entry path is not a string".into(),
                ))
            })?;
            let bytes_js: Uint8Array = tuple.get(1).dyn_into().map_err(|_| {
                vfs::VfsError::from(vfs::error::VfsErrorKind::Other(
                    "readMany entry data is not a Uint8Array".into(),
                ))
            })?;
            results.push((path, bytes_js.to_vec()));
        }
        Ok(results)
    }
}
