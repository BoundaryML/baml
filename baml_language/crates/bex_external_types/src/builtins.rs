//! Typesafe constructors for builtin instance types.
//!
//! These constructors create `BexExternalValue::Instance` values for builtin
//! types like `Response`, `File`, and `Socket`. The field order matches the
//! order defined in `baml_builtins`.

use std::collections::HashMap;

use bex_resource_types::ResourceHandle;
use indexmap::indexmap;

use crate::{BexExternalValue, Ty};

/// Create a new HTTP Response instance.
///
/// Field order:
/// 0. _handle (private)
/// 1. status_code
/// 2. headers
/// 3. url
pub fn new_http_response(
    handle: ResourceHandle,
    status_code: u16,
    headers: HashMap<String, String>,
    url: String,
) -> BexExternalValue {
    BexExternalValue::Instance {
        class_name: "baml.http.Response".to_string(),
        fields: indexmap! {
            "_handle".to_string() => BexExternalValue::Resource(handle),
            "status_code".to_string() => BexExternalValue::Int(i64::from(status_code)),
            "headers".to_string() => BexExternalValue::Map {
                key_type: Ty::String { attr: baml_type::TyAttr::default() },
                value_type: Ty::String { attr: baml_type::TyAttr::default() },
                entries: headers.into_iter().map(|(k, v)| (k, BexExternalValue::String(v))).collect(),
            },
            "url".to_string() => BexExternalValue::String(url),
        },
    }
}

/// Create a new File instance.
///
/// Field order:
/// 0. _handle (private)
pub fn new_file(handle: ResourceHandle) -> BexExternalValue {
    BexExternalValue::Instance {
        class_name: "baml.fs.File".to_string(),
        fields: indexmap! {
            "_handle".to_string() => BexExternalValue::Resource(handle),
        },
    }
}

/// Create a new HTTP Request instance for a GET request.
///
/// Field order:
/// 0. method
/// 1. url
/// 2. headers (empty)
/// 3. body (empty)
pub fn new_http_request_get(url: String) -> BexExternalValue {
    BexExternalValue::Instance {
        class_name: "baml.http.Request".to_string(),
        fields: indexmap! {
            "method".to_string() => BexExternalValue::String("GET".to_string()),
            "url".to_string() => BexExternalValue::String(url),
            "headers".to_string() => BexExternalValue::Map {
                key_type: Ty::String { attr: baml_type::TyAttr::default() },
                value_type: Ty::String { attr: baml_type::TyAttr::default() },
                entries: indexmap::IndexMap::new(),
            },
            "body".to_string() => BexExternalValue::String(String::new()),
        },
    }
}

/// Create a new Socket instance.
///
/// Field order:
/// 0. _handle (private)
pub fn new_socket(handle: ResourceHandle) -> BexExternalValue {
    BexExternalValue::Instance {
        class_name: "baml.net.Socket".to_string(),
        fields: indexmap! {
            "_handle".to_string() => BexExternalValue::Resource(handle),
        },
    }
}
