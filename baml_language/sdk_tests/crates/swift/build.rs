// Codegen + scaffold-emit driver lives in
// `sdk_tests/codegen/src/swift.rs`. `run_all` discovers every
// fixture under `sdk_tests/fixtures/`, emits one `<fixture>/generated/`
// SwiftPM package per fixture under this crate, and writes the
// per-fixture `#[test]` scaffold (a sequence of
// `::sdk_test_harness_runner::*` invocations) to `OUT_DIR`.
fn main() {
    sdk_test_codegen::swift::run_all();
}
