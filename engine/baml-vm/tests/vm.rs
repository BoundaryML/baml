//! VM integ tests.
//!
//! These tests need the compiler crate to go from source to bytecode, that's
//! why they're not placed in the source vm module.

use baml_compiler::test::ast;
use baml_vm::{
    BamlVmProgram, Bytecode, EvalStack, Frame, Function, FunctionKind, GlobalPool, Instruction,
    Object, ObjectIndex, ObjectPool, RuntimeError, StackIndex, Value, Vm, VmError, VmExecState,
};

/// Helper struct for testing VM execution.
struct ProgramInput<Expect> {
    source: &'static str,
    function: &'static str,
    expected: Expect,
}

type Program = ProgramInput<VmExecState>;
type FailingProgram = ProgramInput<VmError>;

/// Unified helper function for VM execution with optional inspection.
fn assert_vm_executes(input: Program) -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(input, |_vm| Ok(()))
}

/// Helper function for VM execution with custom inspection.
fn assert_vm_executes_with_inspection(
    input: Program,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, result) = setup_and_exec_program(input.source, input.function)?;
    let result = result?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for function '{}'",
        input.function
    );

    // Run custom inspection
    inspect(&vm)?;

    Ok(())
}

fn assert_vm_fails(input: FailingProgram) -> anyhow::Result<()> {
    let (_, result) = setup_and_exec_program(input.source, input.function)?;

    assert_eq!(
        result,
        Err(input.expected),
        "VM execution result mismatch for function '{}'",
        input.function
    );

    Ok(())
}

fn setup_and_exec_program(
    source: &'static str,
    function: &str,
) -> Result<(Vm, Result<VmExecState, VmError>), anyhow::Error> {
    let ast = ast(source)?;
    let BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
        resolved_enums_names,
        resolved_class_names,
    } = baml_compiler::compile(&ast)?;
    let (target_function_index, _) = resolved_function_names[function];
    let mut vm = Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![Value::Object(target_function_index)]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects,
        globals,
    };
    let result = vm.exec();
    Ok((vm, result))
}

/// Helper struct for testing VM execution with direct bytecode.
struct BytecodeProgram {
    arity: usize,
    instructions: Vec<Instruction>,
    constants: Vec<Value>,
    expected: VmExecState,
}

/// Helper function for VM execution with direct bytecode.
fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

