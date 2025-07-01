use baml_types::{
    type_meta::base::{StreamingBehavior, TypeMeta},
    TypeIR,
};
use internal_baml_core::ir::repr::make_test_ir;

use crate::helpers::render_output_format;
#[cfg(test)]
use crate::{from_str, helpers::parsed_value_to_response};

#[test]
pub fn make_test_data1() {
    let ir = make_test_ir(
        r##"
      class PersonAssignment {
        person Person @stream.with_state
        assignment string @stream.with_state
      }
    
      class Person {
        name string @stream.done @stream.with_state
        age int @stream.with_state
      }
    "##,
    )
    .unwrap();

    let mut target_type = TypeIR::class("PersonAssignment");
    target_type.set_meta(TypeMeta {
        constraints: vec![],
        streaming_behavior: StreamingBehavior {
            done: false,
            state: true,
            needed: true,
        },
    });
    let target_type = target_type.to_streaming_type(&ir).to_ir_type();

    let target = render_output_format(
        &ir,
        &target_type,
        &Default::default(),
        baml_types::StreamingMode::Streaming,
    )
    .unwrap();

    let llm_data = r#"{"person": {"name": "Greg", "age": 42}, "assignment": "Write"}"#;

    let results = (0..llm_data.len() + 1)
        // let results = (0..2)
        .map(|i| {
            let partial_llm_data = &llm_data[0..i];
            let parsed_value = from_str(&target, &target_type, partial_llm_data, false);

            let value = parsed_value_to_response(
                &ir,
                parsed_value.expect("Failed to parse"),
                baml_types::StreamingMode::Streaming,
            )
            .unwrap();

            serde_json::to_value(
                vec![
                    serde_json::to_value(partial_llm_data).unwrap(),
                    serde_json::to_value(value.serialize_partial()).unwrap(),
                ]
                .into_iter()
                .collect::<Vec<_>>(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();

    let json = serde_json::to_string(&results).unwrap();
    eprintln!("{json}");

    // assert!(false);
}
