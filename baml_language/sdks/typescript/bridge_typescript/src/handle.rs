//! Node.js handle lifecycle — released via ObjectFinalize.
//! Mirrors bridge_python/src/handle.rs.

use bridge_cffi::{
    __testonly_seed_function_ref, __testonly_seed_generic_media, BamlCffiStatus, baml_handle_clone,
    baml_handle_release,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;

pub(crate) fn status_to_napi(context: &str, status: BamlCffiStatus) -> napi::Error {
    let detail = match status {
        BamlCffiStatus::Ok => "ok",
        BamlCffiStatus::InvalidHandle => "invalid handle",
        BamlCffiStatus::TypeMismatch => "handle type mismatch",
        BamlCffiStatus::UnsupportedHandleType => "unsupported handle type",
        BamlCffiStatus::InternalError => "internal error",
        BamlCffiStatus::UnexpectedNullptr => "unexpected null pointer",
    };
    napi::Error::new(napi::Status::GenericFailure, format!("{context}: {detail}"))
}

pub(crate) fn handle_clone(key: u64, context: &str) -> napi::Result<u64> {
    let mut out_key = 0;
    match unsafe { baml_handle_clone(key, &mut out_key) } {
        BamlCffiStatus::Ok => Ok(out_key),
        status => Err(status_to_napi(context, status)),
    }
}

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
        let new_key = handle_clone(self.key, "BamlHandle.clone")?;
        Ok(BamlHandle {
            key: new_key,
            handle_type: self.handle_type,
        })
    }

    #[napi(js_name = "_cloneKeyForWire")]
    pub fn clone_key_for_wire(&self) -> napi::Result<HandleKey> {
        let new_key = handle_clone(self.key, "BamlHandle._cloneKeyForWire")?;
        Ok(HandleKey::from_u64(new_key))
    }
}

impl BamlHandle {
    /// Construct directly from a raw `(key, handle_type)` pair. Used by the
    /// media classes — `BamlHandle`'s fields are private, so callers outside
    /// this module need this constructor.
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
        let _ = unsafe { baml_handle_release(self.key) };
        Ok(())
    }
}

/// Test-only: seed a `FunctionRef` entry into `HANDLE_TABLE`, returning
/// `[key, handleType]` so test code can construct a `BamlHandle`.
#[napi(js_name = "_seedFunctionRefHandle")]
pub fn seed_function_ref_handle(global_index: u32) -> napi::Result<(HandleKey, i32)> {
    let mut key = 0;
    let mut handle_type = 0;
    match unsafe { __testonly_seed_function_ref(global_index as u64, &mut key, &mut handle_type) } {
        BamlCffiStatus::Ok => Ok((HandleKey::from_u64(key), handle_type)),
        status => Err(status_to_napi("_seedFunctionRefHandle", status)),
    }
}

/// Test-only: seed an `Adt(Media(generic))` entry into `HANDLE_TABLE`.
#[napi(js_name = "_seedGenericMediaHandle")]
pub fn seed_generic_media_handle() -> napi::Result<(HandleKey, i32)> {
    let mut key = 0;
    let mut handle_type = 0;
    match unsafe { __testonly_seed_generic_media(&mut key, &mut handle_type) } {
        BamlCffiStatus::Ok => Ok((HandleKey::from_u64(key), handle_type)),
        status => Err(status_to_napi("_seedGenericMediaHandle", status)),
    }
}