/// Helper function for VM execution with direct bytecode and custom inspection.
fn assert_vm_executes_bytecode_with_inspection(
    input: BytecodeProgram,
    inspect: impl FnOnce(&Vm, VmExecState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Create function from bytecode
    let function = Function {
        name: "test_fn".to_string(),
        arity: input.arity,
        bytecode: Bytecode {
            source_lines: vec![1; input.instructions.len()],
            scopes: vec![0; input.instructions.len()],
            instructions: input.instructions,
            constants: input.constants,
        },
        kind: FunctionKind::Exec,
        locals_in_scope: {
            let mut names = Vec::with_capacity(input.arity + 1);
            names.push("<fn test_fn>".to_string());
            names.resize_with(names.capacity(), String::new);
            vec![names]
        },
    };

    let objects = vec![Object::Function(function)];
    let globals = vec![Value::Object(ObjectIndex::from_raw(0))];

    // Create and run the VM
    let mut vm = Vm {
        frames: vec![Frame {
            function: ObjectIndex::from_raw(0),
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![Value::Object(ObjectIndex::from_raw(0))]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects: ObjectPool::from_vec(objects),
        globals: GlobalPool::from_vec(globals),
    };

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for bytecode test",
    );

    // Run custom inspection
    inspect(&vm, result)?;

    Ok(())
}

#[test]
fn return_function_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn one() -> int {
                1
            }

            fn main() -> int {
                one()
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn function_call_without_parameters() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn two() -> int {
                let v = 2;
                v
            }

            fn main() -> int {
                let v = two();
                v
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn function_call_with_parameters() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn one_of(a: int, b: int) -> int {
                a
            }

            fn main() -> int {
                let v = one_of(1, 2);
                v
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

// TODO: Figure out how to make these tests independent of function calls.
#[test]
fn exec_if_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn run_if(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }

            fn main() -> int {
                let a = run_if(true);
                a
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn exec_else_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn run_if(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }

            fn main() -> int {
                let a = run_if(false);
                a
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn exec_else_if_branch() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn run_if(a: bool, b: bool) -> int {
                if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                }
            }

            fn main() -> int {
                let a = run_if(false, true);
                a
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

// TODO: Notice how we use the inspection function to make sure the array
// contents are what we expect. When we figure out a pattern on these types
// of tests we can abstract it away and not have inspect the entire VM, but
// keep it simple for now. Claude 4 is very good at figuring out these
// abstractions once it sees the patterns repeated.
#[test]
fn array_constructor() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: "
                fn main() -> int[] {
                    let a = [1, 2, 3];
                    a
                }
            ",
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(6))),
        },
        |vm| {
            dbg!(&vm.objects);

            let Object::Array(array) = &vm.objects[ObjectIndex::from_raw(6)] else {
                panic!(
                    "expected Array, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(6)]
                );
            };

            assert_eq!(array, &[Value::Int(1), Value::Int(2), Value::Int(3)]);

            Ok(())
        },
    )
}

// TODO: Read comment above.
#[test]
fn class_constructor() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: "
                class Point {
                    x int
                    y int
                }

                fn main() -> Point {
                    let p = Point { x: 1, y: 2 };
                    p
                }
            ",
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(7))),
        },
        |vm| {
            let Object::Instance(instance) = &vm.objects[ObjectIndex::from_raw(7)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(7)]
                );
            };

            assert_eq!(instance.fields, &[Value::Int(1), Value::Int(2)]);

            Ok(())
        },
    )
}

// TODO: Read comment above.
#[test]
fn class_constructor_with_spread_operator() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: "
                class Point {
                    x int
                    y int
                    z int
                    w int
                }

                fn default_point() -> Point {
                    Point { x: 0, y: 0, z: 0, w: 0 }
                }

                fn main() -> Point {
                    let p = Point { x: 1, y: 2, ..default_point() };
                    p
                }
            ",
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(8))),
        },
        |vm| {
            let Object::Instance(instance) = &vm.objects[ObjectIndex::from_raw(8)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(8)]
                );
            };

            assert_eq!(
                instance.fields,
                &[Value::Int(1), Value::Int(2), Value::Int(0), Value::Int(0)],
            );

            Ok(())
        },
    )
}

#[test]
fn class_constructor_with_spread_operator_does_not_break_locals() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
                class Point {
                    x int
                    y int
                    z int
                    w int
                }

                fn default_point() -> Point {
                    Point { x: 0, y: 0, z: 0, w: 0 }
                }

                fn main() -> int {
                    let p = Point { x: 1, y: 2, ..default_point() };
                    let x = 0;
                    x
                }
            ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(0)),
    })
}

#[test]
fn function_returning_string() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                fn main() -> string {
                    "hello"
                }
            "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(0))),
        },
        |vm| {
            let Object::String(string) = &vm.objects[ObjectIndex::from_raw(0)] else {
                panic!(
                    "expected String, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(0)]
                );
            };

            assert_eq!(string, "hello");

            Ok(())
        },
    )
}

#[test]
fn multiple_strings() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                fn get_greeting() -> string {
                    "Hello"
                }

                fn main() -> string {
                    let greeting = get_greeting();
                    let name = "World";
                    greeting
                }
            "#,
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(0))), // "Hello" should be the first string object
        },
        |vm| {
            // Check that we have the expected strings in the objects pool
            let strings: Vec<&str> = vm
                .objects
                .iter()
                .filter_map(|obj| match obj {
                    Object::String(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect();

            assert!(strings.contains(&"Hello"));
            assert!(strings.contains(&"World"));

            Ok(())
        },
    )
}

#[test]
fn block_expr() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn main() -> int {
                let a = {
                    let b = 1;
                    b
                };

                a
            }
        ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn exec_declare_mutable_in_function() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let y = 3;
                y = 5;
                y
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn exec_mutable_in_arg() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn MutableInArg(x: int) -> int {
                x = 3;
                x
            }

            fn main() -> int {
                let r = MutableInArg(42);
                r
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn basic_add() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                1 + 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn basic_sub() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                1 - 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(-1)),
    })
}

