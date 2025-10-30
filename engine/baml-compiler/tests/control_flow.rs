//! Compiler tests for control flow statements (if/else, while loops, break, continue, returns).

use baml_vm::{
    test::{Instruction, Value},
    BinOp, CmpOp,
};

mod common;
use common::{assert_compiles, Program};

// If/else expressions
#[test]
fn if_else_return_expr() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                if (b) { 1 } else { 2 }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_return_expr_with_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                if (b) {
                    let a = 1;
                    a
                } else {
                    let a = 2;
                    a
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_assignment() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                let i = if (b) { 1 } else { 2 };
                i
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadVar("i".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_assignment_with_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(b: bool) -> int {
                let i = if (b) {
                    let a = 1;
                    a
                } else {
                    let a = 2;
                    a
                };

                i
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadVar("a".to_string()),
                Instruction::PopReplace(1),
                Instruction::LoadVar("i".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn if_else_normal_statement() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function identity(i: int) -> int {
                i
            }

            function main(b: bool) -> int {
                let a = 1;

                if (b) {
                    let x = 1;
                    let y = 2;
                    identity(x);
                } else {
                    let x = 3;
                    let y = 4;
                    identity(y);
                }

                a
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(10),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadGlobal(Value::function("identity")),
                Instruction::LoadVar("x".to_string()),
                Instruction::Call(1),
                Instruction::Pop(1),
                Instruction::Pop(2),
                Instruction::Jump(9),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::LoadConst(Value::Int(4)),
                Instruction::LoadGlobal(Value::function("identity")),
                Instruction::LoadVar("y".to_string()),
                Instruction::Call(1),
                Instruction::Pop(1),
                Instruction::Pop(2),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_return_expr() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(a: bool, b: bool) -> int {
                if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(9),
                Instruction::Pop(1),
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_return_expr_with_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(a: bool, b: bool) -> int {
                if (a) {
                    let x = 1;
                    x
                } else if (b) {
                    let y = 2;
                    y
                } else {
                    let z = 3;
                    z
                }
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("x".to_string()),
                Instruction::PopReplace(1),
                Instruction::Jump(13),
                Instruction::Pop(1),
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadVar("y".to_string()),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::LoadVar("z".to_string()),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_assignment() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(a: bool, b: bool) -> int {
                let result = if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                };

                result
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::Jump(9),
                Instruction::Pop(1),
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn else_if_assignment_with_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main(a: bool, b: bool) -> int {
                let result = if (a) {
                    let x = 1;
                    x
                } else if (b) {
                    let y = 2;
                    y
                } else {
                    let z = 3;
                    z
                };

                result
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("x".to_string()),
                Instruction::PopReplace(1),
                Instruction::Jump(13),
                Instruction::Pop(1),
                Instruction::LoadVar("b".to_string()),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadVar("y".to_string()),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::LoadVar("z".to_string()),
                Instruction::PopReplace(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn while_loop_gcd() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function GCD(a: int, b: int) -> int {
                while (a != b) {
                    if (a > b) {
                        a = a - b;
                    } else {
                        b = b - a;
                    }
                }

                a
            }
        "#,
        expected: vec![(
            "GCD",
            vec![
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(18),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(7),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("a".to_string()),
                Instruction::Jump(6),
                Instruction::Pop(1),
                Instruction::LoadVar("b".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("b".to_string()),
                Instruction::Jump(-20),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

// This tests that we don't emit POP_REPLACE for if expressions when they
// do not return values.
#[test]
fn nested_block_expr_with_ending_normal_if() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let a = 1;

                {
                    let b = 2;
                    let c = 3;
                    a = b + c;

                    if (a == 5) {
                        a = 10;
                    }
                }

                a
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::LoadConst(Value::Int(3)),
                Instruction::LoadVar("b".to_string()),
                Instruction::LoadVar("c".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::StoreVar("a".to_string()),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::Pop(2),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

// This tests that we don't emit POP_REPLACE for if expressions when they
// do not return values.
#[test]
fn while_loop_with_ending_if() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let a = 1;

                while (a < 5) {
                    a += 1;

                    if (a == 2) {
                        break;
                    }
                }

                a
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(5),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::Jump(-17),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn break_factorial() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Factorial(limit: int) -> int {
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
        "#,
        expected: vec![(
            "Factorial",
            vec![
                // let result = 1;
                Instruction::LoadConst(Value::Int(1)),
                // while true { ... }
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(19),
                Instruction::Pop(1),
                // if limit == 0 { break; }
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(13),
                Instruction::Jump(2),
                Instruction::Pop(1),
                // result = result * limit;
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::StoreVar("result".to_string()),
                // limit = limit - 1;
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("limit".to_string()),
                // loop back and exit
                Instruction::Jump(-19),
                Instruction::Pop(1),
                // return result
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn continue_factorial() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Factorial(limit: int) -> int {
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
        "#,
        expected: vec![(
            "Factorial",
            vec![
                // let result = 1;
                Instruction::LoadConst(Value::Int(1)),
                // let should_continue = true;
                Instruction::LoadConst(Value::Bool(true)),
                // while should_continue { ... }
                Instruction::LoadVar("should_continue".to_string()),
                Instruction::JumpIfFalse(21),
                Instruction::Pop(1),
                // result = result * limit;
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("limit".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::StoreVar("result".to_string()),
                // limit = limit - 1;
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar("limit".to_string()),
                // if limit != 0 { continue; } else { should_continue = false; }
                Instruction::LoadVar("limit".to_string()),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(5),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::StoreVar("should_continue".to_string()),
                Instruction::Jump(-21),
                Instruction::Pop(1),
                // return result
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn continue_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Nested() -> int {
                while (true) {
                    while (false) {
                        continue;
                    }
                    if (false) {
                        continue;
                    }
                }
                5
            }
        "#,
        expected: vec![(
            "Nested",
            vec![
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(1),
                Instruction::Jump(-4),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(false)),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(3),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::Jump(-15),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(5)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn break_nested() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            function Nested() -> int {
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
        "#,
        expected: vec![(
            "Nested",
            vec![
                Instruction::LoadConst(Value::Int(5)),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(18),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(true)),
                Instruction::JumpIfFalse(8),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::Jump(3),
                Instruction::Jump(-8),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("a".to_string()),
                Instruction::Jump(3),
                Instruction::Jump(-18),
                Instruction::Pop(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

// Block expressions
#[test]
fn block_expr() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function main() -> int {
                let a = {
                    let b = 1;
                    b
                };

                a
            }
        ",
        expected: vec![(
            "main",
            vec![
                Instruction::LoadConst(Value::Int(1)),
                Instruction::LoadVar("b".to_string()),
                Instruction::PopReplace(1),
                Instruction::LoadVar("a".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

// Return statements
#[test]
fn early_return() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function EarlyReturn(x: int) -> int {
              if (x == 42) { return 1; }

              x + 5
            }
        ",
        expected: vec![(
            "EarlyReturn",
            vec![
                Instruction::LoadVar("x".to_string()),  // x
                Instruction::LoadConst(Value::Int(42)), // 42
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(5), // to 8
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)), // 1
                Instruction::Return,
                Instruction::Jump(2), // to 9
                Instruction::Pop(1),
                Instruction::LoadVar("x".to_string()), // x
                Instruction::LoadConst(Value::Int(5)), // 5
                Instruction::BinOp(BinOp::Add),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn return_with_stack() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            function WithStack(x: int) -> int {
              let a = 1;

              // NOTE: currently there's no empty returns.

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
            }
        ",
        expected: vec![(
            "WithStack",
            vec![
                Instruction::LoadConst(Value::Int(1)), // 1
                Instruction::LoadVar("a".to_string()), // a
                Instruction::LoadConst(Value::Int(0)), // 0
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(5), // to 9
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)), // 0
                Instruction::Return,
                Instruction::Jump(2), // to 10
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(1)), // 1
                Instruction::LoadVar("a".to_string()), // a
                Instruction::LoadVar("b".to_string()), // b
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(5), // to 19
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)), // 0
                Instruction::Return,
                Instruction::Jump(2), // to 20
                Instruction::Pop(1),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(2)), // 2
                Instruction::LoadConst(Value::Int(3)), // 3
                Instruction::LoadVar("b".to_string()), // b
                Instruction::LoadVar("c".to_string()), // c
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(10), // to 36
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Bool(true)), // true
                Instruction::JumpIfFalse(5),               // to 34
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)), // 0
                Instruction::Return,
                Instruction::Jump(2), // to 35
                Instruction::Pop(1),
                Instruction::Jump(-12), // to 23
                Instruction::Pop(1),
                Instruction::Pop(2),
                Instruction::LoadConst(Value::Int(7)), // 7
                Instruction::Return,
            ],
        )],
    })
}

// For loop tests
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
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("__baml for loop iterated array 0".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadVar("__baml for loop array length 0".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadVar("__baml for loop iterated array 0".to_string()),
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("__baml for loop index 0".to_string()),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                Instruction::Pop(1),
                Instruction::Jump(-17),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
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
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("__baml for loop iterated array 0".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadVar("__baml for loop array length 0".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(24),
                Instruction::Pop(1),
                Instruction::LoadVar("__baml for loop iterated array 0".to_string()),
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("__baml for loop index 0".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::Pop(1),
                Instruction::Jump(10),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                Instruction::Pop(1),
                Instruction::Jump(-26),
                Instruction::Pop(1),
                Instruction::Pop(3),
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
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("xs".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("__baml for loop iterated array 0".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadVar("__baml for loop array length 0".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(24),
                Instruction::Pop(1),
                Instruction::LoadVar("__baml for loop iterated array 0".to_string()),
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("__baml for loop index 0".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::Pop(1),
                Instruction::Jump(8),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("x".to_string()),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                Instruction::Pop(1),
                Instruction::Jump(-26),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
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
            vec![
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("as".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("__baml for loop iterated array 0".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadVar("__baml for loop array length 0".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(38),
                Instruction::Pop(1),
                Instruction::LoadVar("__baml for loop iterated array 0".to_string()),
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("__baml for loop index 0".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("__baml for loop index 0".to_string()),
                Instruction::LoadVar("bs".to_string()),
                Instruction::LoadGlobal(Value::function("baml.Array.length")),
                Instruction::LoadVar("__baml for loop iterated array 1".to_string()),
                Instruction::Call(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("__baml for loop index 1".to_string()),
                Instruction::LoadVar("__baml for loop array length 1".to_string()),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(17),
                Instruction::Pop(1),
                Instruction::LoadVar("__baml for loop iterated array 1".to_string()),
                Instruction::LoadVar("__baml for loop index 1".to_string()),
                Instruction::LoadArrayElement,
                Instruction::LoadVar("__baml for loop index 1".to_string()),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("__baml for loop index 1".to_string()),
                Instruction::LoadVar("result".to_string()),
                Instruction::LoadVar("a".to_string()),
                Instruction::LoadVar("b".to_string()),
                Instruction::BinOp(BinOp::Mul),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar("result".to_string()),
                Instruction::Pop(1),
                Instruction::Jump(-19),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::Pop(1),
                Instruction::Jump(-40),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar("result".to_string()),
                Instruction::Return,
            ],
        )],
    })
}
