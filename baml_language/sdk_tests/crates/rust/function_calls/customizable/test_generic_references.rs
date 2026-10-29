//! A generic class the runtime keeps to itself (`baml.sap._ParseCache<T>`):
//! an instance crosses as a reference at its type arguments, so its methods
//! bind `T` from the receiver, and two references to one engine instance are
//! equal.

use baml_bridge::baml_value::internal::__BamlValuePrivate;
use baml_sdk::baml::errors::ParseError;
use baml_sdk::baml::sap::{_ParseCache, _new_parse_cache, _new_parse_cache_sync};
use baml_sdk::generic_tests::identity_sync;

#[tokio::test]
async fn test_generic_references_method_binds_the_class_type_argument_from_its_receiver() {
    let ints = _new_parse_cache::<i64>().await.unwrap();
    assert_eq!(ints._parse_final("41".to_string()).await.unwrap(), 41);
    // The same text read at another type argument: the binding is the
    // receiver's, not a default.
    let strings = _new_parse_cache::<String>().await.unwrap();
    assert_eq!(strings._parse_final("41".to_string()).await.unwrap(), "41");
}

#[test]
fn test_generic_references_reference_carries_its_type_arguments() {
    // A cache built at a type argument stated by name binds its methods to
    // that argument.
    let _ = _new_parse_cache_sync::<String>().unwrap(); // loads the program
    let strings = baml_bridge::runtime::invoke_sync::<_ParseCache<String>, ParseError>(
        "baml.sap._new_parse_cache",
        |_| {
            Ok(baml_bridge::encode::type_args(vec![(
                "T",
                String::baml_ty(),
            )]))
        },
    )
    .unwrap();
    assert_eq!(strings._parse_final_sync("41".to_string()).unwrap(), "41");
}

#[test]
fn test_generic_references_one_engine_instance_is_one_reference() {
    let cache = _new_parse_cache_sync::<i64>().unwrap();
    assert_eq!(identity_sync(cache.clone()).unwrap(), cache);
    assert_ne!(_new_parse_cache_sync::<i64>().unwrap(), cache);
}
