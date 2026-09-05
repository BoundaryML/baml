use baml_tests::{
    engine::{OptLevel, run_test},
    stdlib_prefix::compile_source_with_opt,
};
use bex_external_types::BexExternalValue;
use bex_vm_types::Object;
use indexmap::IndexMap;

const BACKTICKS: &str = include_str!("../baml_src/ns_backtick_strings/backtick_strings.baml");
const SOURCE: &str = r#"
class PrefixCounter { value: int }
function prefix_number(value: int) -> int { value }
function prefix_identity(value: int) -> int { value }
function prefix_tick(counter: PrefixCounter) -> int {
    counter.value = counter.value + 1
    counter.value
}
function prefix_subtract() -> int { 20 - prefix_number(3) }
function prefix_divide() -> int { 20 / prefix_number(4) }
function prefix_nested() -> int { (20 - prefix_number(3)) * 2 }
function prefix_call_arg() -> int { prefix_identity(20 - prefix_number(3)) }
function prefix_order() -> int {
    let counter = PrefixCounter { value: 0 }
    prefix_tick(counter) - prefix_tick(counter)
}
function prefix_fail(counter: PrefixCounter, divisor: int) -> int {
    prefix_tick(counter)
    1 / divisor
}
function binary_prefix_unwind(divisor: int) -> int {
    let counter = PrefixCounter { value: 0 }
    { 20 - prefix_fail(counter, divisor) }
        catch (e) { baml.panics.DivisionByZero => counter.value + 40 }
}
function binary_prefix_overflow() -> int {
    { 4611686018427387903 + prefix_number(1) }
        catch (e) { baml.panics.IntegerOverflow => 7 }
}
function verify_prefixes() -> string {
    if (bt_case_h() != "!hi") { return "interpolation at end" }
    if (bt_case_i() != "X Y Z") { return "nested concatenation" }
    if (bt_case_n() != "Hello Ada!") { return "interpolation as call argument" }
    if (prefix_subtract() != 17) { return "subtraction order" }
    if (prefix_divide() != 5) { return "division order" }
    if (prefix_nested() != 34) { return "nested arithmetic" }
    if (prefix_call_arg() != 17) { return "arithmetic as call argument" }
    if (prefix_order() != -1) { return "call evaluation order" }
    if (binary_prefix_unwind(0) != 41) { return "call unwinding" }
    if (binary_prefix_unwind(1) != 19) { return "non-failing call" }
    if (binary_prefix_overflow() != 7) { return "operator unwinding" }
    "ok"
}
"#;

#[test]
fn inlined_binary_prefixes_do_not_spill_call_results() {
    let program = compile_source_with_opt(&format!("{BACKTICKS}\n{SOURCE}"), OptLevel::Two);
    for (name, count) in [
        ("bt_case_h", 6),
        ("bt_case_i", 8),
        ("bt_case_n", 7),
        ("prefix_subtract", 5),
        ("prefix_divide", 5),
        ("prefix_nested", 7),
        ("prefix_call_arg", 6),
    ] {
        let index = program.function_index(&format!("user.{name}")).unwrap();
        let Some(Object::Function(function)) = program.objects.get(index) else {
            panic!("expected function {name}");
        };
        assert_eq!(
            function.bytecode.instructions.len(),
            count,
            "{name}: {:?}",
            function.bytecode.instructions
        );
        assert_eq!(function.real_local_count, 0, "{name}");
    }
}

#[tokio::test]
async fn inlined_binary_prefixes_preserve_order_and_unwinding() {
    let source = format!("{BACKTICKS}\n{SOURCE}");
    for opt in [OptLevel::Zero, OptLevel::One, OptLevel::Two] {
        let output = run_test(&source, "verify_prefixes", IndexMap::new(), opt).await;
        assert_eq!(
            output.result,
            Ok(BexExternalValue::String("ok".into())),
            "{opt:?}"
        );
    }
}