#[test]
fn basic_mul() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                1 * 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn basic_div() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                10 / 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn basic_mod() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                10 % 3
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn basic_bit_and() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                10 & 3
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn basic_bit_or() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                10 | 3
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(11)),
    })
}

#[test]
fn basic_bit_xor() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                10 ^ 3
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(9)),
    })
}

#[test]
fn basic_bit_shift_left() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                10 << 3
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(80)),
    })
}

#[test]
fn basic_bit_shift_right() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                10 >> 3
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn unary_neg() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                -1
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(-1)),
    })
}

#[test]
fn unary_not() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                !true
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_eq() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                1 == 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_not_eq() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                1 != 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn basic_gt() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                1 > 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_gt_eq() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                1 >= 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_lt() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                1 < 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn basic_lt_eq() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                1 <= 2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn basic_and() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                true && false
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn basic_or() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> bool {
                true || false
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn basic_assign_add() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let x = 1;
                x += 2;
                x
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn basic_assign_sub() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let x = 1;
                x -= 2;
                x
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(-1)),
    })
}

#[test]
fn basic_assign_mul() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let x = 1;
                x *= 2;
                x
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn basic_assign_div() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let x = 10;
                x /= 2;
                x
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn basic_assign_mod() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let x = 10;
                x %= 3;
                x
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn basic_assign_bit_and() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let x = 10;
                x &= 3;
                x
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn basic_assign_bit_or() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let x = 10;
                x |= 3;
                x
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(11)),
    })
}

#[test]
fn basic_assign_bit_xor() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let x = 10;
                x ^= 3;
                x
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(9)),
    })
}

#[test]
fn builtin_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let arr = [1, 2, 3];
                arr.len()
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn bind_method_call() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn main() -> int {
                let arr = [1, 2, 3];
                let v = arr.len();

                v
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn while_loop() -> anyhow::Result<()> {
    // NOTE: there's no way to make a safeguard since there's no "return", and we shouldn't rely on
    // "break" to keep the test as isolated as possible.
    // Maybe we should "time-out" the VM? (we know how many jumps it should take...)
    const SOURCE: &str = r#"
        fn GCD(a: int, b: int) -> int {

            while (a != b) {

               if (a > b) {
                   a = a - b;
               } else {
                   b = b - a;
               }

            }

            a
        }

        fn main() -> int {
            GCD(21, 15)
        }
    "#;

    assert_vm_executes(Program {
        source: SOURCE,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn break_factorial() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn Factorial(limit: int) -> int {
                let result = 1;

                while (true) {
                    if (limit == 0) {
                        break;
                    }
                    result = result * limit;
                    limit = limit - 1;
                }

                result
            }

            fn main() -> int {
                Factorial(5)
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(120)),
    })
}

#[test]
fn break_nested_loops() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn Nested() -> int {
                let a = 5;
                while (true) {
                    while (true) {
                        a = a + 1;
                        break;
                    }
                    a = a + 1;
                    break;
                }
                a
            }

            fn main() -> int {
                Nested()
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(7)),
    })
}

#[test]
fn continue_factorial() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn Factorial(limit: int) -> int {
                let result = 1;

                // used to make the loop break without relying on `break` implementation.
                let should_continue = true;
                while (should_continue) {
                    result = result * limit;
                    limit = limit - 1;

                    if (limit != 0) {
                        continue;
                    } else {
                        should_continue = false;
                    }
                }

                result
            }

            fn main() -> int {
                Factorial(5)
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(120)),
    })
}

