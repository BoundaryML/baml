//! Node.js handle lifecycle — released via ObjectFinalize.
//! Mirrors bridge_python/src/handle.rs.

use bridge_ctypes::HANDLE_TABLE;
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

impl ObjectFinalize for BamlHandle {
    fn finalize(self, _env: Env) -> napi::Result<()> {
        HANDLE_TABLE.release(self.key);
        Ok(())
    }
}
