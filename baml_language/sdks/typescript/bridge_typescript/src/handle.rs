//! Node.js handle lifecycle — released via ObjectFinalize.
//! Mirrors bridge_python/src/handle.rs.

use bridge_cffi::{
    __testonly_seed_function_ref, __testonly_seed_generic_media, BamlCffiStatus, baml_handle_clone,
    baml_handle_release,
};
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use napi_derive::napi;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, Ordering},
};

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

pub(crate) const TRANSFER_ADOPTED: u8 = 1;

pub(crate) struct HandleTransferState {
    pub(crate) status: AtomicU8,
    runtime: Mutex<Option<Arc<dyn bex_project::Bex>>>,
    invocation_session: bridge_ctypes::TransferSession<'static>,
}

impl HandleTransferState {
    pub(crate) fn new(
        runtime: Option<Arc<dyn bex_project::Bex>>,
        invocation_session: bridge_ctypes::TransferSession<'static>,
    ) -> Self {
        Self {
            status: AtomicU8::new(0),
            runtime: Mutex::new(runtime),
            invocation_session,
        }
    }
    pub(crate) fn discard(&self) {
        self.status.store(2, Ordering::Release);
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        drop(runtime);
    }
}

/// One table lease, shared by wrapper objects and rooted until adoption.
/// A pending/discarded lease belongs to its receipt, never to its wrappers.
pub(crate) struct HandleOwner {
    key: u64,
    kind: i32,
    state: Option<Arc<HandleTransferState>>,
}

impl HandleOwner {
    fn ensure_active(&self) -> napi::Result<()> {
        if self
            .state
            .as_ref()
            .is_some_and(|s| s.status.load(Ordering::Acquire) != TRANSFER_ADOPTED)
        {
            return Err(napi::Error::from_reason(
                "BAML handle has not been adopted or was discarded",
            ));
        }
        Ok(())
    }
}

impl Drop for HandleOwner {
    fn drop(&mut self) {
        if self.ensure_active().is_ok()
            && self.kind != BamlHandleType::HostValueCallable as i32
            && self.kind != BamlHandleType::HostValueOpaque as i32
        {
            let _ = unsafe { baml_handle_release(self.key) };
        }
        drop(self.state.take());
        bex_project::host_release_dispatch::drain();
    }
}

#[napi]
pub struct BamlHandle {
    owner: Option<Arc<HandleOwner>>,
    kind: i32,
}

#[napi]
impl BamlHandle {
    #[napi(constructor)]
    pub fn new(key: HandleKey, handle_type: i32) -> Self {
        Self::from_parts(key.to_u64(), handle_type)
    }

    #[napi(getter)]
    pub fn key(&self) -> napi::Result<HandleKey> {
        Ok(HandleKey::from_u64(self.key_u64()?))
    }

    #[napi(getter)]
    pub fn handle_type(&self) -> i32 {
        self.kind
    }

    #[napi(js_name = "clone")]
    pub fn clone_handle(&self) -> napi::Result<BamlHandle> {
        let key = handle_clone(self.key_u64()?, "BamlHandle.clone")?;
        Ok(Self {
            owner: Some(Arc::new(HandleOwner {
                key,
                kind: self.kind,
                state: self.lease()?.state.clone(),
            })),
            kind: self.kind,
        })
    }

    /// Release this local lease. Copies and admitted calls keep their owners.
    #[napi]
    pub fn close(&mut self) {
        self.owner.take();
        bex_project::host_release_dispatch::drain();
    }

