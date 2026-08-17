//! Tests for bytecode display formats (textual, expanded, expanded unoptimized).

use baml_tests::engine::{OptLevel, compile_source_with_opt};
use bex_vm::debug::{BytecodeFormat, display_program};
use bex_vm_types::{Function, Object};

fn compile_display_functions(source: &str, opt: OptLevel) -> Vec<(String, Function)> {
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
    // Normalize CRLF → LF so line numbers are consistent across platforms.
    let source = include_str!("bytecode_display.baml").replace("\r\n", "\n");

    let optimized = compile_display_functions(&source, OptLevel::One);
    let optimized_refs: Vec<(String, &Function)> = optimized
        .iter()
        .map(|(name, function)| (name.clone(), function))
        .collect();
    let textual = display_program(&optimized_refs, BytecodeFormat::Textual);
    let expanded = display_program(&optimized_refs, BytecodeFormat::Expanded);

    let unoptimized = compile_display_functions(&source, OptLevel::Zero);
    let unoptimized_refs: Vec<(String, &Function)> = unoptimized
        .iter()
        .map(|(name, function)| (name.clone(), function))
        .collect();
    let expanded_unoptimized = display_program(&unoptimized_refs, BytecodeFormat::Expanded);

    insta::with_settings!({omit_expression => true, snapshot_path => "snapshots"}, {
        insta::assert_snapshot!("bytecode_display_textual", textual);
        insta::assert_snapshot!("bytecode_display_expanded", expanded);
        insta::assert_snapshot!("bytecode_display_expanded_unoptimized", expanded_unoptimized);
    });
}
