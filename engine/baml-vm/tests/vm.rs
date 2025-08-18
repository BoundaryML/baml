//! VM integ tests.
//!
//! These tests need the compiler crate to go from source to bytecode, that's
//! why they're not placed in the source vm module.

use baml_compiler::test::ast;
use baml_vm::{
    BamlVmProgram, Bytecode, Frame, Function, FunctionKind, Instruction, Object, Value, Vm,
    VmExecState,
};

/// Helper struct for testing VM execution.
struct Program {
    source: &'static str,
    function: &'static str,
    expected: VmExecState,
}

/// Unified helper function for VM execution with optional inspection.
fn assert_vm_executes(input: Program) -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(input, |_vm| Ok(()))
}

/// Helper function for VM execution with custom inspection.
fn assert_vm_executes_with_inspection(
    input: Program,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let ast = ast(input.source)?;
    let BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
    } = baml_compiler::compile(&ast)?;

    // eprintln!("objects: {objects:#?}");
    // eprintln!("globals: {globals:#?}");
    // eprintln!("resolved_function_names: {resolved_function_names:#?}");

    // Find the target function index by name
    let (target_function_index, _) = resolved_function_names[input.function];

    // Create and run the VM.
    // TODO: The VM needs to boostrap itself. Add some function in the VM
    // that does that.
    let mut vm = Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: 0,
        }],
        stack: vec![Value::Object(target_function_index)],
        runtime_allocs_offset: objects.len(),
        objects,
        globals,
    };

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for function '{}'",
        input.function
    );

    // Run custom inspection
    inspect(&vm)?;

    Ok(())
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
    let globals = vec![Value::Object(0)];

    // Create and run the VM
    let mut vm = Vm {
        frames: vec![Frame {
            function: 0,
            instruction_ptr: 0,
            locals_offset: 0,
        }],
        stack: vec![Value::Object(0)],
        runtime_allocs_offset: objects.len(),
        objects,
        globals,
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
                if b { 1 } else { 2 }
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
                if b { 1 } else { 2 }
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
                if a {
                    1
                } else if b {
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
            expected: VmExecState::Complete(Value::Object(3)),
        },
        |vm| {
            dbg!(&vm.objects);

            let Object::Array(array) = &vm.objects[3] else {
                panic!("expected Array, got {:?}", vm.objects[3]);
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
            expected: VmExecState::Complete(Value::Object(4)),
        },
        |vm| {
            let Object::Instance(instance) = &vm.objects[4] else {
                panic!("expected Instance, got {:?}", vm.objects[4]);
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
                }

                fn default_point() -> Point {
                    Point { x: 0, y: 0, z: 0 }
                }

                fn main() -> Point {
                    let p = Point { x: 1, y: 2, ..default_point() };
                    p
                }
            ",
            function: "main",
            expected: VmExecState::Complete(Value::Object(5)),
        },
        |vm| {
            let Object::Instance(instance) = &vm.objects[5] else {
                panic!("expected Instance, got {:?}", vm.objects[5]);
            };

            assert_eq!(
                instance.fields,
                &[Value::Int(1), Value::Int(2), Value::Int(0)]
            );

            Ok(())
        },
    )
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
            expected: VmExecState::Complete(Value::Object(0)),
        },
        |vm| {
            let Object::String(string) = &vm.objects[0] else {
                panic!("expected String, got {:?}", vm.objects[0]);
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
            expected: VmExecState::Complete(Value::Object(0)), // "Hello" should be the first string object
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
                let mut y = 3;
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
            fn MutableInArg(mut x: int) -> int {
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
                let mut x = 1;
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
                let mut x = 1;
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
                let mut x = 1;
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
                let mut x = 10;
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
                let mut x = 10;
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
                let mut x = 10;
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
                let mut x = 10;
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
                let mut x = 10;
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
fn while_loop() -> anyhow::Result<()> {
    // NOTE: there's no way to make a safeguard since there's no "return", and we shouldn't rely on
    // "break" to keep the test as isolated as possible.
    // Maybe we should "time-out" the VM? (we know how many jumps it should take...)
    const SOURCE: &str = r#"
        fn GCD(mut a: int, mut b: int) -> int {

            while a != b {

               if a > b {
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
            fn Factorial(mut limit: int) -> int {
                let mut result = 1;

                while true {
                    if limit == 0 {
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
                let mut a = 5;
                while true {
                    while true {
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
            fn Factorial(mut limit: int) -> int {
                let mut result = 1;

                // used to make the loop break without relying on `break` implementation.
                let mut should_continue = true;
                while should_continue {
                    result = result * limit;
                    limit = limit - 1;

                    if limit != 0 {
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
                let mut execute = true;
                while execute {
                    while false {
                        continue;
                    }
                    if false {
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
        fn Fib(mut n: int) -> int {

            let mut a = 0;
            let mut b = 1;

            while n > 0 {
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
                let mut result = 0;

                for x in xs {
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
                let mut result = 0;

                for x in xs {
                    if x > 10 {
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
                let mut result = 0;

                for x in xs {
                    if x > 10 {
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

                let mut result =  0;

                for a in arr_a {
                    for b in arr_b {
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

#[cfg(test)]
mod c_for_loops {

    use super::*;

    #[test]
    fn sum_to_ten() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn SumToTen() -> int {
                    let mut s = 0;

                    for (let mut i = 1; i <= 10; i += 1) {
                        s += i;
                    }

                    s
                }
                "#,
            function: "SumToTen",
            expected: VmExecState::Complete(Value::Int(55)),
        })
    }

    #[test]
    fn after_with_break_continue() -> anyhow::Result<()> {
        assert_vm_executes(Program {
            source: r#"
                fn SumToTen() -> int {
                    let mut s = 0;

                    for (let mut i = 0; ; s += i) {
                        i += 1;
                        if i > 10 {
                            let x = 0; // this tests that popping is correct.
                            break;
                        }
                        if i == 5 {
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
                    let mut s = 0;

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
                    let mut s = 0;

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
                   if x == 42 { return 1; }
                   
                   x + 5
                }

                fn main() -> int {
                    EarlyReturn(42)
                }
                "#,
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

                   if a == 0 { return 0; }
                   
                   {
                      let b = 1;
                      if a != b {
                         return 0;
                      }
                   }
                   
                   {
                      let c = 2;
                      let b = 3;
                      while b != c {
                         if true {
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
