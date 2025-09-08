//! Compiler tests for built-in method calls.

use baml_vm::{GlobalIndex, Instruction};

mod common;
use common::{assert_compiles, Program};

#[test]
fn builtin_method_call() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            fn main() -> int {
                let arr = [1, 2, 3];
                arr.len()
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::LoadConst(2),
                Instruction::AllocArray(3),
                Instruction::LoadGlobal(GlobalIndex::from_raw(3)),
                Instruction::LoadVar(1),
                // call with one argument (self)
                Instruction::Call(1),
                Instruction::Return,
            ],
        )],
    })
}
