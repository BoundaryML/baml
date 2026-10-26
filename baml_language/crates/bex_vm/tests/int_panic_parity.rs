//! The VM's `int` panics must be byte-identical to `bex_lang`'s: generated
//! native code raises the shared [`bex_lang::Panic`] directly, so a drift in
//! the interpreter's cold error paths would make the two backends disagree on
//! user-visible output. Each case runs real bytecode through the VM, reads the
//! thrown `baml.panics.*` object back off the heap, and compares its class
//! and field with the shared crate's rendering of the same operation.

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_lang::{Int63, Panic};
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{Object, Value, errors::VmError};

/// Run `main` and return the uncaught panic object rendered the way the
/// engine prints it: `baml.panics.Class {field: value}`.
fn uncaught_panic(source: &str) -> String {
    let program = compile_source(source);
    let entry = program.rendered_callables()["user.main"].object.raw();
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
    let thrown = loop {
        match vm.exec() {
            Ok(VmExecState::EarlyYield) => {}
            Ok(other) => panic!("expected a panic, got {other:?}"),
            Err(VmError::ThrownUnhandled { value, .. }) => break value,
            Err(VmError::Thrown(thrown)) => break thrown.value,
            Err(other) => panic!("expected a thrown panic, got {other:?}"),
        }
    };
    render_instance(&vm, thrown)
}

fn render_instance(vm: &BexVm, value: Value) -> String {
    let ptr = value.as_object_ptr().expect("panic is a heap object");
    let Object::Instance(instance) = vm.get_object(ptr) else {
        panic!("panic is an instance");
    };
    let Object::Class(class) = vm.get_object(instance.class) else {
        panic!("instance points at a class");
    };
    assert_eq!(instance.field_len(), 1, "int panics carry a single field");
    let (field_name, field) = (&class.fields[0].name, instance.load_field(0));
    let rendered = match field.as_int() {
        Some(int) => int.to_string(),
        None => format!("{:?}", vm.as_string(&field).expect("string field").as_str()),
    };
    format!("{} {{{field_name}: {rendered}}}", class.name.display_name())
}

fn binary(op: &str, left: i64, right: i64) -> String {
    format!(
        "function apply(a: int, b: int) -> int {{ a {op} b }}\n\
         function main() -> int {{ apply({left}, {right}) }}"
    )
}

fn shared(result: Result<Int63, Panic>) -> String {
    result
        .expect_err("shared operation panics")
        .render_readable()
}

fn int63(value: i64) -> Int63 {
    Int63::new(value).expect("in range")
}

#[test]
fn max_plus_one_overflows_identically() {
    let vm = uncaught_panic(&binary("+", Int63::MAX.get(), 1));
    assert_eq!(vm, shared(bex_lang::int::add(Int63::MAX, int63(1))));
    assert_eq!(
        vm,
        r#"baml.panics.IntegerOverflow {message: "4611686018427387903 + 1 overflows int"}"#
    );
}

#[test]
fn min_divided_by_minus_one_overflows_identically() {
    let vm = uncaught_panic(&binary("/", Int63::MIN.get(), -1));
    assert_eq!(vm, shared(bex_lang::int::div(Int63::MIN, int63(-1))));
    assert_eq!(
        vm,
        r#"baml.panics.IntegerOverflow {message: "-4611686018427387904 / -1 overflows int"}"#
    );
}

#[test]
fn division_by_zero_carries_the_dividend_identically() {
    let vm = uncaught_panic(&binary("/", 1, 0));
    assert_eq!(
        vm,
        shared(bex_lang::int::div(int63(1), bex_lang::int::ZERO))
    );
    assert_eq!(vm, "baml.panics.DivisionByZero {dividend: 1}");
    let vm = uncaught_panic(&binary("%", 7, 0));
    assert_eq!(
        vm,
        shared(bex_lang::int::rem(int63(7), bex_lang::int::ZERO))
    );
}

#[test]
fn negative_shift_count_is_identical() {
    let vm = uncaught_panic(&binary("<<", 1, -1));
    assert_eq!(vm, shared(bex_lang::int::shl(int63(1), int63(-1))));
    assert_eq!(
        vm,
        r#"baml.panics.NegativeBitShift {message: "bit shift count is negative: -1"}"#
    );
    let vm = uncaught_panic(&binary(">>", 1, -1));
    assert_eq!(vm, shared(bex_lang::int::shr(int63(1), int63(-1))));
}

#[test]
fn negating_min_overflows_identically() {
    let source = format!(
        "function apply(a: int) -> int {{ -a }}\nfunction main() -> int {{ apply({}) }}",
        Int63::MIN.get()
    );
    let vm = uncaught_panic(&source);
    assert_eq!(vm, shared(bex_lang::int::neg(Int63::MIN)));
    assert_eq!(
        vm,
        r#"baml.panics.IntegerOverflow {message: "-(-4611686018427387904) overflows int"}"#
    );
}
