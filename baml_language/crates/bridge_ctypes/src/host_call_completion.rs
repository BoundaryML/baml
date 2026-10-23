//! Decode what a host completed a BAML→host callback with.
//!
//! The wire's `HostCallCompletion` is the one completion channel. Its arms are
//! decoded here into a [`HostCallCompletion`]; every fault the decoder itself
//! finds — bytes that are not the message, a payload with no outcome, a value
//! the inbound decoder refuses, a host exception with no handle — is the
//! bridge's, never the callable's, and lands as a bridge failure.

use bex_project::{
    HostCallCompletion, HostException, HostKey, HostOutcome, HostValueArc, HostValueKind,
};
use prost::Message;

use crate::{
    InboundOwners,
    baml_bridge::cffi::{
        HostCallCompletion as WireCompletion, HostException as WireException,
        host_call_completion::Outcome,
    },
    handle_table::CffiHandleTable,
};

/// Decode a `HostCallCompletion` payload. Every transfer the payload carries
/// is journaled and released if the completion is refused.
pub fn decode_host_call_completion(
    bytes: &[u8],
    handle_table: &CffiHandleTable,
) -> HostCallCompletion {
    let completion = match WireCompletion::decode(bytes) {
        Ok(completion) => completion,
        Err(error) => {
            // Through `CtypesError`, whose rendering bounds prost's field path.
            return HostCallCompletion::BridgeFailure(format!(
                "the host's completion payload is not a HostCallCompletion: {}",
                crate::CtypesError::from(error)
            ));
        }
    };
    let Some(outcome) = completion.outcome else {
        return HostCallCompletion::BridgeFailure(
            "the host's completion payload carries no outcome".to_string(),
        );
    };
    let mut owners = InboundOwners::new(handle_table);
    let value = |owners: &mut InboundOwners<'_>, value, what: &str| {
        owners
            .collect(&value)
            .and_then(|()| owners.decode(value))
            .map_err(|error| {
                HostCallCompletion::BridgeFailure(format!(
                    "the host's {what} could not be decoded: {error}"
                ))
            })
    };
    match outcome {
        Outcome::Returned(returned) => match value(&mut owners, returned, "returned value") {
            Ok(value) => HostCallCompletion::Returned(Box::new(value)),
            Err(failure) => failure,
        },
        Outcome::Thrown(thrown) => match value(&mut owners, thrown, "thrown value") {
            Ok(value) => HostCallCompletion::Host(HostOutcome::Thrown(Box::new(value))),
            Err(failure) => failure,
        },
        Outcome::Raised(raised) => match host_exception(raised) {
            Ok(exception) => HostCallCompletion::Host(HostOutcome::Raised(Box::new(exception))),
            Err(failure) => failure,
        },
        Outcome::ContractViolation(message) => HostCallCompletion::ContractViolation(message),
        Outcome::BridgeFailure(message) => HostCallCompletion::BridgeFailure(message),
    }
}

/// A host's exception as the engine will build `baml.errors.HostCallable`
/// from it: its registration is the engine's one reference to the object.
fn host_exception(raised: WireException) -> Result<HostException, HostCallCompletion> {
    let key = HostKey::new(raised.handle).ok_or_else(|| {
        HostCallCompletion::BridgeFailure(
            "the host raised an exception without registering it (handle 0)".to_string(),
        )
    })?;
    let handle = HostValueArc::try_intern(key, HostValueKind::Opaque).map_err(|live| {
        HostCallCompletion::BridgeFailure(format!(
            "the host raised an exception under host key {key}, which is live as a {:?} value",
            live.kind
        ))
    })?;
    Ok(HostException {
        message: raised.message,
        class_name: raised.class_name,
        language: raised.language,
        traceback: raised.traceback,
        handle,
    })
}

#[cfg(test)]
mod tests {
    use bex_project::BexExternalValue;

    use super::*;
    use crate::baml_bridge::cffi::{InboundListValue, InboundValue, inbound_value::Value};

    fn int(n: i64) -> InboundValue {
        InboundValue {
            value_type: None,
            value: Some(Value::IntValue(n)),
        }
    }

    #[test]
    fn every_arm_decodes_to_its_outcome() {
        let table = CffiHandleTable::new();
        let returned = WireCompletion {
            outcome: Some(Outcome::Returned(int(1))),
        };
        assert_eq!(
            decode_host_call_completion(&returned.encode_to_vec(), &table),
            HostCallCompletion::Returned(Box::new(BexExternalValue::Int(1)))
        );
        let thrown = WireCompletion {
            outcome: Some(Outcome::Thrown(int(2))),
        };
        assert_eq!(
            decode_host_call_completion(&thrown.encode_to_vec(), &table),
            HostCallCompletion::Host(HostOutcome::Thrown(Box::new(BexExternalValue::Int(2))))
        );
        let raised = WireCompletion {
            outcome: Some(Outcome::Raised(WireException {
                message: "boom".into(),
                class_name: "ValueError".into(),
                language: "python".into(),
                traceback: None,
                handle: 8_001,
            })),
        };
        let HostCallCompletion::Host(HostOutcome::Raised(exception)) =
            decode_host_call_completion(&raised.encode_to_vec(), &table)
        else {
            panic!("a raised exception decodes to Raised")
        };
        assert_eq!(exception.class_name, "ValueError");
        assert_eq!(exception.handle.key.get(), 8_001);
        assert_eq!(exception.handle.kind, HostValueKind::Opaque);
        let violation = WireCompletion {
            outcome: Some(Outcome::ContractViolation("wrong type".into())),
        };
        assert_eq!(
            decode_host_call_completion(&violation.encode_to_vec(), &table),
            HostCallCompletion::ContractViolation("wrong type".into())
        );
        let failure = WireCompletion {
            outcome: Some(Outcome::BridgeFailure("too deep".into())),
        };
        assert_eq!(
            decode_host_call_completion(&failure.encode_to_vec(), &table),
            HostCallCompletion::BridgeFailure("too deep".into())
        );
    }

    #[test]
    fn a_payload_the_bridge_got_wrong_is_the_bridges_failure() {
        let table = CffiHandleTable::new();
        assert!(matches!(
            decode_host_call_completion(&[], &table),
            HostCallCompletion::BridgeFailure(message) if message.contains("no outcome")
        ));
        assert!(matches!(
            decode_host_call_completion(&[0xff], &table),
            HostCallCompletion::BridgeFailure(message) if message.contains("not a HostCallCompletion")
        ));
        let unregistered = WireCompletion {
            outcome: Some(Outcome::Raised(WireException {
                handle: 0,
                ..Default::default()
            })),
        };
        assert!(matches!(
            decode_host_call_completion(&unregistered.encode_to_vec(), &table),
            HostCallCompletion::BridgeFailure(message) if message.contains("handle 0")
        ));
        // A thrown value the inbound decoder refuses (a list with no statement).
        let unstated = WireCompletion {
            outcome: Some(Outcome::Thrown(InboundValue {
                value_type: None,
                value: Some(Value::ListValue(InboundListValue::default())),
            })),
        };
        assert!(matches!(
            decode_host_call_completion(&unstated.encode_to_vec(), &table),
            HostCallCompletion::BridgeFailure(message) if message.contains("thrown value")
        ));
    }
}
