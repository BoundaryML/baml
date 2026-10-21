//! Private lowering used by generated, strongly typed invocation facades.
use std::sync::Arc;

use prost::Message;

use crate::{RustType, SdkError, capi, rust_type::OwnedHandle, wire};

#[doc(hidden)]
#[derive(Clone, Default)]
pub struct InvocationOptions {
    trace: Option<RustType>,
    cancel: Option<Arc<dyn Fn() -> wire::InboundValue + Send + Sync>>,
    timeout_ms: Option<u32>,
}
impl InvocationOptions {
    #[doc(hidden)]
    pub fn new(
        trace: Option<RustType>,
        cancel: Option<Arc<dyn Fn() -> wire::InboundValue + Send + Sync>>,
        timeout_ms: Option<u32>,
    ) -> Self {
        Self {
            trace,
            cancel,
            timeout_ms,
        }
    }
    pub(crate) fn prepare(self, call_id: u64) -> Result<PreparedInvocation, SdkError> {
        let api = capi::api()?;
        let mut wire = wire::InvocationOptions {
            inherited_state: crate::host_value::current_invocation_state(),
            host_environment: call_id,
            ..Default::default()
        };
        if let Some(timeout) = self.timeout_ms {
            if timeout > 2_147_483_647 {
                return Err(SdkError::new("timeout_ms exceeds 2147483647"));
            }
            let mut now = 0;
            // SAFETY: allocated call and writable output on its original runtime.
            #[expect(unsafe_code)]
            let status = unsafe { (api.invocation_clock_ns)(call_id, &raw mut now) };
            if status != 0 {
                return Err(SdkError::new("invocation clock unavailable"));
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
            // SAFETY: the trace owns a live capability; outputs are writable.
            #[expect(unsafe_code)]
            let status = unsafe {
                (api.trace_selection)(
                    call_id,
                    trace.invocation_key(),
                    &raw mut buffer,
                    &raw mut owner,
                )
            };
            if status != 0 {
                return Err(SdkError::new("invalid trace selection"));
            }
            reservation = (owner != 0).then(|| OwnedHandle(owner));
            let bytes = api.copy_and_free(buffer);
            wire.trace = Some(
                wire::TraceSelection::decode(bytes.as_slice())
                    .map_err(|error| SdkError::new(error.to_string()))?,
            );
        }
        if let Some(cancel) = &self.cancel {
            wire.cancel = Some(cancel());
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

impl Drop for PreparedInvocation {
    fn drop(&mut self) {
        if let (Some(cancel), Ok(api)) = (self.wire.cancel.take(), capi::api()) {
            release_inbound(api, cancel);
        }
    }
}
pub(crate) fn release_inbound(api: &capi::Api, value: wire::InboundValue) {
    use wire::inbound_value::Value;
    match value.value {
        Some(Value::Handle(handle)) if handle.handle_type != 15 && handle.handle_type != 16 => {
            // Preparation consumes successful clones synchronously. Releasing
            // their old keys is harmless; failures leave remaining keys owned here.
            #[expect(unsafe_code)]
            unsafe {
                (api.handle_release)(handle.key);
            }
        }
        Some(Value::ListValue(list)) => {
            for item in list.values {
                release_inbound(api, item);
            }
        }
        Some(Value::MapValue(map)) => {
            for entry in map.entries {
                if let Some(item) = entry.value {
                    release_inbound(api, item);
                }
            }
        }
        Some(Value::ClassValue(class)) => {
            for entry in class.fields {
                if let Some(item) = entry.value {
                    release_inbound(api, item);
                }
            }
        }
        _ => {}
    }
}
