//! Compiler tests for for-in loops.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm_types::bytecode::{BinOp, CmpOp};

// ============================================================================
// For-in loops
// ============================================================================

#[test]
fn for_loop_sum() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Sum(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    result += x;
                }

                result
            }
            "#,
        expected: vec![(
            "Sum",
            // MIR-based codegen with local pre-allocation:
            // Locals: _0 (return), xs (param), result, _len, _i, x
            // Note: _iter is eliminated by copy propagation (uses xs directly)
            vec![
                // Pre-allocate locals (4: result, _len, _i, x)
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize result = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("result".to_string()),
                // _len = baml.Array.length(xs) - using param directly
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("xs".to_string()),
                Instruction::Call(1),
                Instruction::StoreVar("_len".to_string()),
                // _i = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("_i".to_string()),
                // Loop condition: _i < _len
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadVar("_len".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                // Loop exit: load result and return
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
                // Loop body: x = xs[_i] - using param directly
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadArrayElement,
                Instruction::StoreVar("x".to_string()),
                // _i = _i + 1
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i".to_string()),
                // result = result + x
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                // Jump back to loop condition
                Instruction::Jump(-19),
            ],
        )],
    })
}

#[test]
fn for_with_break() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function ForWithBreak(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    if (x > 10) {
                        break;
                    }
                    result += x;
                }

                result
            }
            "#,
        expected: vec![(
            "ForWithBreak",
            // MIR-based codegen with local pre-allocation
            // Note: _iter is eliminated by copy propagation (uses xs directly)
            vec![
                // Pre-allocate locals (4: result, _len, _i, x)
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize result = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("result".to_string()),
                // _len = baml.Array.length(xs) - using param directly
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("xs".to_string()),
                Instruction::Call(1),
                Instruction::StoreVar("_len".to_string()),
                // _i = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("_i".to_string()),
                // Loop condition: _i < _len
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadVar("_len".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(19),
                // Loop body: x = xs[_i] - using param directly
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadArrayElement,
                Instruction::StoreVar("x".to_string()),
                // _i = _i + 1
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i".to_string()),
                // if (x > 10)
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::PopJumpIfFalse(2),
                // break - jump to loop exit
                Instruction::Jump(6),
                // result += x
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                // Jump back to loop condition
                Instruction::Jump(-21),
                // Loop exit: load result and return
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn for_with_continue() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function ForWithContinue(xs: int[]) -> int {
                let result = 0;

                for (let x in xs) {
                    if (x > 10) {
                        continue;
                    }
                    result += x;
                }

                result
            }
            "#,
        expected: vec![(
            "ForWithContinue",
            // MIR-based codegen with local pre-allocation
            // Note: _iter is eliminated by copy propagation (uses xs directly)
            vec![
                // Pre-allocate locals (4: result, _len, _i, x)
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize result = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("result".to_string()),
                // _len = baml.Array.length(xs) - using param directly
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("xs".to_string()),
                Instruction::Call(1),
                Instruction::StoreVar("_len".to_string()),
                // _i = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("_i".to_string()),
                // Loop condition: _i < _len
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadVar("_len".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                // Loop exit: load result and return
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
                // Loop body: x = xs[_i] - using param directly
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadArrayElement,
                Instruction::StoreVar("x".to_string()),
                // _i = _i + 1
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i".to_string()),
                // if (x > 10)
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::PopJumpIfFalse(2),
                // continue - jump threading: direct to loop condition
                Instruction::Jump(-19),
                // result += x
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                // Jump back to loop condition
                Instruction::Jump(-24),
                // Note: unreachable continue fallthrough eliminated by jump threading
            ],
        )],
    })
}

#[test]
fn for_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function NestedFor(as: int[], bs: int[]) -> int {

                let result = 0;

                for (let a in as) {
                    for (let b in bs) {
                        result += a * b;
                    }
                }

                result
            }
            "#,
        expected: vec![(
            "NestedFor",
            // MIR-based codegen with local pre-allocation
            // Locals: _0, as, bs, result, _len, _i, a, _len1, _i1, b
            // Note: _iter and _iter1 are eliminated by copy propagation (use params directly)
            vec![
                // Pre-allocate locals (7: result, _len, _i, a, _len1, _i1, b)
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                Instruction::LoadConst(Value::Null),
                // Initialize result = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("result".to_string()),
                // _len = baml.Array.length(as) - using param directly
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("as".to_string()),
                Instruction::Call(1),
                Instruction::StoreVar("_len".to_string()),
                // _i = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("_i".to_string()),
                // Outer loop condition: _i < _len
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadVar("_len".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(3),
                // Outer loop exit: load result and return
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
                // Outer loop body: a = as[_i] - using param directly
                Instruction::LoadVar("as".to_string()),
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadArrayElement,
                Instruction::StoreVar("a".to_string()),
                // _i = _i + 1
                Instruction::LoadVar("_i".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i".to_string()),
                // _len1 = baml.Array.length(bs) - using param directly
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("bs".to_string()),
                Instruction::Call(1),
                Instruction::StoreVar("_len1".to_string()),
                // _i1 = 0
                Instruction::LoadConst(Value::Int(0)),
                Instruction::StoreVar("_i1".to_string()),
                // Inner loop condition: _i1 < _len1
                Instruction::LoadVar("_i1".to_string()),
                Instruction::LoadVar("_len1".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::PopJumpIfFalse(2),
                Instruction::Jump(2),
                // Inner loop exit: jump back to outer loop condition
                Instruction::Jump(-26),
                // Inner loop body: b = bs[_i1] - using param directly
                Instruction::LoadVar("bs".to_string()),
                Instruction::LoadVar("_i1".to_string()),
                Instruction::LoadArrayElement,
                Instruction::StoreVar("b".to_string()),
                // _i1 = _i1 + 1
                Instruction::LoadVar("_i1".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("_i1".to_string()),
                // result = result + (a * b)
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                // Jump back to inner loop condition
                Instruction::Jump(-20),
            ],
        )],
    })
}
