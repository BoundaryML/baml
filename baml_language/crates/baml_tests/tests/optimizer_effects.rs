use baml_tests::{
    engine::{OptLevel, run_test},
    stdlib_prefix::compile_source_with_opt,
};
use bex_external_types::BexExternalValue;
use bex_vm_types::Object;
use indexmap::IndexMap;

const CLASSES: &str = include_str!("../baml_src/ns_classes/classes.baml");
const PATTERNS: &str = include_str!("../baml_src/ns_array_rest_binding/array_rest_binding.baml");
const GC: &str = include_str!("../baml_src/ns_gc/gc.baml");

const SOURCE: &str = r#"
class EffectsBox { value: int }
class EffectsArrays { values: int[] }

function bump(box: EffectsBox) -> int {
    box.value = box.value + 1
    box.value
}
function allocation_effects() -> int {
    let box = EffectsBox { value: 0 }
    let unused = EffectsBox { value: bump(box) }
    box.value
}
function allocation_panic(divisor: int) -> int {
    { let unused = [1 / divisor]; 42 }
        catch (e) { baml.panics.DivisionByZero => 7 }
}
function index_initializer(xs: int[], index: int) -> int {
    { let unused = EffectsBox { value: xs[index] }; 42 }
        catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function guarded_read(xs: int[]) -> int {
    if (xs.length() > 0) { let unused = xs[0] }
    42
}
function guarded_negative_read(xs: int[]) -> int {
    if (xs.length() > 0) { let unused = xs[-1] }
    42
}
function guarded_suffix_read(xs: int[]) -> int {
    if (xs.length() >= 2) { let unused = xs[xs.length() - 2] }
    42
}
function projected_bounds(box: EffectsArrays) -> int {
    if (box.values.length() > 0) { let unused = box.values[0] }
    42
}
function projection_write_invalidates_bounds() -> int {
    let box = EffectsArrays { values: [1] }
    {
        if (box.values.length() > 0) { box.values = []; let unused = box.values[0] }
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function repeated_read_after_mutation(xs: int[]) -> int {
    {
        let first = xs[0]
        xs.pop()
        let second = xs[0]
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function copied_predicate_after_reassignment(index: int) -> int {
    {
        let safe = index < 4611686018427387903
        index = 4611686018427387903
        if (safe) { let unused = index + 1 }
        42
    } catch (e) { baml.panics.IntegerOverflow => 7 }
}
function captured_index_changes() -> int {
    let index = 0
    let change = () => { index = 100 }
    {
        let xs = [1]
        if (index == 0) { change(); let unused = xs[index] }
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function parameter_default(value: int = 1) -> int { value + 1 }
function parameter_copy_before_write(value: int) -> int {
    let values = [value, { value = 10; value }]
    values[0]
}
function mutation_invalidates_bounds() -> int {
    let xs = [1]
    {
        if (xs.length() > 0) {
            let alias = xs
            alias.pop()
            let unused = xs[0]
        }
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function reassignment_invalidates_bounds() -> int {
    let xs = [1]
    {
        let nonempty = xs.length() > 0
        xs = []
        if (nonempty) { let unused = xs[0] }
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function index_reassignment_invalidates_bounds() -> int {
    let xs = [1]
    let index = 0
    {
        if (xs.length() > 0) { index = 100; let unused = xs[index] }
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function stale_length_is_not_a_bounds_proof() -> int {
    let xs = [1]
    {
        let size = xs.length()
        xs.pop()
        if (size > 0) { let unused = xs[size - 1] }
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function unguarded_join(flag: bool) -> int {
    let xs: int[] = []
    {
        if (flag) { if (xs.length() == 0) { return 42 } }
        let unused = xs[0]
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function handler_invalidates_facts() -> int {
    let xs = [1]
    let index = 0
    {
        { index = 10; let unused = 1 / 0 }
            catch (e) { baml.panics.DivisionByZero => null }
        let unused = xs[index]
        42
    } catch (e) { baml.panics.IndexOutOfBounds => 7 }
}
function may_overflow(value: int) -> int {
    { let unused = value + 1; 42 }
        catch (e) { baml.panics.IntegerOverflow => 7 }
}
function bounded_arithmetic(value: int) -> int {
    if (value < 4611686018427387903) { let unused = value + 1 }
    42
}
function loop_bound(count: int) -> int {
    let index = 0
    while (index < count) {
        if (index < 500) { let unused = [index, index + 1, index + 2] }
        index = index + 1
    }
    index
}
// Compile-only convergence probe: never invoke this billion-iteration loop.
function large_loop_analysis() -> int {
    let index = 0
    while (index < 1000000000) { let unused = [index, index + 1]; index = index + 1 }
    index
}
function verify_effects() -> bool {
    allocation_effects() == 1 && allocation_panic(0) == 7 && allocation_panic(1) == 42
        && index_initializer([], 0) == 7 && index_initializer([1], 0) == 42
        && guarded_read([]) == 42 && guarded_read([1]) == 42
        && guarded_negative_read([]) == 42 && guarded_negative_read([1]) == 42
        && guarded_suffix_read([1]) == 42 && guarded_suffix_read([1, 2]) == 42
        && projected_bounds(EffectsArrays { values: [] }) == 42
        && projected_bounds(EffectsArrays { values: [1] }) == 42
        && projection_write_invalidates_bounds() == 7 && repeated_read_after_mutation([1]) == 7
        && copied_predicate_after_reassignment(0) == 7 && captured_index_changes() == 7
        && parameter_default() == 2 && parameter_default(value = 7) == 8
        && parameter_copy_before_write(3) == 3
        && mutation_invalidates_bounds() == 7 && reassignment_invalidates_bounds() == 7
        && index_reassignment_invalidates_bounds() == 7 && stale_length_is_not_a_bounds_proof() == 7
        && unguarded_join(false) == 7 && unguarded_join(true) == 42
        && handler_invalidates_facts() == 7
        && may_overflow(4611686018427387903) == 7 && may_overflow(0) == 42
        && bounded_arithmetic(4611686018427387903) == 42 && bounded_arithmetic(0) == 42
        && loop_bound(1000) == 1000
}
"#;

#[tokio::test]
async fn discardability_preserves_effects_and_invalidates_stale_proofs() {
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let output = run_test(SOURCE, "verify_effects", IndexMap::new(), opt).await;
        assert_eq!(output.result, Ok(BexExternalValue::Bool(true)), "{opt:?}");
    }
}

#[tokio::test]
async fn growing_parser_fixture_keeps_observable_panics() {
    let source = format!(
        "{}\n{}\n{}",
        include_str!("../baml_src/ns_fixtures/ns_parser_expressions/field_access.baml"),
        include_str!("../baml_src/ns_fixtures/ns_parser_expressions/index_access.baml"),
        r#"
        function field_case() -> string {
            FieldAccess(User { name: "Ada", profile: Profile {
                bio: "", settings: Settings { theme: "dark" }
            }, tags: [] }) catch (e) { baml.panics.IndexOutOfBounds => "caught" }
        }
        function index_case(users: IndexUser[]) -> string {
            IndexAccess(users) catch (e) { baml.panics.IndexOutOfBounds => "caught" }
        }
        function verify_parser_panics() -> bool {
            let tagged = IndexUser { name: "Ada", age: 1, tags: ["tag"] }
            let empty = IndexUser { name: "Ada", age: 1, tags: [] }
            field_case() == "caught" && index_case([tagged]) == "caught"
                && index_case([empty, tagged]) == "caught"
                && index_case([tagged, empty]) == "Ada"
        }
        "#
    );
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let output = run_test(&source, "verify_parser_panics", IndexMap::new(), opt).await;
        assert_eq!(output.result, Ok(BexExternalValue::Bool(true)), "{opt:?}");
    }
}

#[test]
fn dead_allocations_and_guarded_patterns_keep_their_old_instruction_budgets() {
    let source = format!("{CLASSES}\n{PATTERNS}\n{GC}\n{SOURCE}");
    let program = compile_source_with_opt(&source, OptLevel::Two);
    for (name, budget) in [
        ("nested_construction_dead_store_fn", 2),
        ("spread_does_not_break_locals_fn", 4),
        ("arb_tail_len", 21),
        ("arb_grab", 30),
        ("arb_pushing_to_rest_does_not_mutate_source", 43),
        ("gc_map_survives_pressure", 17),
        ("parameter_default", 10),
    ] {
        let index = program.function_index(&format!("user.{name}")).unwrap();
        let Some(Object::Function(function)) = program.objects.get(index) else {
            panic!("expected function");
        };
        assert!(
            function.bytecode.instructions.len() <= budget,
            "{name}: {:?}",
            function.bytecode.instructions
        );
    }
    for name in [
        "guarded_read",
        "guarded_negative_read",
        "guarded_suffix_read",
        "projected_bounds",
        "bounded_arithmetic",
    ] {
        let index = program.function_index(&format!("user.{name}")).unwrap();
        let Some(Object::Function(function)) = program.objects.get(index) else {
            panic!("expected function");
        };
        assert!(
            !function
                .bytecode
                .instructions
                .iter()
                .any(|instruction| matches!(
                    instruction,
                    bex_vm_types::Instruction::LoadArrayElement
                        | bex_vm_types::Instruction::AddInt
                        | bex_vm_types::Instruction::BinOp(bex_vm_types::BinOp::Add)
                )),
            "{name}: {:?}",
            function.bytecode.instructions
        );
    }
}
