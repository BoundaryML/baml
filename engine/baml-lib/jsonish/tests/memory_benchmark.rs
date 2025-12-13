#[cfg(not(target_arch = "wasm32"))]
use baml_types::TypeIR;
#[cfg(not(target_arch = "wasm32"))]
use jsonish::{
    from_str,
    helpers::common::{ARRAY_UNION_SCHEMA, UNION_SCHEMA},
};
#[cfg(not(target_arch = "wasm32"))]
use peak_alloc::PeakAlloc;

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static PEAK_ALLOC: PeakAlloc = PeakAlloc;

#[cfg(not(target_arch = "wasm32"))]
macro_rules! test_memory_usage {
    ($test_name:ident, $target_string:expr, $schema:expr, $goal:expr) => {
        #[test_log::test]
        fn $test_name() {
            let ir = jsonish::helpers::load_test_ir($schema);
            let target_string = $target_string;

            let start = std::time::Instant::now();
            let _ = serde_json::from_str::<serde_json::Value>(target_string).unwrap();
            let end = std::time::Instant::now();
            log::info!("Time taken for serde: {:?}", end - start);

            let target = $goal.clone().to_streaming_type(&ir).to_ir_type();
            let of = jsonish::helpers::render_output_format(
                &ir,
                &target,
                &Default::default(),
                baml_types::StreamingMode::Streaming,
            )
            .unwrap();
            // Reset peak memory tracking
            PEAK_ALLOC.reset_peak_usage();

            let start = std::time::Instant::now();
            let mut num_parses = 0;
            for i in 0..target_string.chars().count() {
                if i % 5 != 0 {
                    continue;
                }
                num_parses += 1;
                let result = from_str(
                    &of,
                    &target,
                    &target_string.chars().take(i).collect::<String>(),
                    true,
                );
            }

            let of = jsonish::helpers::render_output_format(
                &ir,
                &$goal,
                &Default::default(),
                baml_types::StreamingMode::NonStreaming,
            )
            .unwrap();
            // Run the function we want to measure
            let result = from_str(&of, &$goal, &target_string, true);
            let end = std::time::Instant::now();

            let time_taken = end - start;
            log::info!("{} - Time taken: {:?}", $goal, time_taken);
            log::info!("{} - Time per parse: {:?}", $goal, time_taken / num_parses);

            assert!(
                time_taken / num_parses < std::time::Duration::from_millis(20),
                "{} - Parsing is too slow: {:?} is more than 20ms",
                $goal,
                time_taken / num_parses
            );

            // Get peak memory usage
            let peak_memory = PEAK_ALLOC.peak_usage_as_mb();

            log::info!("{} - Peak memory usage: {:.2} MB", $goal, peak_memory);
            assert!(
                peak_memory < 2.0,
                "{} - Peak memory usage is too high: {:.2} MB > 2.0 MB",
                $goal,
                peak_memory
            );

            assert!(
                result.is_ok(),
                "{} - Parse failed: {:?}",
                $goal,
                result.err()
            );
        }
    };
}

#[cfg(not(target_arch = "wasm32"))]
test_memory_usage!(
    test_story1_memory_usage,
    jsonish::helpers::common::JSON_STRING_STORY,
    UNION_SCHEMA,
    TypeIR::class("Story1")
);

#[cfg(not(target_arch = "wasm32"))]
test_memory_usage!(
    test_story2_memory_usage,
    jsonish::helpers::common::JSON_STRING_STORY,
    UNION_SCHEMA,
    TypeIR::class("Story2")
);

#[cfg(not(target_arch = "wasm32"))]
test_memory_usage!(
    test_story3_memory_usage,
    jsonish::helpers::common::JSON_STRING_STORY,
    UNION_SCHEMA,
    TypeIR::class("Story3")
);

#[cfg(not(target_arch = "wasm32"))]
test_memory_usage!(
    test_story4_memory_usage,
    jsonish::helpers::common::JSON_STRING_STORY,
    UNION_SCHEMA,
    TypeIR::class("Story4")
);

// Test for array union hint optimization.
// This test uses an array of (TextBlock | ImageBlock | CodeBlock) unions
// where most elements are TextBlock. The hint optimization should help
// by trying the previously successful variant first.
#[cfg(not(target_arch = "wasm32"))]
test_memory_usage!(
    test_array_union_hint,
    jsonish::helpers::common::JSON_STRING_ARRAY_UNION,
    ARRAY_UNION_SCHEMA,
    TypeIR::class("Document")
);
