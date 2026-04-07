//! Unified tests for error handling and stack traces.

use baml_tests::baml_test;

#[tokio::test]
async fn error_stack_trace() {
    // Call chain: main -> one -> two -> three, where three throws
    let output = baml_test!(
        r#"
function three() -> int {
  throw "boom"
}

function two() -> int {
  three()
}

function one() -> int {
  two()
}

function main() -> int {
  one()
}
"#
    );

    let Err(bex_engine::EngineError::UnhandledThrow { value: _, trace }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };

    // Verify trace has frames for the call chain
    assert!(
        trace.len() >= 4,
        "expected at least 4 frames, got {}",
        trace.len()
    );

    // Frames should include main, one, two, three (order depends on implementation)
    let frame_names: Vec<&str> = trace.iter().map(|f| f.function_name.as_str()).collect();
    assert!(
        frame_names.iter().any(|n| n.contains("main")),
        "missing main frame, got: {frame_names:?}"
    );
    assert!(
        frame_names.iter().any(|n| n.contains("one")),
        "missing one frame, got: {frame_names:?}"
    );
    assert!(
        frame_names.iter().any(|n| n.contains("two")),
        "missing two frame, got: {frame_names:?}"
    );
    assert!(
        frame_names.iter().any(|n| n.contains("three")),
        "missing three frame, got: {frame_names:?}"
    );

    // Verify that all frames have a file_path set and non-zero line numbers.
    for frame in trace {
        assert_eq!(
            frame.file_path, "test.baml",
            "frame '{}' should have file_path 'test.baml', got '{}'",
            frame.function_name, frame.file_path
        );
        assert!(
            frame.error_line > 0,
            "frame '{}' should have a non-zero error_line",
            frame.function_name
        );
    }
}
