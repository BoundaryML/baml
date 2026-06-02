//! Node.js handle lifecycle — released via ObjectFinalize.
//! Mirrors bridge_python/src/handle.rs.

use bex_project::{BexExternalAdt, MediaKind, MediaValue};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE};
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// A u64 handle key split into two i32 halves, mirroring the shape of
/// protobufjs's `Long` type (`{ low: number, high: number }`).
///
/// JavaScript has no convenient native representation for u64 — `number` is
/// f64 and silently loses precision above 2^53. Rather than forcing BigInt
/// (which many JS libraries don't handle well), we pass the key as two 32-bit
/// halves that are layout-compatible with `Long`. This means a protobufjs
/// `Long` decoded from a uint64 proto field can be handed directly to the
/// `BamlHandle` constructor, and the `HandleKey` returned by the getter can
/// be passed straight back into a proto uint64 field — no conversions needed.
#[napi(object)]
pub struct HandleKey {
    pub low: i32,
    pub high: i32,
}

impl HandleKey {
    pub fn to_u64(&self) -> u64 {
        ((self.high as u32 as u64) << 32) | (self.low as u32 as u64)
    }

    pub fn from_u64(v: u64) -> Self {
        HandleKey {
            low: v as i32,
            high: (v >> 32) as i32,
        }
    }
}

/// Base class for all opaque BAML handles.
///
/// When the Node.js garbage collector finalizes an instance, `finalize`
/// releases the corresponding entry from the global handle table.
#[napi(custom_finalize)]
pub struct BamlHandle {
    key: u64,
    handle_type: i32,
}

#[napi]
impl BamlHandle {
    #[napi(constructor)]
    pub fn new(key: HandleKey, handle_type: i32) -> Self {
        BamlHandle {
            key: key.to_u64(),
            handle_type,
        }
    }

    #[napi(getter)]
    pub fn key(&self) -> HandleKey {
        HandleKey::from_u64(self.key)
    }

    #[napi(getter)]
    pub fn handle_type(&self) -> i32 {
        self.handle_type
    }

    #[napi(js_name = "clone")]
    pub fn clone_handle(&self) -> napi::Result<BamlHandle> {
        let new_key = HANDLE_TABLE.clone_handle(self.key).ok_or_else(|| {
            napi::Error::new(napi::Status::GenericFailure, "Handle is no longer valid")
        })?;
        Ok(BamlHandle {
            key: new_key,
            handle_type: self.handle_type,
        })
    }
}

impl BamlHandle {
    /// Construct directly from a raw `(key, handle_type)` pair. Used by the
    /// handle-table helpers and the media classes — `BamlHandle`'s fields are
    /// private, so callers outside this module need this constructor.
    pub(crate) fn from_parts(key: u64, handle_type: i32) -> Self {
        BamlHandle { key, handle_type }
    }

    /// Read the raw u64 table key (the wire field is the `{low, high}` split).
    pub(crate) fn key_u64(&self) -> u64 {
        self.key
    }
}

impl ObjectFinalize for BamlHandle {
    fn finalize(self, _env: Env) -> napi::Result<()> {
        HANDLE_TABLE.release(self.key);
        Ok(())
    }
}

/// Validate that `key` exists in `HANDLE_TABLE`, then wrap as a `BamlHandle`.
/// Used by the proto decoder's handle path. Does **not** drain — the entry
/// stays in the table and is owned by the returned `BamlHandle`. Mirrors
/// `bridge_python::py_handle::take_pyhandle_from_table`.
#[napi]
pub fn take_handle_from_table(key: HandleKey, handle_type: i32) -> napi::Result<BamlHandle> {
    let key_u64 = key.to_u64();
    if HANDLE_TABLE.resolve(key_u64).is_none() {
        return Err(napi::Error::new(
            napi::Status::GenericFailure,
            format!("BAML handle key {key_u64} is not in HANDLE_TABLE"),
        ));
    }
    Ok(BamlHandle::from_parts(key_u64, handle_type))
}

/// Allocate a fresh `HANDLE_TABLE` row sharing the same `Arc` as `handle`,
/// returning the new key so the caller can stage a wire `BamlHandle`. The
/// original `handle` keeps its key and stays usable. Mirrors
/// `bridge_python::py_handle::put_pyhandle_into_table`.
#[napi]
pub fn put_handle_into_table(handle: &BamlHandle) -> napi::Result<HandleKey> {
    let new_key = HANDLE_TABLE.clone_handle(handle.key_u64()).ok_or_else(|| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("BamlHandle key {} is not in HANDLE_TABLE", handle.key_u64()),
        )
    })?;
    Ok(HandleKey::from_u64(new_key))
}

/// Test-only: seed a `FunctionRef` entry into `HANDLE_TABLE`, returning
/// `[key, handleType]` so test code can construct a `BamlHandle`.
#[napi(js_name = "_seedFunctionRefHandle")]
pub fn seed_function_ref_handle(global_index: u32) -> (HandleKey, i32) {
    let entry = CffiHandleTableEntry::FunctionRef {
        global_index: global_index as usize,
    };
    let ht = entry.handle_type();
    let key = HANDLE_TABLE.insert(entry);
    (HandleKey::from_u64(key), ht as i32)
}

/// Test-only: seed an `Adt(Media(generic))` entry into `HANDLE_TABLE`.
#[napi(js_name = "_seedGenericMediaHandle")]
pub fn seed_generic_media_handle() -> (HandleKey, i32) {
    let media = MediaValue::from_url(MediaKind::Generic, "https://example.com/", None);
    let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(media));
    let ht = entry.handle_type();
    let key = HANDLE_TABLE.insert(entry);
    (HandleKey::from_u64(key), ht as i32)
}