#[test]
fn continue_nested() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn ContinueNested() -> int {
                let execute = true;
                while (execute) {
                    while (false) {
                        continue;
                    }
                    if (false) {
                        continue;
                    }
                    execute = false;
                }
                5
            }

            fn main() -> int {
                ContinueNested()
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn while_with_scope() -> anyhow::Result<()> {
    const SOURCE: &str = r#"
        fn Fib(n: int) -> int {

            let a = 0;
            let b = 1;

            while (n > 0) {
                n -= 1;
                let t = a + b;
                b = a;
                a = t;
            }

            a
        }

        fn main() -> int {
            Fib(5)
        }
    "#;

    assert_vm_executes(Program {
        source: SOURCE,
        function: "main",
        expected: VmExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn for_loop_sum() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn Sum(xs: int[]) -> int {
                let result = 0;

                for (x in xs) {
                    result += x;
                }

                result
            }

            fn main() -> int {
                Sum([1, 2, 3, 4])
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(10)),
    })
}

#[test]
fn for_loop_with_break() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn ForWithBreak(xs: int[]) -> int {
                let result = 0;

                for (x in xs) {
                    if (x > 10) {
                        break;
                    }
                    result += x;
                }

                result
            }

            fn main() -> int {
                ForWithBreak([3, 4, 11, 100])
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(7)),
    })
}

#[test]
fn for_loop_with_continue() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn ForWithContinue(xs: int[]) -> int {
                let result = 0;

                for (x in xs) {
                    if (x > 10) {
                        continue;
                    }
                    result += x;
                }

                result
            }

            fn main() -> int {
                ForWithContinue([5, 20, 6])
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(11)),
    })
}

#[test]
fn for_loop_nested() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            fn NestedFor(arr_a: int[], arr_b: int[]) -> int {

                let result =  0;

                for (a in arr_a) {
                    for (b in arr_b) {
                        result += a * b;
                    }
                }

                result
            }

            fn main() -> int {
                NestedFor([1, 2], [3, 4])
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(21)),
    })
}

// #[test]
// fn basic_method_decl() -> anyhow::Result<()> {
//     assert_vm_executes(Program {
//         source: r#"
//             class Number {
//                 value int
//
//                 function add(self, other: Number) -> Number {
//                     Number { value: self.value + other.value }
//                 }
//             }
//
//             function main() -> int {
//                 let a = Number { value: 1 };
//                 let b = Number { value: 2 };
//                 let n = a.add(b);
//                 n.value
//             }
//         "#,
//         function: "main",
//         expected: VmExecState::Complete(Value::Int(3)),
//     })
// }

#[test]
fn mut_self_method_decl() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Number {
                value int

                function add(self, other: Number) -> bool {
                    self.value += other.value;
                    true
                }
            }

            function main() -> int {
                let a = Number { value: 1 };
                let b = Number { value: 2 };
                a.add(b);
                a.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

#[test]
fn field_assignment_add_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 10 };
                c.value += 5;
                c.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(15)),
    })
}

#[test]
fn field_assignment_sub_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 20 };
                c.value -= 8;
                c.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(12)),
    })
}

#[test]
fn field_assignment_mul_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 7 };
                c.value *= 3;
                c.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(21)),
    })
}

#[test]
fn field_assignment_div_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 24 };
                c.value /= 4;
                c.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(6)),
    })
}

#[test]
fn field_assignment_mod_assign() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }
            function main() -> int {
                let c = Counter { value: 17 };
                c.value %= 5;
                c.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn field_assignment_simple() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Data {
                value int
                active bool
            }
            function main() -> int {
                let d = Data { value: 100, active: true };
                d.value = 42;
                d.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn field_assignment_multiple_ops() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Stats {
                score int
            }
            function main() -> int {
                let s = Stats { score: 10 };
                s.score += 5;   // 15
                s.score *= 2;   // 30
                s.score -= 10;  // 20
                s.score /= 4;   // 5
                s.score
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(5)),
    })
}

#[test]
fn test_nested_object_construction() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                x int
                y int
            }
            class Outer {
                inner Inner
                value int
            }
            function main() -> int {
                let o = Outer {
                    inner: Inner { x: 10, y: 20 },
                    value: 30
                };
                // Test that construction worked by accessing a simple field
                o.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(30)),
    })
}

