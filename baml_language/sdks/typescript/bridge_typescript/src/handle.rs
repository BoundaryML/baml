//! Node.js handle lifecycle — released via ObjectFinalize.
//! Mirrors bridge_python/src/handle.rs.

use bridge_cffi::{BamlCffiStatus, handle_cffi};
use bridge_ctypes::WireHandle;
use napi::bindgen_prelude::*;
use napi_derive::napi;

pub(crate) fn status_to_napi(context: &str, status: BamlCffiStatus) -> napi::Error {
    let detail = status.description();
    napi::Error::new(napi::Status::GenericFailure, format!("{context}: {detail}"))
}

pub(crate) fn handle_clone(key: u64, context: &str) -> napi::Result<u64> {
    handle_cffi::clone_handle(key).map_err(|error| status_to_napi(context, error.into()))
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
        let new_key = self.key_for_wire("BamlHandle.clone")?;
        Ok(BamlHandle {
            key: new_key,
            handle_type: self.handle_type,
        })
    }

    /// Clone this handle for inbound wire ownership. A host value is the
    /// engine's reference to the host's own object: its key goes as it is,
    /// nothing is cloned.
    #[napi(js_name = "_cloneKeyForWire")]
    pub fn clone_key_for_wire(&self) -> napi::Result<HandleKey> {
        Ok(HandleKey::from_u64(
            self.key_for_wire("BamlHandle._cloneKeyForWire")?,
        ))
    }

    /// Whether this handle is the engine's reference to a host object, as
    /// opposed to the host's reference to an engine object.
    #[napi(getter, js_name = "isHostValue")]
    pub fn is_host_value(&self) -> bool {
        matches!(self.wire_handle(), Ok(WireHandle::Host(..)))
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

    /// Which reference this handle is: the host's reference to an engine
    /// object, or the engine's reference to a host object.
    fn wire_handle(&self) -> napi::Result<WireHandle> {
        WireHandle::from_parts(self.key, self.handle_type)
            .map_err(|error| napi::Error::new(napi::Status::InvalidArg, error.to_string()))
    }

    /// The key one more wire occurrence of this handle carries: a fresh
    /// engine reference for an engine object, the key itself for a host value.
    fn key_for_wire(&self, context: &str) -> napi::Result<u64> {
        match self.wire_handle()? {
            WireHandle::Host(key, _) => Ok(key.get()),
            WireHandle::Engine(_) => handle_clone(self.key, context),
        }
    }
}

impl ObjectFinalize for BamlHandle {
    fn finalize(self, _env: Env) -> napi::Result<()> {
        // A host value is released through the host-value registry; only an
        // engine reference is this wrapper's to release.
        if let Ok(WireHandle::Engine(key)) = self.wire_handle() {
            let _ = handle_cffi::release_handle(key.get());
        }
        Ok(())
    }
}

/// Release one owned wire reference that never reached a host wrapper.
#[napi(js_name = "_releaseWireHandle")]
pub fn release_wire_handle(key: HandleKey) -> napi::Result<()> {
    handle_cffi::release_handle(key.to_u64())
        .map_err(|error| status_to_napi("host-call decode rollback", error.into()))
}

/// Test-only: seed an `Adt(Media(generic))` entry into `HANDLE_TABLE`.
#[napi(js_name = "_seedGenericMediaHandle")]
pub fn seed_generic_media_handle() -> napi::Result<(HandleKey, i32)> {
    let parts = handle_cffi::seed_generic_media_handle();
    Ok((HandleKey::from_u64(parts.key), parts.handle_type))
}

/// Test-only: seed an engine-heap (`BexHeapHandle`) entry into `HANDLE_TABLE`
/// — the identity-bearing, deduplicating arm — returning `[key, handleType]`.
/// Two seeds of one `slabKey` share a key.
#[napi(js_name = "_seedHeapHandle")]
pub fn seed_heap_handle(slab_key: u32) -> napi::Result<(HandleKey, i32)> {
    let parts = handle_cffi::seed_heap_handle(u64::from(slab_key));
    Ok((HandleKey::from_u64(parts.key), parts.handle_type))
}

/// Test-only: the number of live `HANDLE_TABLE` rows (a refcounted engine-heap
/// row counts once however many owners it has).
#[napi(js_name = "_liveHandleCount")]
pub fn live_handle_count() -> u32 {
    u32::try_from(bridge_cffi::handle_cffi::live_handle_count()).unwrap_or(u32::MAX)
}

/// Test-only: the outstanding ownership count of a live key — the releases it
/// still owes — or `null` for a dead/unknown key. Lets an audit see an
/// exactly-once imbalance on a shared engine-heap key, which row counts hide.
#[napi(js_name = "_handleRefcount")]
pub fn handle_refcount(key: HandleKey) -> Option<u32> {
    bridge_cffi::handle_cffi::handle_refcount(key.to_u64())
        .map(|count| u32::try_from(count).unwrap_or(u32::MAX))
}
