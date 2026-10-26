//! The two reference tables a handle's key can name.
//!
//! A `HOST_VALUE_CALLABLE` or `HOST_VALUE_OPAQUE` key is the engine's
//! reference to a value this process registered, numbered by this bridge; any
//! other key is this process's reference to an engine object, numbered by
//! the engine. The spaces share numbers and are never compared: an engine
//! key is cloned and released through the engine's handle API, a host key
//! never is.

use crate::{capi, wire};

/// Which table a handle's key names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeySpace {
    /// The engine's reference to a value this process registered.
    Host,
    /// This process's reference to an engine object.
    Engine,
}

impl KeySpace {
    /// The table a key of `handle_type` names. The two host kinds are the
    /// only host-numbered keys; every other kind the engine defines,
    /// `HANDLE_UNSPECIFIED` and any kind this bridge does not know included,
    /// is a key the engine issued.
    pub(crate) fn of(handle_type: i32) -> Self {
        use wire::BamlHandleType as Kind;
        match Kind::try_from(handle_type) {
            Ok(Kind::HostValueCallable | Kind::HostValueOpaque) => Self::Host,
            Ok(
                Kind::HandleUnspecified
                | Kind::UntaggedRustData
                | Kind::UntaggedBexHeap
                | Kind::FunctionRef
                | Kind::AdtMediaImage
                | Kind::AdtMediaAudio
                | Kind::AdtMediaVideo
                | Kind::AdtMediaPdf
                | Kind::AdtMediaGeneric
                | Kind::AdtPromptAst
                | Kind::AdtType
                | Kind::AdtTaggedHeapHandle
                | Kind::AdtFunctionSpec
                | Kind::AdtRuntimeValue
                | Kind::InvocationState
                | Kind::TraceReservation,
            )
            | Err(_) => Self::Engine,
        }
    }
}

/// A fresh engine reference to the object engine key `key` names, or `None`
/// when the engine refuses (the key was released, or the runtime is gone).
pub(crate) fn clone_engine_key(key: u64) -> Option<u64> {
    // An engine key exists only once the engine is loaded.
    let api = capi::loaded_api()?;
    let mut cloned = 0;
    // SAFETY: `cloned` is writable for the call; the engine validates `key`.
    #[expect(unsafe_code)]
    let status = unsafe { (api.handle_clone)(key, &raw mut cloned) };
    (status == 0 && cloned != 0).then_some(cloned)
}

/// Release one engine reference. A key the engine no longer holds is
/// refused by the engine and changes nothing.
pub(crate) fn release_engine_key(key: u64) {
    if let Some(api) = capi::loaded_api() {
        // SAFETY: each engine key this process holds is one reference, which
        // the caller gives up here.
        #[expect(unsafe_code)]
        unsafe {
            (api.handle_release)(key);
        }
    }
}