#[test]
fn test_nested_object_construction_with_field_access() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                x int
                y int
            }
            class Outer {
                inner Inner
                value int
            }
            function main() -> int {
                let o = Outer {
                    inner: Inner { x: 10, y: 20 },
                    value: 30
                };
                // Test nested field access after nested construction
                o.inner.y
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(20)),
    })
}

#[test]
fn test_nested_field_read_with_nested_construction() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> int {
                let o = Outer { inner: Inner { value: 42 } };
                o.inner.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn test_nested_field_read() -> anyhow::Result<()> {
    // Test nested field read without nested construction
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> int {
                let i = Inner { value: 42 };
                let o = Outer { inner: i };
                o.inner.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn test_constructor_with_preceding_variables() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class MyClass {
                x int
                y int
            }
            function main() -> int {
                let a = 10;
                let b = 20;
                let c = 30;
                let obj = MyClass { x: 100, y: 200 };
                obj.x + obj.y + a + b + c
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(360)), // 100 + 200 + 10 + 20 + 30
    })
}

#[test]
fn test_nested_constructor_with_preceding_variables() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                val int
            }
            class Outer {
                inner Inner
                x int
            }
            function main() -> int {
                let a = 5;
                let b = 10;
                let obj = Outer {
                    inner: Inner { val: 100 },
                    x: 50
                };
                obj.inner.val + obj.x + a + b
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(165)), // 100 + 50 + 5 + 10
    })
}

#[test]
fn test_method_call_field_assignment() -> anyhow::Result<()> {
    // Test that we can modify a field of a method's return value
    // Note: BAML has value semantics, so methods return copies not references
    assert_vm_executes(Program {
        source: r#"
            class Counter {
                value int
            }

            class Factory {
                counter Counter

                function get_counter(self) -> Counter {
                    self.counter
                }
            }

            function main() -> int {
                let f = Factory {
                    counter: Counter { value: 10 }
                };
                // get_counter returns a copy of the counter
                let c = f.get_counter();
                // We can modify the copy
                c.value += 5;
                // Return the modified copy's value
                c.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(15)), // Modified copy
    })
}

#[test]
fn test_array_element_field_assignment() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Item {
                count int
            }

            function main() -> int {
                let items = [
                    Item { count: 10 },
                    Item { count: 20 },
                    Item { count: 30 }
                ];

                // Modify field of array element
                items[1].count += 5;
                items[1].count
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(25)), // 20 + 5
    })
}

#[test]
fn test_array_element_method_field_assignment() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Data {
                value int

                function get_self(self) -> Data {
                    self
                }
            }

            class Container {
                data Data

                function get_data(self) -> Data {
                    self.data
                }
            }

            function main() -> int {
                let containers = [
                    Container { data: Data { value: 10 } },
                    Container { data: Data { value: 20 } },
                    Container { data: Data { value: 30 } }
                ];

                // First test: Can we modify array element's field?
                containers[1].data.value += 5;
                let result1 = containers[1].data.value; // Should be 25

                // Test method call assignment:
                containers[1].get_data().value += 10;
                let result2 = containers[1].data.value; // Should be 35 (25 + 10)

                result2
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(35)), // 20 + 5 + 10
    })
}

#[test]
fn test_method_call_then_array_access_assignment() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Item {
                value int
            }
            class Container {
                data Item[]
                function get_nested(self) -> Item[] {
                    self.data
                }
            }
            function main() -> int {
                let i1 = Item { value: 10 };
                let i2 = Item { value: 20 };
                let i3 = Item { value: 30 };
                let arr = [i1, i2, i3];
                let obj = Container { data: arr };
                obj.get_nested()[1].value += 5;
                obj.data[1].value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(25)),
    })
}

