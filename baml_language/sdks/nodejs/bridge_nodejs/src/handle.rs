//! Node.js handle lifecycle — released via ObjectFinalize.
//! Mirrors bridge_python/src/py_handle.rs.

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

#[napi(object)]
pub struct HandlePayload {
    pub key: HandleKey,
    pub handle_type: i32,
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

fn status_to_napi_error(
    context: &str,
    key: Option<u64>,
    status: bridge_cffi::BamlCffiStatus,
) -> napi::Error {
    let key_text = key.map(|key| format!(" for key {key}")).unwrap_or_default();
    let reason = match status {
        bridge_cffi::BAML_HANDLE_INVALID_HANDLE => "invalid handle",
        bridge_cffi::BAML_HANDLE_TYPE_MISMATCH => "handle type mismatch",
        bridge_cffi::BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE => "unsupported handle type",
        bridge_cffi::BAML_HANDLE_INTERNAL_ERROR => "internal handle error",
        _ => "unknown handle error",
    };
    napi::Error::new(
        napi::Status::GenericFailure,
        format!("{context}{key_text}: {reason}"),
    )
}

fn ensure_ok(
    context: &str,
    key: Option<u64>,
    status: bridge_cffi::BamlCffiStatus,
) -> napi::Result<()> {
    if status == bridge_cffi::BAML_OK {
        Ok(())
    } else {
        Err(status_to_napi_error(context, key, status))
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
    pub fn new(key: HandleKey, handle_type: i32) -> napi::Result<Self> {
        let key = key.to_u64();
        handle_validate_raw(key, handle_type)?;
        Ok(BamlHandle { key, handle_type })
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
        let payload = handle_clone_raw(self.key, self.handle_type)?;
        let new_key = payload.key.to_u64();
        Ok(BamlHandle {
            key: new_key,
            handle_type: payload.handle_type,
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
        let _ = bridge_cffi::handle_release_impl(self.key, self.handle_type);
        Ok(())
    }
}

fn handle_validate_raw(key: u64, handle_type: i32) -> napi::Result<()> {
    ensure_ok(
        "handleValidate",
        Some(key),
        bridge_cffi::handle_validate_impl(key, handle_type),
    )
}

fn handle_clone_raw(key: u64, handle_type: i32) -> napi::Result<HandlePayload> {
    let mut out_key = 0;
    let mut out_handle_type = 0;
    ensure_ok(
        "handleClone",
        Some(key),
        bridge_cffi::handle_clone_impl(
            key,
            handle_type,
            Some(&mut out_key),
            Some(&mut out_handle_type),
        ),
    )?;
    Ok(HandlePayload {
        key: HandleKey::from_u64(out_key),
        handle_type: out_handle_type,
    })
}

#[napi(js_name = "handleValidate")]
pub fn handle_validate(key: HandleKey, handle_type: i32) -> napi::Result<()> {
    handle_validate_raw(key.to_u64(), handle_type)
}

#[napi(js_name = "handleClone")]
pub fn handle_clone(key: HandleKey, handle_type: i32) -> napi::Result<HandlePayload> {
    handle_clone_raw(key.to_u64(), handle_type)
}

#[napi(js_name = "handleRelease")]
pub fn handle_release(key: HandleKey, handle_type: i32) -> napi::Result<()> {
    let key = key.to_u64();
    ensure_ok(
        "handleRelease",
        Some(key),
        bridge_cffi::handle_release_impl(key, handle_type),
    )
}

#[napi(js_name = "handleType")]
pub fn handle_type(key: HandleKey) -> napi::Result<i32> {
    let key = key.to_u64();
    bridge_cffi::handle_type_impl(key)
        .map(|handle_type| handle_type as i32)
        .map_err(|status| status_to_napi_error("handleType", Some(key), status))
}

/// Validate that `key` exists in `HANDLE_TABLE`, then wrap as a `BamlHandle`.
/// Used by the proto decoder's handle path. Does **not** drain — the entry
/// stays in the table and is owned by the returned `BamlHandle`. Mirrors
/// `bridge_python::py_handle::take_pyhandle_from_table`.
#[napi]
pub fn take_handle_from_table(key: HandleKey, handle_type: i32) -> napi::Result<BamlHandle> {
    let key_u64 = key.to_u64();
    handle_validate_raw(key_u64, handle_type)?;
    Ok(BamlHandle::from_parts(key_u64, handle_type))
}

/// Allocate a fresh `HANDLE_TABLE` row sharing the same `Arc` as `handle`,
/// returning the new key so the caller can stage a wire `BamlHandle`. The
/// original `handle` keeps its key and stays usable. Mirrors
/// `bridge_python::py_handle::put_pyhandle_into_table`.
#[napi]
pub fn put_handle_into_table(handle: &BamlHandle) -> napi::Result<HandleKey> {
    handle_clone_raw(handle.key_u64(), handle.handle_type()).map(|payload| payload.key)
}

/// Test-only: seed a `FunctionRef` entry into `HANDLE_TABLE`, returning
/// `[key, handleType]` so test code can construct a `BamlHandle`.
#[napi(js_name = "_seedFunctionRefHandle")]
pub fn seed_function_ref_handle(global_index: u32) -> (HandleKey, i32) {
    let mut key = 0;
    let mut handle_type = 0;
    let status = bridge_cffi::baml_handle_test_seed_function_ref(
        global_index as u64,
        &mut key,
        &mut handle_type,
    );
    debug_assert_eq!(status, bridge_cffi::BAML_OK);
    (HandleKey::from_u64(key), handle_type)
}

/// Test-only: seed an `Adt(Media(generic))` entry into `HANDLE_TABLE`.
#[napi(js_name = "_seedGenericMediaHandle")]
pub fn seed_generic_media_handle() -> (HandleKey, i32) {
    let mut key = 0;
    let mut handle_type = 0;
    let status = bridge_cffi::baml_handle_test_seed_generic_media(&mut key, &mut handle_type);
    debug_assert_eq!(status, bridge_cffi::BAML_OK);
    (HandleKey::from_u64(key), handle_type)
}
