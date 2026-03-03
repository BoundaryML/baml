//! Tests for bytecode display formats (textual, expanded, expanded unoptimized).

use baml_tests::engine::compile_source_with_opt;
use bex_vm::debug::{BytecodeFormat, display_program};
use bex_vm_types::{Function, Object};

fn compile_display_functions(
    source: &str,
    opt: baml_compiler_emit::OptLevel,
) -> Vec<(String, Function)> {
    let program = compile_source_with_opt(source, opt);
    let mut functions: Vec<(String, Function)> = program
        .function_indices
        .iter()
        .filter(|(name, _)| !name.starts_with("baml."))
        .filter_map(|(name, idx)| match program.objects.get(*idx) {
            Some(Object::Function(f)) => Some((name.clone(), (**f).clone())),
            _ => None,
        })
        .collect();
    functions.sort_by(|(a, _), (b, _)| a.cmp(b));
    functions
}

#[test]
fn bytecode_display_formats() {
    let source = include_str!("bytecode_display.baml");

    // 1. Textual format (optimized)
    let o1 = compile_display_functions(source, baml_compiler_emit::OptLevel::One);
    let o1_refs: Vec<(String, &Function)> = o1.iter().map(|(n, f)| (n.clone(), f)).collect();
    let textual = display_program(&o1_refs, BytecodeFormat::Textual);

    // 2. Expanded format (optimized)
    let expanded = display_program(&o1_refs, BytecodeFormat::Expanded);

    // 3. Expanded format (unoptimized)
    let o0 = compile_display_functions(source, baml_compiler_emit::OptLevel::Zero);
    let o0_refs: Vec<(String, &Function)> = o0.iter().map(|(n, f)| (n.clone(), f)).collect();
    let expanded_unopt = display_program(&o0_refs, BytecodeFormat::Expanded);

    insta::with_settings!({omit_expression => true, snapshot_path => "snapshots"}, {
        insta::assert_snapshot!("bytecode_display_textual", textual);
        insta::assert_snapshot!("bytecode_display_expanded", expanded);
        insta::assert_snapshot!("bytecode_display_expanded_unoptimized", expanded_unopt);
    });
}
