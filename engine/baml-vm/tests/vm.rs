//! VM integ tests.
//!
//! These tests need the compiler crate to go from source to bytecode, that's
//! why they're not placed in the source vm module.

use baml_compiler::ast;
use baml_vm::{Bytecode, Frame, Function, FunctionKind, Instruction, Object, Value, Vm};

/// Helper struct for testing VM execution.
struct Program {
    source: &'static str,
    function: &'static str,
    expected: Value,
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
    let (objects, globals) = baml_compiler::compile(ast)?;

    // Find the target function index by name
    let target_function_index = objects
        .iter()
        .enumerate()
        .find_map(|(i, obj)| match obj {
            Object::Function(f) if f.name == input.function => Some(i),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("function '{}' not found", input.function))?;

    // Create and run the VM.
    // TODO: The VM needs to boostrap itself. Add some function in the VM
    // that does that.
    let mut vm = Vm {
        frames: vec![],
        stack: vec![Value::Object(target_function_index)],
        objects,
        globals,
    };

    vm.frames.push(Frame {
        function: target_function_index,
        instruction_ptr: 0,
        locals_offset: 0,
    });

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
    expected: Value,
}

/// Helper function for VM execution with direct bytecode.
fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

/// Helper function for VM execution with direct bytecode and custom inspection.
fn assert_vm_executes_bytecode_with_inspection(
    input: BytecodeProgram,
    inspect: impl FnOnce(&Vm, Value) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Create function from bytecode
    let function = Function {
        name: "test_fn".to_string(),
        arity: input.arity,
        bytecode: Bytecode {
            source_lines: vec![1; input.instructions.len()],
            instructions: input.instructions,
            constants: input.constants,
        },
        kind: FunctionKind::Exec,
        local_var_names: {
            let mut names = Vec::with_capacity(input.arity + 1);
            names.push("<fn test_fn>".to_string());
            names.resize_with(names.capacity(), String::new);
            names
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
        expected: Value::Int(2),
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
        expected: Value::Int(1),
    })
}

// TODO: Parser is kinda broken and won't parse this:
//
// fn run_if() -> int {
//     let b = true;
//     if b { 1 } else { 2 }
// }
//
// Figure out how to make these tests independent of function calls.
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
        expected: Value::Int(1),
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
        expected: Value::Int(2),
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
            expected: Value::Object(1),
        },
        |vm| {
            let Object::Array(array) = &vm.objects[1] else {
                panic!("expected Array, got {:?}", vm.objects[1]);
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
            expected: Value::Object(2),
        },
        |vm| {
            let Object::Instance(instance) = &vm.objects[2] else {
                panic!("expected Instance, got {:?}", vm.objects[2]);
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
            expected: Value::Object(3),
        },
        |vm| {
            let Object::Instance(instance) = &vm.objects[3] else {
                panic!("expected Instance, got {:?}", vm.objects[3]);
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
fn for_loop_simple() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn main() -> int {
                let list = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
                for (i in list) { i + 1 }
                42
            }
        ",
        function: "main",
        expected: Value::Int(42),
    })
}

#[test]
fn nested_for_loop() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
            fn main() -> int {
                let list = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
                for (i in list) {
                    for (j in list) { j }
                    i
                }
                42
            }
        ",
        function: "main",
        expected: Value::Int(42),
    })
}

#[test]
fn for_loop_with_expressions() -> anyhow::Result<()> {
    // Test that arrays with expressions work properly
    assert_vm_executes(Program {
        source: "
            fn three(x: int) -> int {
                3
            }

            fn main() -> int {
                let list = [three(1), three(2), three(3)];
                for (i in list) { i }
                42
            }
        ",
        function: "main",
        expected: Value::Int(42),
    })
}

#[test]
fn function_returning_string() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: "
                fn main() -> string {
                    \"hello\"
                }
            ",
            function: "main",
            expected: Value::Object(0),
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
            expected: Value::Object(0), // "Hello" should be the first string object
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
        expected: Value::Int(1),
    })
}

#[test]
fn create_iterator_instruction() -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(
        BytecodeProgram {
            arity: 0,
            constants: vec![Value::Int(1), Value::Int(2), Value::Int(3)],
            instructions: vec![
                Instruction::LoadConst(0),   // Load 1
                Instruction::LoadConst(1),   // Load 2
                Instruction::LoadConst(2),   // Load 3
                Instruction::AllocArray(3),  // Create array [1, 2, 3]
                Instruction::CreateIterator, // Create iterator
                Instruction::Return,         // Return the iterator object
            ],
            expected: Value::Object(2), // Should return iterator object at index 2
        },
        |vm, result| {
            let Value::Object(index) = result else {
                panic!("expected Object, got {result:?}");
            };

            let Object::Iterator { iterable, index } = &vm.objects[index] else {
                panic!("expected Iterator, got {:?}", vm.objects[index]);
            };

            assert_eq!(*index, 0); // Should start at index 0
            assert_eq!(*iterable, 1); // Array should be at index 1 (after function at index 0)

            Ok(())
        },
    )
}

#[test]
fn iter_next_instruction() -> anyhow::Result<()> {
    assert_vm_executes_bytecode(BytecodeProgram {
        arity: 0,
        constants: vec![Value::Int(10), Value::Int(20), Value::Int(30)],
        instructions: vec![
            Instruction::LoadConst(0),   // Load 10
            Instruction::LoadConst(1),   // Load 20
            Instruction::LoadConst(2),   // Load 30
            Instruction::AllocArray(3),  // Create array [10, 20, 30]
            Instruction::CreateIterator, // Create iterator
            Instruction::IterNext,       // Get first element
            Instruction::Pop,            // Remove has_next boolean
            Instruction::Return,         // Return the element
        ],
        expected: Value::Int(10), // Should return first element
    })
}
