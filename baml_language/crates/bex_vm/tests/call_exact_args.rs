use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use baml_type::{CallLayout, Name};
use bex_vm::{BexVm, BytecodeProgram, VmExecState, convert_program};
use bex_vm_types::{GlobalIndex, Instruction, Object, bytecode::OpCode, types::Function};

fn function<'a>(program: &'a BytecodeProgram, name: &str) -> &'a Function {
    program
        .objects
        .iter()
        .find_map(|object| match object {
            Object::Function(function) if function.name == name => Some(function.as_ref()),
            _ => None,
        })
        .expect("function exists")
}

fn ops(function: &Function) -> Vec<(usize, OpCode)> {
    let code = &function.bytecode.compact.as_ref().unwrap().code;
    let mut result = Vec::new();
    let mut pc = 0;
    while pc < code.len() {
        let op = OpCode::try_from(code[pc]).expect("valid opcode");
        result.push((pc, op));
        pc += op.encoded_size();
    }
    result
}

fn run(source: &str) -> i64 {
    let program = compile_source(source);
    let entry = program.function_index("user.main").unwrap();
    let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
    vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
    loop {
        match vm.exec().expect("execution succeeds") {
            VmExecState::Complete(value) => return value.as_int().unwrap(),
            VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
}

const SIMPLE: &str =
    "function leaf(n: int) -> int { n } function main() -> int { leaf(20) + leaf(22) }";

#[test]
fn exact_calls_preserve_operands_and_source_positions() {
    let program = convert_program(compile_source(SIMPLE)).unwrap();
    let main = function(&program, "user.main");
    let original = main.bytecode.lower_to_compact();
    let compact = main.bytecode.compact.as_ref().unwrap();
    let mut restored = compact.code.clone();
    let exact: Vec<_> = ops(main)
        .into_iter()
        .filter(|(_, op)| *op == OpCode::CallExactArgs)
        .collect();
    assert_eq!(exact.len(), 2);
    for (pc, _) in exact {
        restored[pc] = OpCode::Call as u8;
    }
    assert_eq!(restored, original.code, "only opcode bytes may change");
    assert_eq!(compact.line_table, original.line_table);
    assert_eq!(compact.exception_table, original.exception_table);
    assert_eq!(
        compact.handler_context_table,
        original.handler_context_table
    );
    assert_eq!(run(SIMPLE), 42);
}

#[test]
fn nonmatching_argument_layout_stays_general() {
    let mut program = compile_source(SIMPLE);
    let index = program.function_indices["user.main"];
    let Object::Function(main) = &mut (*program.objects)[index] else {
        unreachable!()
    };
    for layout in main.bytecode.call_layouts.values_mut() {
        *layout = CallLayout(vec![Some(Name::new("n"))]);
    }
    main.bytecode.compact = None;
    let program = convert_program(program).unwrap();
    let instructions = ops(function(&program, "user.main"));
    assert_eq!(
        instructions
            .iter()
            .filter(|(_, op)| *op == OpCode::Call)
            .count(),
        2
    );
    assert!(
        !instructions
            .iter()
            .any(|(_, op)| *op == OpCode::CallExactArgs)
    );
}

#[test]
fn writable_callee_globals_are_not_specialized() {
    let mut program = compile_source(SIMPLE);
    let global = GlobalIndex::from_raw(program.function_global_indices["user.leaf"]);
    let index = program.function_indices["user.leaf"];
    let Object::Function(leaf) = &mut (*program.objects)[index] else {
        unreachable!()
    };
    // Unreachable here, but any potential write must block specialization.
    leaf.bytecode
        .instructions
        .push(Instruction::StoreGlobal(global));
    leaf.bytecode.compact = None;
    let program = convert_program(program).unwrap();
    let instructions = ops(function(&program, "user.main"));
    assert_eq!(
        instructions
            .iter()
            .filter(|(_, op)| *op == OpCode::Call)
            .count(),
        2
    );
}

#[test]
fn generic_calls_keep_type_arguments() {
    let source =
        "function identity<T>(n: T) -> T { n } function main() -> int { identity<int>(42) }";
    let program = convert_program(compile_source(source)).unwrap();
    let instructions = ops(function(&program, "user.main"));
    assert!(instructions.iter().any(|(_, op)| *op == OpCode::Call));
    assert!(
        !instructions
            .iter()
            .any(|(_, op)| *op == OpCode::CallExactArgs)
    );
    assert_eq!(run(source), 42);
}

#[test]
fn exact_calls_compose_with_defaults_indirect_calls_and_native_callbacks() {
    assert_eq!(
        run(r#"
        function leaf(n: int) -> int { n }
        function add(a: int, b: int = 10, c: int = 20) -> int { a + b + c }
        function invoke(f: (int) -> int, n: int) -> int { f(n) }
        function main() -> int {
            let values = [1, 2].map((n: int) -> int { leaf(n) });
            add(1) + add(1, c = 3, b = 2) + invoke(leaf, 2) + values[0] + values[1]
        }
    "#),
        42
    );
}

#[test]
fn exact_recursion_preserves_overflow_catching_and_caller_state() {
    let source = r#"
        function recurse(n: int) -> int { recurse(n + 1) }
        function leaf() -> int { 35 }
        function main() -> int {
            let result = recurse(0) catch (e) { baml.panics.StackOverflow => 7 };
            result + leaf()
        }
    "#;
    assert_eq!(run(source), 42);
}

#[test]
fn exact_calls_poll_and_resume_inside_the_callee() {
    let program = compile_source(SIMPLE);
    let entry = program.function_index("user.main").unwrap();
    let flag = Arc::new(AtomicBool::new(true));
    let mut vm = BexVm::from_program(program, Arc::clone(&flag)).unwrap();
    vm.early_yield = bex_vm_types::EarlyYieldCheck::with_interval(Arc::clone(&flag), 1);
    vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
    assert!(matches!(vm.exec().unwrap(), VmExecState::EarlyYield));
    flag.store(false, std::sync::atomic::Ordering::Relaxed);
    match vm.exec().unwrap() {
        VmExecState::Complete(value) => assert_eq!(value.as_int(), Some(42)),
        other => panic!("expected completion after resuming, got {other:?}"),
    }
}

#[test]
fn exact_transitions_preserve_resume_pc_across_callbacks_and_reused_depths() {
    let source = r#"
        function leaf(n: int) -> int { n }
        function inner(n: int) -> int { leaf(n) + leaf(1) }
        function outer(n: int) -> int { inner(n) + leaf(1) }
        function invoke(f: (int) -> int, n: int) -> int { f(n) }
        function main() -> int {
            let first = outer(3);
            let values = [1, 2].map((n: int) -> int { outer(n) });
            let second = invoke(outer, 5);
            first + values[0] + values[1] + second + leaf(23)
        }
    "#;
    // Vary the boundary so some transitions stay in compact dispatch while
    // others yield before entering a callee or after restoring its caller.
    // Native callbacks and indirect calls also reuse earlier frame depths.
    for interval in [1, 2, 3, 5, 11] {
        let program = compile_source(source);
        let entry = program.function_index("user.main").unwrap();
        let flag = Arc::new(AtomicBool::new(true));
        let mut vm = BexVm::from_program(program, Arc::clone(&flag)).unwrap();
        vm.early_yield = bex_vm_types::EarlyYieldCheck::with_interval(flag, interval);
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        let mut completed = false;
        let mut yields = 0;
        for _ in 0..200 {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => yields += 1,
                VmExecState::Complete(value) => {
                    assert_eq!(value.as_int(), Some(42), "interval {interval}");
                    completed = true;
                    break;
                }
                other => panic!("unexpected state at interval {interval}: {other:?}"),
            }
        }
        assert!(
            completed,
            "resumption stopped progressing at interval {interval}"
        );
        assert!(yields > 0, "interval {interval} did not exercise a handoff");
    }
}
