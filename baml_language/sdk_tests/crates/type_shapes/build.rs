// Codegen driver lives in `sdk_tests/build/src/lib.rs` — see there for
// the full pipeline (discover .baml → ProjectDatabase → diagnostics →
// `build_symbol_pool` → `codegen_python::to_source_code`) and the
// pyproject.toml shape.
fn main() {
    sdk_test_build::run(env!("CARGO_PKG_NAME"));
}
