use std::{fmt::Write as _, path::PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use bex_project::BexExternalValue;
use bridge_ctypes::{CffiHandleTableOptions, build_to_host_call};
use indexmap::IndexMap;
use prost::Message as _;

fn main() {
    let path = std::env::var_os("BAML_CSHARP_OPTIONAL_HOST_CALL_VECTORS")
        .map(PathBuf::from)
        .expect("BAML_CSHARP_OPTIONAL_HOST_CALL_VECTORS must be set");
    assert!(path.is_absolute(), "vector output path must be absolute");

    let options = CffiHandleTableOptions::for_in_process();
    let required = [BexExternalValue::Int(7)];
    let mut cases = Vec::new();
    cases.push(("7:unset:unset", IndexMap::new()));

    let mut optional = IndexMap::new();
    optional.insert(
        "first".to_string(),
        BexExternalValue::String("alpha".into()),
    );
    cases.push(("7:alpha:unset", optional));

    let mut optional = IndexMap::new();
    optional.insert("later".to_string(), BexExternalValue::Int(9));
    cases.push(("7:unset:9", optional));

    let mut optional = IndexMap::new();
    optional.insert("first".to_string(), BexExternalValue::Null);
    cases.push(("7:null:unset", optional));

    let mut optional = IndexMap::new();
    optional.insert(
        "first".to_string(),
        BexExternalValue::String("alpha".into()),
    );
    optional.insert("later".to_string(), BexExternalValue::Int(9));
    cases.push(("7:alpha:9", optional));

    let mut output = String::new();
    for (expected, optional) in cases {
        let call = build_to_host_call(&required, &optional, &options)
            .expect("encode optional host-call vector");
        writeln!(
            output,
            "{expected}\t{}",
            STANDARD.encode(call.encode_to_vec())
        )
        .expect("write vector line");
    }
    std::fs::write(&path, output).expect("write optional host-call vectors");
    assert!(
        std::fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 0),
        "optional host-call vector output is empty"
    );
}
