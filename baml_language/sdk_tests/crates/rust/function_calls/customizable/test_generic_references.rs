//! A generic class the runtime keeps to itself (`baml.sap._ParseCache<T>`):
//! an instance crosses as a reference at its type arguments, so its methods
//! bind `T` from the receiver, and two references to one engine instance are
//! equal.

use baml_bridge::baml_value::internal::__BamlValuePrivate;
use baml_sdk::baml::errors::ParseError;
use baml_sdk::baml::sap::{_ParseCache, _new_parse_cache};
use baml_sdk::generic_tests::identity;

#[test]
fn test_generic_references_method_binds_the_class_type_argument_from_its_receiver() {
    let ints = _new_parse_cache::<i64>().unwrap();
    assert_eq!(ints._parse_final("41".to_string()).unwrap(), 41);
    // The same text read at another type argument: the binding is the
    // receiver's, not a default.
    let strings = _new_parse_cache::<String>().unwrap();
    assert_eq!(strings._parse_final("41".to_string()).unwrap(), "41");
}

#[test]
fn test_generic_references_reference_carries_its_type_arguments() {
    // A string cache, decoded as an int cache: a reference decodes only at
    // the type arguments it came with.
    let _ = _new_parse_cache::<String>().unwrap();
    let as_ints = baml_bridge::runtime::invoke_sync::<_ParseCache<i64>, ParseError>(
        "baml.sap._new_parse_cache",
        |_| {
            Ok(baml_bridge::encode::type_args(vec![(
                "T",
                String::baml_ty(),
            )]))
        },
    );
    assert!(
        matches!(
            as_ints,
            Err(baml_bridge::Error::Decode(
                baml_bridge::DecodeError::TypeArgsMismatch { .. }
            ))
        ),
        "{as_ints:?}"
    );
}

#[test]
fn test_generic_references_one_engine_instance_is_one_reference() {
    let cache = _new_parse_cache::<i64>().unwrap();
    assert_eq!(identity(cache.clone()).unwrap(), cache);
    assert_ne!(_new_parse_cache::<i64>().unwrap(), cache);
}
