//! Compiler tests for control flow statements (if/else, while loops, break, continue, returns).

use baml_vm::{BinOp, CmpOp, GlobalIndex, Instruction};

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
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(0),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
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
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(2),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::LoadVar(2),
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
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(0),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::LoadVar(2),
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
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(2),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::LoadVar(2),
                Instruction::PopReplace(1),
                Instruction::LoadVar(2),
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
                Instruction::LoadConst(0),
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(10),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::LoadConst(2),
                Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                Instruction::LoadVar(3),
                Instruction::Call(1),
                Instruction::Pop(1),
                Instruction::Pop(2),
                Instruction::Jump(9),
                Instruction::Pop(1),
                Instruction::LoadConst(3),
                Instruction::LoadConst(4),
                Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
                Instruction::LoadVar(4),
                Instruction::Call(1),
                Instruction::Pop(1),
                Instruction::Pop(2),
                Instruction::LoadVar(2),
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
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(0),
                Instruction::Jump(9),
                Instruction::Pop(1),
                Instruction::LoadVar(2),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(2),
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
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(3),
                Instruction::PopReplace(1),
                Instruction::Jump(13),
                Instruction::Pop(1),
                Instruction::LoadVar(2),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::LoadVar(3),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(2),
                Instruction::LoadVar(3),
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
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(0),
                Instruction::Jump(9),
                Instruction::Pop(1),
                Instruction::LoadVar(2),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::Jump(3),
                Instruction::Pop(1),
                Instruction::LoadConst(2),
                Instruction::LoadVar(3),
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
                Instruction::LoadVar(1),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(3),
                Instruction::PopReplace(1),
                Instruction::Jump(13),
                Instruction::Pop(1),
                Instruction::LoadVar(2),
                Instruction::JumpIfFalse(6),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::LoadVar(3),
                Instruction::PopReplace(1),
                Instruction::Jump(5),
                Instruction::Pop(1),
                Instruction::LoadConst(2),
                Instruction::LoadVar(3),
                Instruction::PopReplace(1),
                Instruction::LoadVar(3),
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
                Instruction::LoadVar(1),
                Instruction::LoadVar(2),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(18),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
                Instruction::LoadVar(2),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(7),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
                Instruction::LoadVar(2),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar(1),
                Instruction::Jump(6),
                Instruction::Pop(1),
                Instruction::LoadVar(2),
                Instruction::LoadVar(1),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar(2),
                Instruction::Jump(-20),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
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
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::LoadConst(2),
                Instruction::LoadVar(2),
                Instruction::LoadVar(3),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(1),
                Instruction::LoadVar(1),
                Instruction::LoadConst(3),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::LoadConst(4),
                Instruction::StoreVar(1),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::Pop(2),
                Instruction::LoadVar(1),
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
                Instruction::LoadConst(0),
                Instruction::LoadVar(1),
                Instruction::LoadConst(1),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
                Instruction::LoadConst(2),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(1),
                Instruction::LoadVar(1),
                Instruction::LoadConst(3),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(5),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::Jump(-17),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
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
                Instruction::LoadConst(0),
                // while true { ... }
                Instruction::LoadConst(1),
                Instruction::JumpIfFalse(19),
                Instruction::Pop(1),
                // if limit == 0 { break; }
                Instruction::LoadVar(1),
                Instruction::LoadConst(2),
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(13),
                Instruction::Jump(2),
                Instruction::Pop(1),
                // result = result * limit;
                Instruction::LoadVar(2),
                Instruction::LoadVar(1),
                Instruction::BinOp(BinOp::Mul),
                Instruction::StoreVar(2),
                // limit = limit - 1;
                Instruction::LoadVar(1),
                Instruction::LoadConst(3),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar(1),
                // loop back and exit
                Instruction::Jump(-19),
                Instruction::Pop(1),
                // return result
                Instruction::LoadVar(2),
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
                Instruction::LoadConst(0),
                // let should_continue = true;
                Instruction::LoadConst(1),
                // while should_continue { ... }
                Instruction::LoadVar(3),
                Instruction::JumpIfFalse(21),
                Instruction::Pop(1),
                // result = result * limit;
                Instruction::LoadVar(2),
                Instruction::LoadVar(1),
                Instruction::BinOp(BinOp::Mul),
                Instruction::StoreVar(2),
                // limit = limit - 1;
                Instruction::LoadVar(1),
                Instruction::LoadConst(2),
                Instruction::BinOp(BinOp::Sub),
                Instruction::StoreVar(1),
                // if limit != 0 { continue; } else { should_continue = false; }
                Instruction::LoadVar(1),
                Instruction::LoadConst(3),
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(5),
                Instruction::Jump(4),
                Instruction::Pop(1),
                Instruction::LoadConst(4),
                Instruction::StoreVar(3),
                Instruction::Jump(-21),
                Instruction::Pop(1),
                // return result
                Instruction::LoadVar(2),
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
                Instruction::LoadConst(0),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadConst(1),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(1),
                Instruction::Jump(-4),
                Instruction::Pop(1),
                Instruction::LoadConst(2),
                Instruction::JumpIfFalse(4),
                Instruction::Pop(1),
                Instruction::Jump(3),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::Jump(-15),
                Instruction::Pop(1),
                Instruction::LoadConst(3),
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
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::JumpIfFalse(18),
                Instruction::Pop(1),
                Instruction::LoadConst(2),
                Instruction::JumpIfFalse(8),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
                Instruction::LoadConst(3),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(1),
                Instruction::Jump(3),
                Instruction::Jump(-8),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
                Instruction::LoadConst(4),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(1),
                Instruction::Jump(3),
                Instruction::Jump(-18),
                Instruction::Pop(1),
                Instruction::LoadVar(1),
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
                Instruction::LoadConst(0),
                Instruction::LoadVar(1),
                Instruction::PopReplace(1),
                Instruction::LoadVar(1),
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
                Instruction::LoadVar(1),   // x
                Instruction::LoadConst(0), // 42
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(5), // to 8
                Instruction::Pop(1),
                Instruction::LoadConst(1), // 1
                Instruction::Return,
                Instruction::Jump(2), // to 9
                Instruction::Pop(1),
                Instruction::LoadVar(1),   // x
                Instruction::LoadConst(2), // 5
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
                Instruction::LoadConst(0), // 1
                Instruction::LoadVar(2),   // a
                Instruction::LoadConst(1), // 0
                Instruction::CmpOp(CmpOp::Eq),
                Instruction::JumpIfFalse(5), // to 9
                Instruction::Pop(1),
                Instruction::LoadConst(2), // 0
                Instruction::Return,
                Instruction::Jump(2), // to 10
                Instruction::Pop(1),
                Instruction::LoadConst(3), // 1
                Instruction::LoadVar(2),   // a
                Instruction::LoadVar(3),   // b
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(5), // to 19
                Instruction::Pop(1),
                Instruction::LoadConst(4), // 0
                Instruction::Return,
                Instruction::Jump(2), // to 20
                Instruction::Pop(1),
                Instruction::Pop(1),
                Instruction::LoadConst(5), // 2
                Instruction::LoadConst(6), // 3
                Instruction::LoadVar(4),   // b
                Instruction::LoadVar(3),   // c
                Instruction::CmpOp(CmpOp::NotEq),
                Instruction::JumpIfFalse(10), // to 36
                Instruction::Pop(1),
                Instruction::LoadConst(7),   // true
                Instruction::JumpIfFalse(5), // to 34
                Instruction::Pop(1),
                Instruction::LoadConst(8), // 0
                Instruction::Return,
                Instruction::Jump(2), // to 35
                Instruction::Pop(1),
                Instruction::Jump(-12), // to 23
                Instruction::Pop(1),
                Instruction::Pop(2),
                Instruction::LoadConst(9), // 7
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
                Instruction::LoadConst(0),
                Instruction::LoadVar(1),
                Instruction::LoadGlobal(GlobalIndex::from_raw(3)),
                Instruction::LoadVar(3),
                Instruction::Call(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(5),
                Instruction::LoadVar(4),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(15),
                Instruction::Pop(1),
                Instruction::LoadVar(3),
                Instruction::LoadVar(5),
                Instruction::LoadArrayElement,
                Instruction::LoadVar(5),
                Instruction::LoadConst(1),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(5),
                Instruction::LoadVar(2),
                Instruction::LoadVar(6),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(2),
                Instruction::Pop(1),
                Instruction::Jump(-17),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar(2),
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
                Instruction::LoadConst(0),
                Instruction::LoadVar(1),
                Instruction::LoadGlobal(GlobalIndex::from_raw(3)),
                Instruction::LoadVar(3),
                Instruction::Call(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(5),
                Instruction::LoadVar(4),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(24),
                Instruction::Pop(1),
                Instruction::LoadVar(3),
                Instruction::LoadVar(5),
                Instruction::LoadArrayElement,
                Instruction::LoadVar(5),
                Instruction::LoadConst(1),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(5),
                Instruction::LoadVar(6),
                Instruction::LoadConst(2),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::Pop(1),
                Instruction::Jump(10),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::LoadVar(2),
                Instruction::LoadVar(6),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(2),
                Instruction::Pop(1),
                Instruction::Jump(-26),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar(2),
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
                Instruction::LoadConst(0),
                Instruction::LoadVar(1),
                Instruction::LoadGlobal(GlobalIndex::from_raw(3)),
                Instruction::LoadVar(3),
                Instruction::Call(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(5),
                Instruction::LoadVar(4),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(24),
                Instruction::Pop(1),
                Instruction::LoadVar(3),
                Instruction::LoadVar(5),
                Instruction::LoadArrayElement,
                Instruction::LoadVar(5),
                Instruction::LoadConst(1),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(5),
                Instruction::LoadVar(6),
                Instruction::LoadConst(2),
                Instruction::CmpOp(CmpOp::Gt),
                Instruction::JumpIfFalse(5),
                Instruction::Pop(1),
                Instruction::Pop(1),
                Instruction::Jump(8),
                Instruction::Jump(2),
                Instruction::Pop(1),
                Instruction::LoadVar(2),
                Instruction::LoadVar(6),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(2),
                Instruction::Pop(1),
                Instruction::Jump(-26),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar(2),
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
                Instruction::LoadConst(0),
                Instruction::LoadVar(1),
                Instruction::LoadGlobal(GlobalIndex::from_raw(3)),
                Instruction::LoadVar(4),
                Instruction::Call(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(6),
                Instruction::LoadVar(5),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(38),
                Instruction::Pop(1),
                Instruction::LoadVar(4),
                Instruction::LoadVar(6),
                Instruction::LoadArrayElement,
                Instruction::LoadVar(6),
                Instruction::LoadConst(1),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(6),
                Instruction::LoadVar(2),
                Instruction::LoadGlobal(GlobalIndex::from_raw(3)),
                Instruction::LoadVar(8),
                Instruction::Call(1),
                Instruction::LoadConst(0),
                Instruction::LoadVar(10),
                Instruction::LoadVar(9),
                Instruction::CmpOp(CmpOp::Lt),
                Instruction::JumpIfFalse(17),
                Instruction::Pop(1),
                Instruction::LoadVar(8),
                Instruction::LoadVar(10),
                Instruction::LoadArrayElement,
                Instruction::LoadVar(10),
                Instruction::LoadConst(1),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(10),
                Instruction::LoadVar(3),
                Instruction::LoadVar(7),
                Instruction::LoadVar(11),
                Instruction::BinOp(BinOp::Mul),
                Instruction::BinOp(BinOp::Add),
                Instruction::StoreVar(3),
                Instruction::Pop(1),
                Instruction::Jump(-19),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::Pop(1),
                Instruction::Jump(-40),
                Instruction::Pop(1),
                Instruction::Pop(3),
                Instruction::LoadVar(3),
                Instruction::Return,
            ],
        )],
    })
}