    #[napi(
        js_name = "_callInterfaceMethod",
        ts_return_type = "Promise<BamlEncodedResult>"
    )]
    pub fn call_interface_method<'e>(
        &self,
        env: &'e napi::Env,
        member: String,
        args_proto: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<napi::bindgen_prelude::PromiseRaw<'e, crate::encoded_result::BamlEncodedResult>>
    {
        let prepared = crate::runtime::prepare_interface_call(self, member, args_proto.as_ref());
        crate::runtime::invoke_async(env, prepared)
    }

    #[napi(
        js_name = "_callOwnedFunction",
        ts_return_type = "Promise<BamlEncodedResult>"
    )]
    pub fn call_owned_function<'e>(
        &self,
        env: &'e napi::Env,
        args_proto: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<napi::bindgen_prelude::PromiseRaw<'e, crate::encoded_result::BamlEncodedResult>>
    {
        let prepared = crate::runtime::prepare_owned_call(self, args_proto.as_ref());
        crate::runtime::invoke_async(env, prepared)
    }

    #[napi(js_name = "_callOwnedFunctionSync")]
    pub fn call_owned_function_sync(
        &self,
        args_proto: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<crate::encoded_result::BamlEncodedResult> {
        let prepared = crate::runtime::prepare_owned_call(self, args_proto.as_ref());
        crate::runtime::invoke_sync(prepared)
    }

    #[napi(js_name = "_cloneKeyForWire")]
    pub fn clone_key_for_wire(&self) -> napi::Result<HandleKey> {
        Ok(HandleKey::from_u64(handle_clone(
            self.key_u64()?,
            "BamlHandle._cloneKeyForWire",
        )?))
    }

    /// Decoder-only lookup: the receipt still owns provisional host references.
    #[napi(js_name = "_hostValueKey")]
    pub fn host_value_key(&self) -> napi::Result<HandleKey> {
        let owner = self.lease()?;
        if owner
            .state
            .as_ref()
            .is_some_and(|s| s.status.load(Ordering::Acquire) == 2)
        {
            return Err(napi::Error::from_reason("BAML handle was discarded"));
        }
        if owner.kind != BamlHandleType::HostReference as i32 {
            return Err(napi::Error::from_reason("Expected an owned host reference"));
        }
        let entry = bridge_ctypes::HANDLE_TABLE
            .resolve(owner.key)
            .ok_or_else(|| napi::Error::from_reason("Invalid host reference"))?;
        match entry.as_ref() {
            bridge_ctypes::CffiHandleTableEntry::HostValue(host) => {
                Ok(HandleKey::from_u64(host.key))
            }
            _ => Err(napi::Error::from_reason("Expected an owned host reference")),
        }
    }
}

impl BamlHandle {
    pub(crate) fn from_parts(key: u64, kind: i32) -> Self {
        Self {
            owner: Some(Arc::new(HandleOwner {
                key,
                kind,
                state: None,
            })),
            kind,
        }
    }
    pub(crate) fn provisional(key: u64, kind: i32, state: Arc<HandleTransferState>) -> Self {
        Self {
            owner: Some(Arc::new(HandleOwner {
                key,
                kind,
                state: Some(state),
            })),
            kind,
        }
    }
    pub(crate) fn share_owner(&self) -> Self {
        Self {
            owner: self.owner.clone(),
            kind: self.kind,
        }
    }
    pub(crate) fn key_u64(&self) -> napi::Result<u64> {
        let owner = self.lease()?;
        owner.ensure_active()?;
        Ok(owner.key)
    }
    pub(crate) fn lease(&self) -> napi::Result<&Arc<HandleOwner>> {
        self.owner
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("BAML reference is closed"))
    }
    pub(crate) fn raw_key(&self) -> Option<u64> {
        self.owner.as_ref().map(|owner| owner.key)
    }
    pub(crate) fn invocation_owner(
        &self,
    ) -> Result<
        (
            Arc<dyn bex_project::Bex>,
            bridge_ctypes::TransferSession<'static>,
        ),
        bridge_cffi::BridgeError,
    > {
        let invalid = |message: String| bridge_cffi::BridgeError::InvalidInvocation(message);
        let owner = self.lease().map_err(|error| invalid(error.to_string()))?;
        owner
            .ensure_active()
            .map_err(|error| invalid(error.to_string()))?;
        let state = owner
            .state
            .as_ref()
            .ok_or_else(|| invalid("BAML reference has no issuing runtime".into()))?;
        if state.invocation_session.is_closed() {
            return Err(invalid(
                "BAML reference's issuing runtime was closed or replaced".into(),
            ));
        }
        let runtime = state
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| invalid("BAML reference has no issuing runtime".into()))?;
        Ok((runtime, state.invocation_session.clone()))
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

/// Test-only: count native table leases without forcing host garbage collection.
#[napi(js_name = "_liveHandleCount")]
pub fn live_handle_count() -> u32 {
    bridge_cffi::handle::live_handle_count() as u32
}
