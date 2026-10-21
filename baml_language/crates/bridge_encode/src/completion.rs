//! The one mapping from what a host's callable did to the wire's
//! `HostCallCompletion`, shared by every bridge that uses the walker.

use bridge_ctypes::{
    HostKey,
    baml_bridge::cffi::{HostCallCompletion, HostException, host_call_completion::Outcome},
};

use crate::{BridgeFault, Completion};

/// Which declared type a value was encoded against: the callable's return
/// type, or its throws type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoded {
    Returned,
    Thrown,
}

impl<E> Completion<E> {
    /// The completion the engine receives for an encode against `encoded`'s
    /// declared type. A value that fits is that outcome; a value that does not
    /// is the callable's broken contract; a value the bridge could not carry is
    /// the bridge's failure. The host's own exception raised while the value
    /// was read is returned to the caller, which completes the call with it as
    /// with any exception the callable raises.
    pub fn into_wire(self, encoded: Encoded) -> Result<HostCallCompletion, E> {
        let outcome = match self {
            Completion::Wire(value) => match encoded {
                Encoded::Returned => Outcome::Returned(value),
                Encoded::Thrown => Outcome::Thrown(value),
            },
            Completion::HostError(error) => return Err(error),
            Completion::ContractViolation(error) => Outcome::ContractViolation(format!(
                "the callable's {} does not inhabit its declared type: {error}",
                match encoded {
                    Encoded::Returned => "return value",
                    Encoded::Thrown => "thrown value",
                }
            )),
            Completion::BridgeFailure(fault) => Outcome::BridgeFailure(match fault {
                BridgeFault::Value(error) => error.to_string(),
                BridgeFault::Host(_) => {
                    // The host's own exception while a decided plan was
                    // committed (a handle that could not be cloned): the
                    // bridge, not the callable, failed to carry the value.
                    "the bridge could not carry the callable's value".to_string()
                }
            }),
        };
        Ok(HostCallCompletion {
            outcome: Some(outcome),
        })
    }
}

/// The completion for an exception of the host's own: the engine builds the
/// `baml.errors.HostCallable` for it, `handle` being the host's registration
/// of the exception object.
pub fn raised(
    message: String,
    class_name: String,
    language: &str,
    traceback: Option<String>,
    handle: HostKey,
) -> HostCallCompletion {
    HostCallCompletion {
        outcome: Some(Outcome::Raised(HostException {
            message,
            class_name,
            language: language.to_string(),
            traceback,
            handle: handle.get(),
        })),
    }
}

/// The completion for a failure of the bridge's own.
pub fn bridge_failure(message: String) -> HostCallCompletion {
    HostCallCompletion {
        outcome: Some(Outcome::BridgeFailure(message)),
    }
}
