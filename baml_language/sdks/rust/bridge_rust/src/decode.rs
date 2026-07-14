//! Outbound (engine → host) result decoding.
//!
//! [`decode_result`] interprets the `BamlOutboundResult` envelope that
//! `bridge_cffi::call_and_encode` returns; [`unwrap`] peels the value
//! envelopes (`union_variant`, `literal`) every `from_baml` impl must
//! tolerate.

use prost::Message as _;

use crate::{
    BamlValue, Error, SdkError,
    wire::{
        self, BamlLiteralValue, baml_literal_value::Literal, baml_outbound_value::Value as Out,
    },
};

/// Peel value envelopes down to a bare value.
///
/// The engine wraps a value produced through a union-typed (including
/// optional-typed) slot in a `union_variant` envelope, and literal-typed
/// values ride as `literal` envelopes; the host decodes by *static* type,
/// so both reduce to the inner value. Idempotent on bare values.
pub fn unwrap(v: wire::BamlOutboundValue) -> wire::BamlOutboundValue {
    match v.value {
        Some(Out::UnionVariantValue(variant)) => match variant.value {
            Some(inner) => unwrap(*inner),
            None => wire::BamlOutboundValue { value: None },
        },
        Some(Out::LiteralValue(lit)) => literal_to_plain(lit),
        _ => v,
    }
}

fn literal_to_plain(lit: BamlLiteralValue) -> wire::BamlOutboundValue {
    let value = lit.literal.map(|l| match l {
        Literal::StringValue(s) => Out::StringValue(s),
        Literal::IntValue(i) => Out::IntValue(i),
        Literal::BoolValue(b) => Out::BoolValue(b),
        Literal::BigintValue(s) => Out::BigintValue(s),
        // Float literals ride the wire as their decimal source text.
        Literal::FloatValue(s) => match s.parse::<f64>() {
            Ok(f) => Out::FloatValue(f),
            Err(_) => Out::StringValue(s),
        },
    });
    wire::BamlOutboundValue { value }
}

/// Decode a `BamlOutboundResult` envelope into the function's typed
/// result.
///
/// - `ok` decodes as `R` (the declared return type).
/// - `error` first tries the declared throws type `E` — success is
///   [`Error::Thrown`]; anything else (engine-infrastructure errors,
///   contract drift) lands in [`Error::Runtime`] with class name, message,
///   and trace preserved.
/// - `panic` becomes [`Error::Panic`] — except exit panics, which
///   terminate the process (parity with the other SDKs' hard exit).
pub fn decode_result<R: BamlValue, E: BamlValue>(bytes: &[u8]) -> Result<R, Error<E>> {
    let envelope = wire::BamlOutboundResult::decode(bytes)
        .map_err(|e| Error::Sdk(SdkError::new(format!("malformed result envelope: {e}"))))?;
    match envelope.result {
        Some(wire::baml_outbound_result::Result::Ok(value)) => {
            R::from_baml(value).map_err(Error::Decode)
        }
        Some(wire::baml_outbound_result::Result::Error(err)) => {
            let value = err.value.unwrap_or_default();
            match E::from_baml(value.clone()) {
                Ok(thrown) => Err(Error::Thrown {
                    value: thrown,
                    trace: err.trace,
                }),
                Err(_) => Err(Error::Runtime {
                    class_name: class_fqn(&value),
                    message: render_message(&value),
                    trace: err.trace,
                }),
            }
        }
        Some(wire::baml_outbound_result::Result::Panic(panic)) => {
            if panic.is_exit_panic {
                // A `baml.sys.exit` panic is a process exit, not a
                // catchable error — parity with the other SDKs. An exit
                // code outside the OS's i32 range falls back to the
                // generic failure code.
                #[expect(clippy::exit, reason = "exit panics terminate the process by contract")]
                std::process::exit(i32::try_from(panic.exit_code).unwrap_or(1));
            }
            let value = panic.value.unwrap_or_default();
            Err(Error::Panic {
                message: render_message(&value),
                trace: panic.trace,
            })
        }
        None => Err(Error::Sdk(SdkError::new("empty result envelope"))),
    }
}

