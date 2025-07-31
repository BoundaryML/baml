#[cfg(not(target_arch = "wasm32"))]
use baml_types::TypeIR;
#[cfg(not(target_arch = "wasm32"))]
use jsonish::{from_str, helpers::common::UNION_SCHEMA};
#[cfg(not(target_arch = "wasm32"))]
use peak_alloc::PeakAlloc;

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static PEAK_ALLOC: PeakAlloc = PeakAlloc;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn test_story_memory_usage() {
    let ir = jsonish::helpers::load_test_ir(UNION_SCHEMA);
    let target_string = jsonish::helpers::common::JSON_STRING;

    let start = std::time::Instant::now();
    let _ = serde_json::from_str::<serde_json::Value>(target_string).unwrap();
    let end = std::time::Instant::now();
    println!("Time taken for serde: {:?}", end - start);

    for goal in [
        TypeIR::recursive_type_alias("JSON"),
        // TypeIR::map(TypeIR::string(), TypeIR::recursive_type_alias("JSON")),
    ] {
        let target = goal.clone().to_streaming_type(&ir).to_ir_type();
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
        let mut num_parses = 1;
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
            &goal,
            &Default::default(),
            baml_types::StreamingMode::NonStreaming,
        )
        .unwrap();
        // Run the function we want to measure
        let result = from_str(&of, &goal, &target_string, true);
        let end = std::time::Instant::now();

        let time_taken = end - start;
        println!("Time taken: {:?}", time_taken);
        println!("Time per parse: {:?}", time_taken / num_parses);

        // Get peak memory usage
        let peak_memory = PEAK_ALLOC.peak_usage_as_mb();

        println!("Peak memory usage for {goal}: {:.2} MB\n\n", peak_memory);
        match &result {
            Ok(_) => println!("Parse result successful: true"),
            Err(e) => println!("Parse result successful: false, error: {:?}", e),
        }

        // You can add assertions here if needed
        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
    }
}

// #[cfg(not(target_arch = "wasm32"))]
// #[test]
// fn test_story_memory_usage_multiple_iterations() {
//     let ir = jsonish::helpers::load_test_ir(UNION_SCHEMA);
//     let target = TypeIR::class("Story");
//     let of = jsonish::helpers::render_output_format(
//         &ir,
//         &target,
//         &Default::default(),
//         baml_types::StreamingMode::NonStreaming,
//     )
//     .unwrap();

//     // Reset peak memory tracking
//     PEAK_ALLOC.reset_peak_usage();

//     // Run multiple iterations to see memory usage pattern
//     let iterations = 1000;
//     let mut successful_parses = 0;

//     for _ in 0..iterations {
//         let result = from_str(
//             &of,
//             &target,
//             jsonish::helpers::common::JSON_STRING_STORY,
//             true,
//         );
//         if result.is_ok() {
//             successful_parses += 1;
//         }
//     }

//     // Get peak memory usage
//     let peak_memory = PEAK_ALLOC.peak_usage_as_mb();

//     println!("Peak memory usage ({} iterations): {:.2} MB", iterations, peak_memory);
//     println!("Successful parses: {}/{}", successful_parses, iterations);

//     if successful_parses != iterations {
//         // Show an example error if any failed
//         let result = from_str(
//             &of,
//             &target,
//             jsonish::helpers::common::JSON_STRING_STORY,
//             true,
//         );
//         println!("Example error: {:?}", result.err());
//     }

//     assert_eq!(successful_parses, iterations);
// }

#[cfg(target_arch = "wasm32")]
fn main() {
    // No-op for WASM builds
}
