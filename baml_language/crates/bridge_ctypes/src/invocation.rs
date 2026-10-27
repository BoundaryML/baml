//! Effective callback frames shared by native and Wasm transports.

use std::sync::Arc;

use bex_project::{Bex, HostKey, InvocationCapture};

use crate::{
    CffiHandleTableEntry, CffiHandleTableOptions, CtypesError, HANDLE_TABLE, InvocationStateHandle,
    TableKey, WireHandle,
    baml_bridge::cffi::{BamlOutboundValue, HostInvocation, baml_outbound_value},
    external_to_outbound,
};

/// Releases each wire occurrence separately; identity-bearing keys can repeat.
/// A host value is the engine's reference, not the host's: nothing to release.
pub fn release_outbound_references(value: BamlOutboundValue) {
    use baml_outbound_value::Value;
    match value.value {
        Some(Value::HandleValue(handle)) => match WireHandle::try_from(&handle) {
            Ok(WireHandle::Engine(key)) => {
                HANDLE_TABLE.release(key);
            }
            // The host owns nothing for these; a key 0 names nothing.
            Ok(WireHandle::Host(..)) | Err(_) => {}
        },
        // A type value holds one row per runtime declaration it names, read
        // off the raw message so a tree the parse would refuse releases too.
        Some(Value::TyValue(ty)) => {
            crate::ty_decode::live_keys(&ty, &mut |key| {
                HANDLE_TABLE.release(key);
            });
        }
        Some(Value::ListValue(list)) => {
            for value in list.items {
                release_outbound_references(value);
            }
        }
        Some(Value::MapValue(map)) => {
            for entry in map.entries {
                if let Some(value) = entry.value {
                    release_outbound_references(value);
                }
            }
        }
        Some(Value::ClassValue(class)) => {
            for entry in class.fields {
                if let Some(value) = entry.value {
                    release_outbound_references(value);
                }
            }
        }
        Some(Value::UnionVariantValue(union)) => {
            if let Some(value) = union.value {
                release_outbound_references(*value);
            }
        }
        _ => {}
    }
}

/// The adapter carries these controls through actual host exit. Arguments
/// have their own decoder ownership; copying the frame requires `handle_clone`.
pub struct OwnedHostInvocation(pub HostInvocation);

impl OwnedHostInvocation {
    /// Transfer control references to an adapter that decoded its own frame.
    pub fn handoff(&mut self) {
        self.0.effective_state = 0;
        self.0.cancel = None;
    }
}

pub struct OwnedHostArguments(pub Option<crate::baml_bridge::cffi::BamlToHostCall>);
impl Drop for OwnedHostArguments {
    fn drop(&mut self) {
        if let Some(call) = self.0.take() {
            for arg in call.args {
                if let Some(value) = arg.value {
                    release_outbound_references(value);
                }
            }
        }
    }
}

impl Drop for OwnedHostInvocation {
    fn drop(&mut self) {
        // After `handoff` the frame names no state (0) and owns nothing.
        if let Some(state) = TableKey::new(self.0.effective_state) {
            HANDLE_TABLE.release(state);
        }
        if let Some(cancel) = self.0.cancel.take() {
            release_outbound_references(cancel);
        }
    }
}

pub fn build_host_invocation(
    capture: Arc<dyn bex_project::BexRustData>,
    host_value_key: HostKey,
    callback_id: u32,
    application_args: Vec<u8>,
) -> Result<HostInvocation, CtypesError> {
    use bex_project::RustDataArc as _;
    let capture = capture
        .downcast_payload::<InvocationCapture>()
        .map_err(|_| {
            CtypesError::InternalError("host dispatch has no engine invocation frame".into())
        })?;
    let cancel = capture.cancel.as_ref().ok_or_else(|| {
        CtypesError::InternalError("host dispatch has no effective CancelToken".into())
    })?;
    let cancel = external_to_outbound(cancel, &CffiHandleTableOptions::for_wire())?;
    let owner: Arc<dyn Bex> = capture.runtime.clone();
    let effective_state = HANDLE_TABLE.insert(CffiHandleTableEntry::InvocationState(
        InvocationStateHandle {
            owner,
            state: capture.state.clone(),
        },
    ));
    Ok(HostInvocation {
        host_value_key: host_value_key.get(),
        callback_id,
        application_args,
        effective_state: effective_state.get(),
        host_environment: capture.host_environment(),
        cancel: Some(cancel),
        deadline_ns: capture.deadline_ns(),
    })
}