#[test]
fn nested_field_assignment_simple() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> int {
                let i = Inner { value: 10 };
                let o = Outer { inner: i };
                o.inner.value = 42;
                o.inner.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn nested_field_assignment_compound() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> int {
                let i = Inner { value: 10 };
                let o = Outer { inner: i };
                o.inner.value += 32;
                o.inner.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn field_assignment_object_field() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> bool {
                let o = Outer { inner: Inner { value: 10 } };
                o.inner = Inner { value: 20 };
                // For now, test that assignment works, not nested field access
                true
            }"#,
        function: "main",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[cfg(test)]
mod c_for_loops {

    use super::*;

    #[test]
    fn sum_to_ten() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn SumToTen() -> int {
                    let s = 0;

                    for (let i = 1; i <= 10; i += 1) {
                        s += i;
                    }

                    s
                }"#,
            function: "SumToTen",
            expected: VmExecState::Complete(Value::Int(55)),
        })
    }

    #[test]
    fn after_with_break_continue() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn SumToTen() -> int {
                    let s = 0;

                    for (let i = 0; ; s += i) {
                        i += 1;
                        if (i > 10) {
                            let x = 0; // this tests that popping is correct.
                            break;
                        }
                        if (i == 5) {
                            // since `s += i` is in the for loop's after, this 'continue' is
                            // actually irrelevant and the function does the same as SumToTen.
                            // That's the behavior we're looking for.
                            continue;
                        }
                    }

                    s
                }"#,
            function: "SumToTen",
            expected: VmExecState::Complete(Value::Int(55)),
        })
    }

    #[test]
    fn only_cond() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn OnlyCond() -> int {
                    let s = 0;

                    for (; false;) {
                    }

                    s
                }"#,
            function: "OnlyCond",
            expected: VmExecState::Complete(Value::Int(0)),
        })
    }

    #[test]
    fn endless() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn Nothing() -> int {
                    let s = 0;

                    for (;;) {
                        break;
                    }

                    s
                }"#,
            function: "Nothing",
            expected: VmExecState::Complete(Value::Int(0)),
        })
    }
}

#[cfg(test)]
mod return_stmt {

    use super::*;

    #[test]
    fn early_return() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn EarlyReturn(x: int) -> int {
                   if (x == 42) { return 1; }

                   x + 5
                }

                fn main() -> int {
                    EarlyReturn(42)
                }"#,
            function: "main",
            expected: VmExecState::Complete(Value::Int(1)),
        })
    }

    #[test]
    fn with_stack() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn WithStack() -> int {
                   let a = 1;

                   if (a == 0) { return 0; }

                   {
                      let b = 1;
                      if (a != b) {
                         return 0;
                      }
                   }

                   {
                      let c = 2;
                      let b = 3;
                      while (b != c) {
                         if (true) {
                              return 0;
                         }
                      }
                    }

                    7
                }"#,
            function: "WithStack",
            expected: VmExecState::Complete(Value::Int(0)),
        })
    }
}

mod assert_stmt {

    use baml_vm::RuntimeError;

    use super::*;

    #[test]
    fn assert_ok() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn assertOk() -> int {

                    assert 2 + 2 == 4;

                    3
                }"#,
            function: "assertOk",
            expected: VmExecState::Complete(Value::Int(3)),
        })
    }

    #[test]
    fn assert_not_ok() -> anyhow::Result<()> {
        assert_vm_fails(FailingProgram {
            source: r#"
                fn assertNotOk() -> int {
                    assert 3 == 1;

                    2
                }"#,
            function: "assertNotOk",
            expected: RuntimeError::AssertionError.into(),
        })
    }
}

#[cfg(test)]
mod maps {
    use super::*;

    #[test]
    fn create_and_access() -> anyhow::Result<()> {
        let str_index = ObjectIndex::from_raw(0);
        assert_vm_executes_with_inspection(
            Program {
                source: r#"
fn CreateMap() -> map<string, string> {
    { hello "world" }
}
fn UseMap() -> string {
    let map = CreateMap();
    map["hello"]
}"#,
                function: "UseMap",
                expected: VmExecState::Complete(Value::Object(str_index)),
            },
            |vm| {
                assert_eq!(vm.objects[str_index].as_string().unwrap(), "world");
                Ok(())
            },
        )
    }

