//! Own result leases until synchronous JS decoding adopts the whole aggregate.
use crate::handle::{BamlHandle, HandleKey, HandleOwner, HandleTransferState, TRANSFER_ADOPTED};
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use bridge_ctypes::{EncodedTransfer, PendingDelivery, TransferSession};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::{Arc, atomic::Ordering};

#[napi]
pub struct BamlEncodedResult {
    pending: Option<PendingDelivery<'static, Vec<u8>>>,
    session: TransferSession<'static>,
    state: Option<Arc<HandleTransferState>>,
    // Native owners remain rooted even if JS collects a provisional wrapper.
    handles: Vec<Arc<HandleOwner>>,
    claims: Vec<u64>,
}

impl BamlEncodedResult {
    pub(crate) fn new(
        encoded: EncodedTransfer<'static, Vec<u8>>,
        session: TransferSession<'static>,
        runtime: Option<Arc<dyn bex_project::Bex>>,
    ) -> napi::Result<Self> {
        Self::with_invocation_session(encoded, session.clone(), runtime, session)
    }

    pub(crate) fn with_invocation_session(
        encoded: EncodedTransfer<'static, Vec<u8>>,
        session: TransferSession<'static>,
        runtime: Option<Arc<dyn bex_project::Bex>>,
        invocation_session: TransferSession<'static>,
    ) -> napi::Result<Self> {
        let pending = session
            .stage(encoded)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(Self {
            pending: Some(pending),
            session,
            state: Some(Arc::new(HandleTransferState::new(
                runtime,
                invocation_session,
            ))),
            handles: vec![],
            claims: vec![],
        })
    }
    pub(crate) fn abort_guard(&self) -> DeliveryAbortGuard {
        DeliveryAbortGuard {
            state: self.state.clone(),
            session: self.session.clone(),
            receipt: self.pending.as_ref().map(|pending| pending.receipt()),
        }
    }
    fn discard_inner(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        state.discard();
        let pending = self.pending.take();
        self.handles.clear();
        self.claims.clear();
        drop(pending);
    }
}

struct DrainHostReleases;
impl Drop for DrainHostReleases {
    fn drop(&mut self) {
        bex_project::host_release_dispatch::drain();
    }
}

#[napi]
impl BamlEncodedResult {
    #[napi(getter)]
    pub fn payload(&self) -> napi::Result<Buffer> {
        self.pending
            .as_ref()
            .map(|p| Buffer::from(p.payload().clone()))
            .ok_or_else(|| napi::Error::from_reason("BAML result was already consumed"))
    }
    #[napi(js_name = "_wrapHandle")]
    pub fn wrap_handle(&mut self, key: HandleKey, kind: i32) -> napi::Result<BamlHandle> {
        let state = self
            .state
            .as_ref()
            .ok_or_else(|| napi::Error::from_reason("BAML result was already consumed"))?;
        self.handles.reserve(1);
        self.claims.reserve(1);
        let key = key.to_u64();
        let handle = BamlHandle::provisional(key, kind, state.clone());
        self.handles.push(handle.lease()?.clone());
        if kind != BamlHandleType::HostValueCallable as i32
            && kind != BamlHandleType::HostValueOpaque as i32
        {
            self.claims.push(key);
        }
        Ok(handle)
    }
    #[napi(js_name = "_adopt")]
    pub fn adopt(&mut self) -> napi::Result<()> {
        let _drain = DrainHostReleases;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let pending = self
                .pending
                .as_ref()
                .ok_or_else(|| napi::Error::from_reason("BAML result was already consumed"))?;
            self.session
                .adopt(pending.receipt(), &self.claims)
                .map_err(|e| napi::Error::from_reason(e.to_string()))?;
            if let Some(state) = self.state.take() {
                state.status.store(TRANSFER_ADOPTED, Ordering::Release);
            }
            self.pending.take();
            self.handles.clear();
            self.claims.clear();
            Ok(())
        }))
        .unwrap_or_else(|_| {
            Err(napi::Error::from_reason(
                "Resource cleanup panicked during BAML result adoption",
            ))
        })
    }

    #[napi(js_name = "_discard")]
    pub fn discard(&mut self) -> napi::Result<()> {
        let _drain = DrainHostReleases;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.discard_inner())).map_err(
            |_| napi::Error::from_reason("Resource cleanup panicked during BAML result discard"),
        )
    }
}

impl Drop for BamlEncodedResult {
    fn drop(&mut self) {
        let _drain = DrainHostReleases;
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.discard_inner())).is_err()
        {
            log::error!("resource cleanup panicked while discarding a Node BAML result");
        }
    }
}

/// Reclaim a JS-wrapped result if native argument conversion or dispatch fails.
/// Successfully adopted handles remain owned by the SDK even if a broken host
/// handler throws afterward. This guard never revokes adopted ownership.
pub(crate) struct DeliveryAbortGuard {
    state: Option<Arc<HandleTransferState>>,
    session: TransferSession<'static>,
    receipt: Option<bridge_ctypes::TransferReceipt>,
}
impl DeliveryAbortGuard {
    pub(crate) fn disarm(mut self) {
        self.state.take();
    }
}
impl Drop for DeliveryAbortGuard {
    fn drop(&mut self) {
        let _drain = DrainHostReleases;
        let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(state) = self.state.take() {
                if state.status.load(Ordering::Acquire) != TRANSFER_ADOPTED {
                    state.discard();
                    if let Some(receipt) = self.receipt.take() {
                        let _ = self.session.discard(receipt);
                    }
                }
            }
        }));
        if cleanup.is_err() {
            log::error!("resource cleanup panicked during failed Node callback delivery");
        }
    }
}
