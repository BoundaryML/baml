//! The line table must attribute an `Await` opcode to the `await` expression.
//!
//! A thread parks on the `Await` opcode, so debuggers, stack traces and the
//! durable worker's position events read the line of that pc. Pulling the
//! awaited future can inline a virtual local, which installs the span of the
//! statement that defined the future. The emitter restores the terminator's
//! span before it emits the opcode.

use baml_compiler2_emit::OptLevel;
use bex_vm_types::{Instruction, Object, Program, types::Function};

const SOURCE: &str = r#"function slow() -> string {
    "done"
}

function await_a_local() -> string {
    let pending = spawn { slow() };
    let filler = 1;
    let more = filler + 1;
    let result = await pending;
    result
}

function await_any_of_locals() -> int {
    let first = spawn { slow() };
    let second = spawn { slow() };
    let filler = 1;
    let winner = baml.future.__await_any([first, second]);
    winner + filler
}
"#;

fn line_of(needle: &str) -> usize {
    SOURCE
        .lines()
        .position(|line| line.contains(needle))
        .map(|index| index + 1)
        .unwrap_or_else(|| panic!("`{needle}` is in the test source"))
}

fn function<'a>(program: &'a Program, name: &str) -> &'a Function {
    program
        .objects
        .iter()
        .find_map(|object| match object {
            Object::Function(function)
                if function.name == name || function.name.ends_with(&format!(".{name}")) =>
            {
                Some(&**function)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("function `{name}` is in the compiled program"))
}

fn lines_of(function: &Function, wanted: fn(&Instruction) -> bool) -> Vec<usize> {
    function
        .bytecode
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, instruction)| wanted(instruction))
        .map(|(pc, _)| function.bytecode.source_line_for_pc(pc))
        .collect()
}

#[test]
fn an_await_opcode_maps_to_the_await_expression() {
    for opt in [OptLevel::Zero, OptLevel::One] {
        let program = baml_db::testing::compile_source_with_opt(SOURCE, opt);
        let lines = lines_of(function(&program, "await_a_local"), |instruction| {
            matches!(instruction, Instruction::Await)
        });
        assert_eq!(
            lines,
            vec![line_of("let result = await pending;")],
            "line of the Await opcode at {opt:?}"
        );
    }
}

#[test]
fn an_await_any_opcode_maps_to_the_await_any_call() {
    for opt in [OptLevel::Zero, OptLevel::One] {
        let program = baml_db::testing::compile_source_with_opt(SOURCE, opt);
        let lines = lines_of(function(&program, "await_any_of_locals"), |instruction| {
            matches!(instruction, Instruction::AwaitAny)
        });
        assert_eq!(
            lines,
            vec![line_of("baml.future.__await_any")],
            "line of the AwaitAny opcode at {opt:?}"
        );
    }
}