    #[test]
    fn access_no_key() -> anyhow::Result<()> {
        assert_vm_fails(FailingProgram {
            source: r#"
fn CreateMap() -> map<string, string> {
    { hello "world" }
}

fn UseMapNoKey() -> string {
    let map = CreateMap();
    map["world"]
}"#,
            function: "UseMapNoKey",
            expected: RuntimeError::NoSuchKeyInMap.into(),
        })
    }

    #[test]
    fn contains() -> anyhow::Result<()> {
        let str_index = ObjectIndex::from_raw(0);
        assert_vm_executes_with_inspection(
            Program {
                source: r#"
fn CreateMapJSON() -> map<string, string> {
    {"hello": "world"}
}
fn UseMapContains() -> string {
    let map = CreateMapJSON();
    if (map.contains("hello")) {
        map["hello"]
    } else {
        "hi"
    }
}"#,
                function: "UseMapContains",
                expected: VmExecState::Complete(Value::Object(str_index)),
            },
            |vm| {
                assert_eq!(vm.objects[str_index].as_string().unwrap(), "world");
                Ok(())
            },
        )
    }

    #[test]
    fn modify() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
fn EditMapKey() -> int {
	let map = { hi 123 };

	map["hi"] = 42 - 4;
	map["hi"] += 4;

	map["hi"]

}"#,
            function: "EditMapKey",
            expected: VmExecState::Complete(Value::Int(42)),
        })
    }

    #[test]
    fn len() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
fn Len() -> int {
    let map = {
        hi 123
        it_works 456
    };
    map.len()
}"#,
            function: "Len",
            expected: VmExecState::Complete(Value::Int(2)),
        })
    }
}

#[cfg(test)]
mod enums {
    use super::*;

    #[test]
    fn return_enum_variant() -> anyhow::Result<()> {
        assert_vm_executes_with_inspection(
            Program {
                source: r#"
                    enum Shape {
                        Square
                        Rectangle
                        Circle
                    }

                    fn main() -> Shape {
                        Shape.Rectangle
                    }
                "#,
                function: "main",
                expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(7))),
            },
            |vm| {
                let Object::Variant(variant) = &vm.objects[ObjectIndex::from_raw(7)] else {
                    panic!(
                        "expected Variant, got {:?}",
                        &vm.objects[ObjectIndex::from_raw(7)]
                    );
                };

                assert_eq!(variant.index, 1);

                Ok(())
            },
        )
    }

    #[test]
    fn assign_enum_variant() -> anyhow::Result<()> {
        assert_vm_executes_with_inspection(
            Program {
                source: r#"
                    enum Shape {
                        Square
                        Rectangle
                        Circle
                    }

                    fn main() -> Shape {
                        let s = Shape.Rectangle;
                        s
                    }
                "#,
                function: "main",
                expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(7))),
            },
            |vm| {
                let Object::Variant(variant) = &vm.objects[ObjectIndex::from_raw(7)] else {
                    panic!(
                        "expected Variant, got {:?}",
                        &vm.objects[ObjectIndex::from_raw(7)]
                    );
                };

                assert_eq!(variant.index, 1);

                Ok(())
            },
        )
    }

    #[test]
    fn take_and_return_enum_variant() -> anyhow::Result<()> {
        assert_vm_executes_with_inspection(
            Program {
                source: r#"
                    enum Shape {
                        Square
                        Rectangle
                        Circle
                    }

                    function return_shape(shape: Shape) -> Shape {
                        shape
                    }

                    fn main() -> Shape {
                        return_shape(Shape.Rectangle)
                    }
                "#,
                function: "main",
                expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(8))),
            },
            |vm| {
                let Object::Variant(variant) = &vm.objects[ObjectIndex::from_raw(8)] else {
                    panic!(
                        "expected Variant, got {:?}",
                        &vm.objects[ObjectIndex::from_raw(8)]
                    );
                };

                assert_eq!(variant.index, 1);

                Ok(())
            },
        )
    }
}