/// Field accessor for decoding a class value, used by generated
/// `from_baml` impls.
///
/// Construction verifies the wire FQN against the expected class: with no
/// runtime typemap, decode is driven by the declared static type, so a
/// mismatch here is always engine/codegen drift and must fail loudly —
/// never coerce by position.
pub struct ClassFields {
    class: &'static str,
    fields: std::collections::HashMap<String, wire::BamlOutboundValue>,
}

impl ClassFields {
    pub fn new(
        v: wire::BamlOutboundValue,
        expected_fqn: &'static str,
    ) -> Result<Self, crate::DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::ClassValue(class)) => {
                if class.name != expected_fqn {
                    return Err(crate::DecodeError::FqnMismatch {
                        expected: expected_fqn,
                        got: class.name,
                    });
                }
                Ok(Self {
                    class: expected_fqn,
                    fields: class
                        .fields
                        .into_iter()
                        .map(|entry| (entry.key, entry.value.unwrap_or_default()))
                        .collect(),
                })
            }
            _ => Err(crate::DecodeError::WrongType {
                expected: expected_fqn,
                got: crate::baml_value::wire_variant_kind(&v),
            }),
        }
    }

    /// Decode and remove the named field. An absent field decodes as null
    /// (so optional fields tolerate omission); a required field then
    /// surfaces the absence as [`MissingField`] rather than a null-type
    /// mismatch.
    ///
    /// [`MissingField`]: crate::DecodeError::MissingField
    pub fn take<T: BamlValue>(&mut self, field: &'static str) -> Result<T, crate::DecodeError> {
        match self.fields.remove(field) {
            Some(value) => T::from_baml(value),
            None => T::from_baml(wire::BamlOutboundValue { value: None }).map_err(|_| {
                crate::DecodeError::MissingField {
                    class: self.class,
                    field,
                }
            }),
        }
    }
}

/// Decode an enum value's wire variant string, verifying the enum FQN.
/// Generated impls match the returned value against their variants'
/// declared wire values.
pub fn enum_variant(
    v: wire::BamlOutboundValue,
    expected_fqn: &'static str,
) -> Result<String, crate::DecodeError> {
    let v = unwrap(v);
    match v.value {
        Some(Out::EnumValue(e)) => {
            if e.name != expected_fqn {
                return Err(crate::DecodeError::FqnMismatch {
                    expected: expected_fqn,
                    got: e.name,
                });
            }
            Ok(e.value)
        }
        _ => Err(crate::DecodeError::WrongType {
            expected: expected_fqn,
            got: crate::baml_value::wire_variant_kind(&v),
        }),
    }
}

/// The class FQN of an error-arm value, when it is a class instance.
fn class_fqn(v: &wire::BamlOutboundValue) -> Option<String> {
    match &v.value {
        Some(Out::ClassValue(class)) => Some(class.name.clone()),
        _ => None,
    }
}

/// Bounded, best-effort message for an error-arm value: a class's
/// `message` field, a bare string payload, or the value's variant kind —
/// never a full value dump.
fn render_message(v: &wire::BamlOutboundValue) -> String {
    match &v.value {
        Some(Out::ClassValue(class)) => class
            .fields
            .iter()
            .find(|f| f.key == "message")
            .and_then(|f| f.value.clone())
            .map(unwrap)
            .and_then(|value| match value.value {
                Some(Out::StringValue(s)) => Some(s),
                _ => None,
            })
            .unwrap_or_else(|| format!("<{}>", class.name)),
        Some(Out::StringValue(s)) => s.clone(),
        Some(_) => "<non-class error value>".to_string(),
        None => "<null error value>".to_string(),
    }
}
