//! The panics a bridge completes a host call with: one for a callable that
//! broke the contract it was bound to, one for a failure of the bridge's own.

use baml_type::{RuntimeTy, TypeName};
use bridge_ctypes::{
    baml_bridge::cffi::{
        InboundClassValue, InboundMapEntry, InboundValue, inbound_map_entry::Key,
        inbound_value::Value,
    },
    runtime_ty_to_proto_ty,
};

/// A `baml.panics.HostContractViolation` as the wire carries it: what a
/// bridge completes a host call with when the callable's result does not fit
/// the return type it was bound to. The engine routes a thrown panic class as
/// a panic, never as a catchable error, which is what its own return check
/// reports for the same defect.
pub fn host_contract_violation(message: &str, language: &str) -> InboundValue {
    let class = TypeName::from_dotted_path("baml.panics.HostContractViolation");
    let field = |name: &str, value: InboundValue| InboundMapEntry {
        key: Some(Key::StringKey(name.to_string())),
        value: Some(value),
    };
    let string = |text: String| InboundValue {
        value_type: None,
        value: Some(Value::StringValue(text)),
    };
    InboundValue {
        value_type: Some(runtime_ty_to_proto_ty(&RuntimeTy::Class(
            class,
            Box::new([]),
        ))),
        value: Some(Value::ClassValue(InboundClassValue {
            fields: vec![
                field(
                    "message",
                    string(format!(
                        "host callable returned a value of the wrong type: {message}"
                    )),
                ),
                field("class_name", InboundValue::default()),
                field("language", string(language.to_string())),
            ],
        })),
    }
}

/// A `baml.panics.SdkPanic` as the wire carries it: what a bridge completes a
/// host call with when it could not encode a result the callable produced in
/// good faith (a value too deep for the wire, a declaration the facts do not
/// reach). Neither the callable's contract nor the program is at fault.
pub fn sdk_panic(message: &str) -> InboundValue {
    let class = TypeName::from_dotted_path("baml.panics.SdkPanic");
    InboundValue {
        value_type: Some(runtime_ty_to_proto_ty(&RuntimeTy::Class(
            class,
            Box::new([]),
        ))),
        value: Some(Value::ClassValue(InboundClassValue {
            fields: vec![InboundMapEntry {
                key: Some(Key::StringKey("message".to_string())),
                value: Some(InboundValue {
                    value_type: None,
                    value: Some(Value::StringValue(format!(
                        "the bridge could not encode the host callable's result: {message}"
                    ))),
                }),
            }],
        })),
    }
}

#[cfg(test)]
mod tests {
    use bridge_ctypes::baml_bridge::cffi::baml_ty::Ty as WireTy;

    use super::*;

    #[test]
    fn states_the_panic_class_with_its_three_fields() {
        let value = host_contract_violation("result: expected int", "python");
        assert!(matches!(
            value.value_type.as_ref().and_then(|ty| ty.ty.as_ref()),
            Some(WireTy::ClassTy(class)) if class.name == "baml.panics.HostContractViolation"
        ));
        let Some(Value::ClassValue(InboundClassValue { fields })) = &value.value else {
            panic!("not a class value");
        };
        let names: Vec<_> = fields
            .iter()
            .map(|field| match &field.key {
                Some(Key::StringKey(name)) => name.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(names, ["message", "class_name", "language"]);
    }
}
