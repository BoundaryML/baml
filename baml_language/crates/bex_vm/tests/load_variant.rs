//! `Status.X` compiles to one `LoadVariant` instead of a `LoadConst` of the
//! variant index followed by `AllocVariant`, and evaluates to the enum's
//! shared variant object, as `AllocVariant` did.

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{BexVm, BytecodeProgram, VmExecState, convert_program};
use bex_vm_types::{Object, bytecode::OpCode, types::Function};

fn function<'a>(program: &'a BytecodeProgram, name: &str) -> &'a Function {
    program
        .objects
        .iter()
        .find_map(|object| match object {
            Object::Function(function) if function.name() == name => Some(function.as_ref()),
            _ => None,
        })
        .expect("function exists")
}

fn ops(function: &Function) -> Vec<OpCode> {
    let code = &function.bytecode.compact.as_ref().unwrap().code;
    let mut result = Vec::new();
    let mut pc = 0;
    while pc < code.len() {
        let op = OpCode::try_from(code[pc]).expect("valid opcode");
        result.push(op);
        pc += op.encoded_size();
    }
    result
}

/// Run `main` with the fast path on and off; both must agree.
fn run(source: &str) -> i64 {
    let results = [true, false].map(|fast_path| {
        let program = compile_source(source);
        let entry = program.rendered_callables()["user.main"].object.raw();
        let mut vm =
            BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("valid program");
        vm.dispatch_fast_path = fast_path;
        vm.disable_telemetry();
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        match vm.exec().expect("execution succeeds") {
            VmExecState::Complete(value) => value.as_int().expect("integer result"),
            other => panic!("expected completion, got {other:?}"),
        }
    });
    assert_eq!(results[0], results[1], "fast path and full dispatch differ");
    results[0]
}

const SOURCE: &str = r#"
    enum Status {
      Todo
      Doing
      Done
    }
    class Task { status: Status }
    function next(s: Status) -> Status {
      if (s == Status.Todo) { return Status.Doing; }
      if (s == Status.Doing) { return Status.Done; }
      Status.Todo
    }
    function code(s: Status) -> int {
      match (s) {
        Status.Todo => 1
        Status.Doing => 2
        Status.Done => 3
      }
    }
    function main() -> int {
      let t = Task { status: Status.Todo };
      let total = 0;
      for (let i = 0; i < 7; i += 1) {
        t.status = next(t.status);
        total = total * 4 + code(t.status);
      }
      total
    }
"#;

#[test]
fn variant_constants_load_in_one_instruction() {
    let program = convert_program(compile_source(SOURCE)).unwrap();
    for name in ["next", "main"] {
        let function_ops = ops(function(&program, name));
        assert!(
            function_ops.contains(&OpCode::LoadVariant),
            "LoadVariant missing from {name}: {function_ops:?}"
        );
        assert!(
            !function_ops.contains(&OpCode::AllocVariant),
            "AllocVariant left in {name}: {function_ops:?}"
        );
    }
}

#[test]
fn loaded_variants_compare_and_match_like_allocated_ones() {
    // 2, 3, 1, 2, 3, 1, 2 in base 4.
    let expected = [2, 3, 1, 2, 3, 1, 2]
        .into_iter()
        .fold(0, |total, code| total * 4 + code);
    assert_eq!(run(SOURCE), expected);
}

#[test]
fn a_variant_constant_is_the_enum_shared_variant() {
    let program = compile_source(
        r#"
        enum Status {
          Todo
          Done
        }
        function main() -> Status { Status.Done }
        "#,
    );
    let entry = program.rendered_callables()["user.main"].object.raw();
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("valid program");
    vm.disable_telemetry();
    vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
    let VmExecState::Complete(value) = vm.exec().expect("execution succeeds") else {
        panic!("expected completion");
    };
    let variant_ptr = value.as_object_ptr().expect("a variant object");
    let Object::Variant(variant) = vm.get_object(variant_ptr) else {
        panic!("expected a variant");
    };
    assert_eq!(variant.index, 1);
    let Object::Enum(enm) = vm.get_object(variant.enm) else {
        panic!("a variant's enum is an enum");
    };
    assert_eq!(enm.shared_variants.get(1), Some(&variant_ptr));
}
