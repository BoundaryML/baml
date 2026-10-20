//! What a host completes a BAML→host callback with, once decoded from the wire.
//!
//! A callback has one completion channel (`complete_host_call`) and five
//! outcomes. Two of them the engine must judge against the callable's declared
//! throws type and are carried as a [`HostOutcome`]; the other three are the
//! callable's result, a contract the host already found broken, and a failure
//! of the bridge's own, which need no host-specific judgment. Only a decoder of
//! that channel builds these: no engine code may put a value on the host path.

use std::sync::Arc;

use bex_resource_types::HostValueArc;

use crate::BexExternalValue;

/// An exception of the host's own that a callback raised. The engine builds
/// the `baml.errors.HostCallable` for it, with `handle` as the `_handle` the
/// same host can recover the exception object through.
#[derive(Debug, Clone, PartialEq)]
pub struct HostException {
    pub message: String,
    pub class_name: String,
    pub language: String,
    pub traceback: Option<String>,
    /// The host's registration of the exception, interned once by the decoder.
    pub handle: Arc<HostValueArc>,
}

/// A callback outcome the engine judges against the callable's declared
/// throws type: it either inhabits that type (or is a panic) and is thrown, or
/// the callable broke its contract.
#[derive(Debug, Clone, PartialEq)]
pub enum HostOutcome {
    /// A BAML value the callback threw, encoded by the host against the
    /// declared throws type. Boxed: the value is large and the success path
    /// never carries one.
    Thrown(Box<BexExternalValue>),
    /// An exception of the host's own.
    Raised(Box<HostException>),
}

/// A decoded `HostCallCompletion`: every arm of the wire's oneof, plus the
/// faults the decoder itself found, which are the bridge's.
#[derive(Debug, Clone, PartialEq)]
pub enum HostCallCompletion {
    /// The callable returned this value. Boxed like the thrown value: a
    /// completion is one per call, and the other arms are a string or two.
    Returned(Box<BexExternalValue>),
    /// The callable threw a BAML value or raised a host exception.
    Host(HostOutcome),
    /// The host could not encode what the callable produced against the
    /// declared type: a `baml.panics.HostContractViolation`.
    ContractViolation(String),
    /// The bridge failed in its own machinery, or sent a payload the decoder
    /// could not accept: a fatal bridge failure, `baml.panics.SdkPanic` to
    /// the host.
    BridgeFailure(String),
}
