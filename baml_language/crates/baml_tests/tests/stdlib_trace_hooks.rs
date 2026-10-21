//! Small stdlib plumbing stays hidden without changing dynamic dispatch or
//! suppressing spans selected by user callbacks.

use baml_tests::{
    engine::{named_and_interface_body_functions, run_test},
    stdlib_prefix::{OptLevel, compile_source_with_opt},
};
use bex_engine::BexExternalValue;
use bex_vm_types::{Object, bytecode::Instruction};

#[test]
fn primitive_stdlib_helpers_select_hidden_tracing() {
    let db = baml_db::testing::setup_test_db("function main() -> int { 42 }");
    let diagnostics: Vec<_> = baml_db::collect_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == baml_compiler_diagnostics::Severity::Error)
        .map(|diagnostic| diagnostic.message)
        .collect();
    assert!(
        diagnostics.is_empty(),
        "stdlib hook diagnostics: {diagnostics:?}"
    );
    for opt in [OptLevel::Zero, OptLevel::Two] {
        let program = compile_source_with_opt("function main() -> int { 42 }", opt);
        let functions: Vec<_> = named_and_interface_body_functions(&program)
            .filter_map(|(name, index)| match program.objects.get(index) {
                Some(Object::Function(function)) => Some((name, function)),
                _ => None,
            })
            .collect();

        for name in [
            "baml.String.length",
            "baml.String.is_empty",
            "baml.iter.ArrayIterator.new",
            "baml.iter.Range.new",
            "baml.iter.Repeat.new",
        ] {
            let (_, function) = functions
                .iter()
                .find(|(candidate, _)| candidate == name)
                .unwrap_or_else(|| panic!("missing stdlib helper {name}"));
            assert!(
                function
                    .bytecode
                    .instructions
                    .iter()
                    .any(|instruction| matches!(instruction, Instruction::TraceHookHidden)),
                "{opt:?}: {name} must select hidden tracing; instructions={:?}",
                function.bytecode.instructions
            );
        }

        for leaf in ["ArrayIterator", "Range", "Repeat"] {
            let next: Vec<_> = functions
                .iter()
                .filter(|(name, _)| name.contains(leaf) && name.ends_with(".next"))
                .collect();
            assert!(!next.is_empty(), "missing {leaf}.next implementation");
            for (name, function) in next {
                assert!(
                    function
                        .bytecode
                        .instructions
                        .iter()
                        .any(|instruction| matches!(instruction, Instruction::TraceHookHidden)),
                    "{opt:?}: {name} must select hidden tracing"
                );
            }
        }
    }
}

#[tokio::test]
async fn hidden_stdlib_helpers_preserve_bound_virtual_and_callback_calls() {
    let source = r#"
/// baml:$trace=trace.span
function double(value: int) -> int throws never {
    assert.is_true(trace.current_span_id() != null);
    value * 2
}
function main() -> int {
    let length = "hé😀".length;
    assert.equal(length(), 3);
    assert.is_true("".is_empty());
    assert.is_true(!"x".is_empty());

    let iterator = baml.iter.ArrayIterator.new([4, 5, 6]);
    let next = iterator.next;
    assert.equal(next(), 4);
    let virtual: baml.iter.Iterator<Item = int, Error = never> = iterator;
    assert.equal(virtual.next(), 5);
    assert.equal(virtual.next(), 6);
    assert.is_true(virtual.next() is baml.iter.Done);
    assert.is_true(virtual.next() is baml.iter.Done);

    let nullable: baml.iter.Iterator<Item = int?, Error = never> = [null, 7].iter();
    assert.equal(nullable.next(), null);
    assert.equal(nullable.next(), 7);
    assert.is_true(nullable.next() is baml.iter.Done);

    assert.equal(baml.iter.Range.new(1, 4).collect(), [1, 2, 3]);
    assert.equal(baml.iter.Repeat.new("x", count = 2).collect(), ["x", "x"]);

    let sum = 0;
    for (let value in [1, 2, 3].iter().map(double)) {
        sum += value;
    }
    assert.equal(sum, 12);
    42
}
"#;
    for opt in [OptLevel::Zero, OptLevel::Two] {
        let output = run_test(source, "main", Default::default(), opt).await;
        assert_eq!(
            output
                .result
                .unwrap_or_else(|error| panic!("{opt:?}: {error:?}")),
            BexExternalValue::Int(42),
            "{opt:?}: hidden helpers must preserve results and user callback spans"
        );
    }
}
