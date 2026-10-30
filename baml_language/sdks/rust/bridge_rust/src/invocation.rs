//! Private lowering used by generated, strongly typed invocation facades.
use std::sync::Arc;

use prost::Message;

use crate::{
    SdkError,
    baml_value::internal::RuntimeInstance,
    capi,
    encode::{ArgumentError, Encoder},
    rust_type::OwnedHandle,
    wire,
};

/// Encode a call's cancel token, which rides the request beside its
/// arguments.
#[doc(hidden)]
pub type CancelEncoder = Arc<crate::encode::EncodeValue>;

#[doc(hidden)]
#[derive(Clone, Default)]
pub struct InvocationOptions {
    /// The `trace.Options` or `trace.ReservedSpan` instance the call is
    /// traced with.
    trace: Option<RuntimeInstance>,
    cancel: Option<CancelEncoder>,
    timeout_ms: Option<u32>,
}
impl InvocationOptions {
    #[doc(hidden)]
    pub fn new(
        trace: Option<RuntimeInstance>,
        cancel: Option<CancelEncoder>,
        timeout_ms: Option<u32>,
    ) -> Self {
        Self {
            trace,
            cancel,
            timeout_ms,
        }
    }
    /// The request's invocation options; the cancel token is encoded with
    /// `encoder`, which owns its handle until the call is sent.
    pub(crate) fn prepare<E>(
        self,
        call_id: u64,
        encoder: &mut Encoder,
    ) -> Result<PreparedInvocation, crate::Error<E>> {
        let api = capi::api()?;
        let mut wire = wire::InvocationOptions {
            inherited_state: crate::host_value::current_invocation_state(),
            host_environment: call_id,
            ..Default::default()
        };
        if let Some(timeout) = self.timeout_ms {
            if timeout > 2_147_483_647 {
                return Err(SdkError::new("timeout_ms exceeds 2147483647").into());
            }
            let mut now = 0;
            // SAFETY: allocated call and writable output on its original runtime.
            #[expect(unsafe_code)]
            let status = unsafe { (api.invocation_clock_ns)(call_id, &raw mut now) };
            if status != 0 {
                return Err(SdkError::new("invocation clock unavailable").into());
            }
            wire.deadline_ns = Some(
                now.checked_add(u64::from(timeout) * 1_000_000)
                    .ok_or_else(|| SdkError::new("invocation deadline overflow"))?,
            );
        }
        let mut reservation = None;
        if let Some(trace) = &self.trace {
            let mut buffer = capi::Buffer {
                ptr: std::ptr::null(),
                len: 0,
            };
            let mut owner = 0;
            // SAFETY: the trace instance owns a live handle; outputs are writable.
            #[expect(unsafe_code)]
            let status = unsafe {
                (api.trace_selection)(call_id, trace.key(), &raw mut buffer, &raw mut owner)
            };
            if status != 0 {
                return Err(SdkError::new("invalid trace selection").into());
            }
            reservation = (owner != 0).then(|| OwnedHandle(owner));
            let bytes = api.copy_and_free(buffer);
            wire.trace = Some(
                wire::TraceSelection::decode(bytes.as_slice())
                    .map_err(|error| SdkError::new(error.to_string()))?,
            );
        }
        if let Some(cancel) = &self.cancel {
            let token = cancel(encoder)
                .and_then(|token| {
                    crate::encode::fits_the_wire(token, crate::encode::ARGUMENT_DEPTH)
                })
                .map_err(|error| ArgumentError {
                    name: "cancel".to_string(),
                    error,
                })?;
            wire.cancel = Some(token);
        }
        Ok(PreparedInvocation {
            wire,
            _reservation: reservation,
        })
    }
}
pub(crate) struct PreparedInvocation {
    pub(crate) wire: wire::InvocationOptions,
    _reservation: Option<OwnedHandle>,
}

#[doc(hidden)]
pub fn current_context<T: crate::BamlValue>() -> Result<T, crate::Error<std::convert::Infallible>> {
    let api = capi::api().map_err(crate::Error::Sdk)?;
    let mut output = capi::Buffer {
        ptr: std::ptr::null(),
        len: 0,
    };
    // SAFETY: the current scope owns the state and output storage is writable.
    #[expect(unsafe_code)]
    let status = unsafe {
        (api.invocation_context)(
            crate::host_value::current_invocation_state(),
            &raw mut output,
        )
    };
    if status != 0 {
        return Err(crate::Error::Sdk(SdkError::new(
            "cannot inspect invocation context",
        )));
    }
    let bytes = api.copy_and_free(output);
    let value = wire::BamlOutboundValue::decode(bytes.as_slice())
        .map_err(|error| crate::Error::Sdk(SdkError::new(error.to_string())))?;
    T::from_baml(value).map_err(crate::Error::Decode)
}
