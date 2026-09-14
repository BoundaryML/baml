//! Compile-check the Prompt Fiddle demo.
//! The runtime corpus runner lives in baml_cli/tests/baml_corpus.rs.

#[test]
fn promptfiddle_demo_compiles() {
    // This cross-workspace include is intentionally cursed: Prompt Fiddle owns
    // the demo, while this existing test binary checks it without a second compiler build.
    let source =
        include_str!("../../../../typescript2/app-promptfiddle/src/playground/default.baml");
    baml_db::testing::compile_multi_file(&[("baml_src/main.baml", source)]);
}
