//! Stage the Ruby SDK fixtures.
use std::fs;

use baml_sdkgen_types::NamingConvention;
use sdk_test_harness_runner::fixtures;

use crate::{CodegenCtx, Overlay, load_fixture, write_codegen_output};

pub fn run_all(ctx: &CodegenCtx) {
    for fixture in fixtures::checked_shared(&ctx.fixtures_root) {
        let loaded = load_fixture(&ctx.fixtures_root, fixture);
        let (files, skipped) = sdkgen_ruby_sorbet::to_source_code_with_bytecode_and_skipped(
            &loaded.pool,
            &loaded.baml_bytecode,
            NamingConvention::Language,
            None,
        );
        eprintln!("Ruby {fixture} skipped: {}", skipped.join(", "));
        let root = ctx.crate_dir.join(fixture);
        let generated = root.join("generated");
        if generated.exists() {
            fs::remove_dir_all(&generated).unwrap();
        }
        write_codegen_output(&generated, files, fixture);
        let mut overlay = Overlay::new(&root);
        overlay.copy_tree(&root.join("customizable"), "");
        overlay.install();
    }
}
