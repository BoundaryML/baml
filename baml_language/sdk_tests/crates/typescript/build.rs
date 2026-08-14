// Codegen + scaffold-emit driver lives in
// `sdk_tests/harness_setup/src/typescript.rs`. `run_all`
// discovers every fixture under `sdk_tests/fixtures/`, runs the Node
// TypeScript generator, and emits
// the per-fixture `#[test]` scaffold (a sequence of
// `::sdk_test_harness_runner::*` invocations) to `OUT_DIR`.
fn main() {
    sdk_test_harness_setup::typescript::run_all();
}
