//! Compiler tests for watch functionality.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use bex_vm_types::bytecode::CmpOp;

#[test]
fn watch_primitive() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function primitive() -> int {
                watch let value = 0;

                value = 1;

                value
            }
        ",
        expected: vec![(
            "primitive",
            vec![
                // Initialize locals with null (only "value" needs a slot now, _0 is ReturnPhi)
                // Initialize watched variable
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("value".to_string()),
                // Register watch (only once, at initialization)
                Instruction::LoadConst(Value::string("value")), // channel "value"
                Instruction::LoadConst(Value::Null),            // filter null
                Instruction::Watch(1),
                // Assignment: value = 1
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreVar("value".to_string()),
                // Return value - _0 is ReturnPhi, so value stays on stack through Unwatch
                Instruction::LoadVar("value".to_string()),
                // Unwatch on scope exit (stack-neutral, doesn't disturb TOS)
                Instruction::Unwatch(1),
                // No StoreVar/LoadVar for _0 - value already on stack
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// VizEnter/VizExit Tests
// ============================================================================

#[test]
fn viz_header_before_if_emits_viz_enter_exit() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function header_before_if() -> int {
                //# MyHeader
                if (true) {
                    1
                } else {
                    2
                }
            }
        "#,
        expected: vec![(
            "header_before_if",
            vec![
                // No result temp init - _0 is ReturnPhi (VizExit is stack-neutral)
                Instruction::NotifyBlock(0), // //# MyHeader
                Instruction::VizEnter(0),    // VizEnter because header precedes if
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)), // else branch - stays on stack
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)), // then branch - stays on stack
                Instruction::VizExit(0),               // VizExit at join (stack-neutral)
                Instruction::Return,                   // value already on stack
            ],
        )],
    })
}

#[test]
fn viz_header_before_while_emits_viz_enter_exit() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function header_before_while() -> int {
                let x = 0;
                //# LoopHeader
                while (x < 5) {
                    x = x + 1;
                }
                x
            }
        "#,
        expected: vec![(
            "header_before_while",
            vec![
                Instruction::LoadConst(Value::Int(0)), // x = 0
                Instruction::StoreVar("x".to_string()),
                Instruction::NotifyBlock(0), // //# LoopHeader
                Instruction::VizEnter(0),    // VizEnter because header precedes while
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(4),
                Instruction::VizExit(0), // VizExit at loop exit
                Instruction::LoadVar("x".to_string()),
                Instruction::Return,
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(bex_vm_types::bytecode::BinOp::Add),
                Instruction::StoreVar("x".to_string()),
                Instruction::Jump(-12),
            ],
        )],
    })
}

#[test]
fn viz_standalone_header_no_viz_enter_exit() -> anyhow::Result<()> {
    // Note: `let x = 5; x` is optimized to just returning 5 directly
    assert_compiles(Program {
        source: r#"
            function standalone_header() -> int {
                //# JustAHeader
                let x = 5;
                x
            }
        "#,
        expected: vec![(
            "standalone_header",
            vec![
                Instruction::NotifyBlock(0), // //# JustAHeader - only NotifyBlock, no VizEnter
                Instruction::LoadConst(Value::Int(5)), // constant-propagated x
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn viz_multiple_headers_only_one_before_if() -> anyhow::Result<()> {
    // Note: x=1 is constant-propagated, so no StoreVar for x appears
    assert_compiles(Program {
        source: r#"
            function multiple_headers() -> int {
                //# FirstHeader
                let x = 1;
                //# SecondHeader
                if (x > 0) {
                    2
                } else {
                    3
                }
            }
        "#,
        expected: vec![(
            "multiple_headers",
            vec![
                // No result temp - _0 is ReturnPhi (VizExit is stack-neutral)
                Instruction::NotifyBlock(0), // //# FirstHeader - no VizEnter (not before control flow)
                Instruction::NotifyBlock(1), // //# SecondHeader
                Instruction::VizEnter(0),    // VizEnter because this header precedes if
                Instruction::LoadConst(Value::Int(1)), // x (inlined constant)
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(3)), // else branch - stays on stack
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(2)), // then branch - stays on stack
                Instruction::VizExit(0),               // VizExit at join (stack-neutral)
                Instruction::Return,                   // value already on stack
            ],
        )],
    })
}

#[test]
fn viz_if_without_header_no_viz() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function if_no_header() -> int {
                if (true) {
                    1
                } else {
                    2
                }
            }
        "#,
        expected: vec![(
            "if_no_header",
            vec![
                // No VizEnter because no header before if
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                Instruction::LoadConst(Value::Int(2)), // else branch
                Instruction::Jump(2),
                Instruction::LoadConst(Value::Int(1)), // then branch
                // No VizExit
                Instruction::Return,
            ],
        )],
    })
}
