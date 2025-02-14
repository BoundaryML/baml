use crate::helpers::render_output_format;
#[cfg(test)]
use crate::{from_str, helpers::parsed_value_to_response};
use baml_types::{FieldType, StreamingBehavior};
use internal_baml_core::ir::repr::make_test_ir;

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

    let target_type = FieldType::WithMetadata {
        base: Box::new(FieldType::class("PersonAssignment")),
        constraints: vec![],
        streaming_behavior: StreamingBehavior {
            done: false,
            state: true,
        },
    };
    let target = render_output_format(&ir, &target_type, &Default::default()).unwrap();

    let llm_data = r#"{"person": {"name": "Greg", "age": 42}, "assignment": "Write"}"#;

    let results = (0..llm_data.len() + 1)
        // let results = (0..2)
        .map(|i| {
            let partial_llm_data = &llm_data[0..i];
            let parsed_value = from_str(&target, &target_type, partial_llm_data, true);
            let value =
                parsed_value_to_response(&ir, parsed_value.unwrap(), &target_type, true).unwrap();

            serde_json::to_value(&vec![
                serde_json::to_value(partial_llm_data).unwrap(),
                serde_json::to_value(&value.serialize_partial()).unwrap(),
            ])
            .unwrap()
        })
        .collect::<Vec<_>>();

    let json = serde_json::to_string(&results).unwrap();
    eprintln!("{}", json);

    // assert!(false);
}
