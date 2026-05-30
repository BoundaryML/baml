//! Verifies the checked-in generated files are still in sync with the
//! `baml_std` source of truth. Depends only on `baml_rustgen_check` (zero deps) — it
//! does NOT run the codegen, so it never pulls `baml_compiler2_ast` into the
//! build graph. Regenerate with `cargo run -p tools_rustgen` (or `mise run codegen`).

fn main() {
    baml_rustgen_check::rerun_if_baml_std_changed();
    baml_rustgen_check::assert_generated_matches_baml_std("sys_ops/src/io_generated.rs");
    baml_rustgen_check::assert_generated_matches_baml_std("sys_ops/src/io_adapter.rs");
}
