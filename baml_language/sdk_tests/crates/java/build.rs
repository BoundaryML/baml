// Codegen + scaffold-emit driver lives in
// `sdk_tests/codegen/src/java.rs`. `run_all` discovers every
// fixture under `sdk_tests/fixtures/`, runs
// `sdkgen_java::to_source_code_with_bytecode` (a stub for now —
// panics are downgraded to build diagnostics), writes the per-fixture
// Gradle files, and emits the per-fixture `#[test]` scaffold (a
// sequence of `::sdk_test_harness_runner::*` invocations) to `OUT_DIR`.
fn main() {
    sdk_test_codegen::java::run_all();
}
