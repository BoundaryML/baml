//! Compiler tests for class construction and field operations.

use baml_tests::{
    codegen::{Program, assert_compiles},
    vm::{Instruction, Value},
};
use baml_vm::BinOp;

#[test]
fn class_constructor() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            class Point {
                x int
                y int
            }

            function main() -> Point {
                let p = Point { x: 1, y: 2 };
                p
            }
        ",
        expected: vec![(
            "main",
            // 'p' is Virtual (single-use), inlined at use site:
            vec![
                Instruction::AllocInstance(Value::class("Point")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreField(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn class_constructor_return_directly() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            class Point {
                x int
                y int
            }

            function main() -> Point {
                Point { x: 1, y: 2 }
            }
        ",
        expected: vec![(
            "main",
            // Stackification with Virtual _0 and fall-through elimination:
            vec![
                Instruction::AllocInstance(Value::class("Point")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreField(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "spread operator not yet in HIR"]
fn class_constructor_with_spread_operator() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Point {
                x int
                y int
                z int
                w int
            }

            function default_point() -> Point {
                Point { x: 0, y: 0, z: 0, w: 0 }
            }

            function main() -> Point {
                let p = Point { x: 1, y: 2, ...default_point() };
                p
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::AllocInstance(Value::class("Point")),
                Instruction::LoadGlobal(Value::function("default_point")),
                Instruction::Call(0),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(0),
                Instruction::StoreField(0),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(1),
                Instruction::StoreField(1),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(2),
                Instruction::StoreField(2),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(3),
                Instruction::StoreField(3),
                Instruction::Pop(1),
                Instruction::LoadVar("p".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_class_construction() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }

            function main() -> int {
                let o = Outer { inner: Inner { value: 42 } };
                42
            }
        ",
        expected: vec![(
            "main",
            // Stackification with fall-through elimination:
            // o is assigned but never read (dead store)
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::AllocInstance(Value::class("Outer")),
                Instruction::Copy(0),
                Instruction::AllocInstance(Value::class("Inner")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::StoreField(0),
                Instruction::StoreField(0),
                Instruction::StoreVar("o".to_string()),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_class_with_multiple_fields() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
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
                30
            }
        ",
        expected: vec![(
            "main",
            // Stackification with fall-through elimination:
            // o is assigned but never read (dead store)
            vec![
                Instruction::LoadConst(Value::Null),
                Instruction::AllocInstance(Value::class("Outer")),
                Instruction::Copy(0),
                Instruction::AllocInstance(Value::class("Inner")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::StoreField(1),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(30)),
                Instruction::StoreField(1),
                Instruction::StoreVar("o".to_string()),
                Instruction::LoadConst(Value::Int(30)),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_field_read() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
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
        ",
        expected: vec![(
            "main",
            // 'o' is Virtual (single-use), inlined at use site:
            vec![
                Instruction::AllocInstance(Value::class("Outer")),
                Instruction::Copy(0),
                Instruction::AllocInstance(Value::class("Inner")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::StoreField(0),
                Instruction::StoreField(0),
                Instruction::LoadField(0),
                Instruction::LoadField(0),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "field assignment not yet in HIR"]
fn field_assignment_simple() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            class Data {
                value int
            }

            function setDataValue(d: Data) -> int {
                d.value = 42;
                d.value
            }
        ",
        expected: vec![(
            "setDataValue",
            vec![
                // d.value = 42
                Instruction::LoadVar("d".to_string()),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::StoreField(0),
                // d.value
                Instruction::LoadVar("d".to_string()),
                Instruction::LoadField(0),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Additional spread operator tests (from old compiler)
// ============================================================================

#[test]
#[ignore = "spread operator not yet in HIR"]
fn class_constructor_with_spread_before_named_fields() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Point {
                x int
                y int
                z int
                w int
            }

            function default_point() -> Point {
                Point { x: 0, y: 0, z: 0, w: 0 }
            }

            function main() -> Point {
                let p = Point { ...default_point(), x: 1, y: 2 };
                p
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::AllocInstance(Value::class("Point")),
                Instruction::LoadGlobal(Value::function("default_point")),
                Instruction::Call(0),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(2),
                Instruction::StoreField(2),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(3),
                Instruction::StoreField(3),
                Instruction::Pop(1),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreField(1),
                Instruction::LoadVar("p".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "spread operator not yet in HIR"]
fn class_constructor_with_spread_after_named_fields() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Point {
                x int
                y int
                z int
                w int
            }

            function default_point() -> Point {
                Point { x: 0, y: 0, z: 0, w: 0 }
            }

            function main() -> Point {
                let p = Point { x: 1, y: 2, ...default_point() };
                p
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::AllocInstance(Value::class("Point")),
                Instruction::LoadGlobal(Value::function("default_point")),
                Instruction::Call(0),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(0),
                Instruction::StoreField(0),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(1),
                Instruction::StoreField(1),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(2),
                Instruction::StoreField(2),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(3),
                Instruction::StoreField(3),
                Instruction::Pop(1),
                Instruction::LoadVar("p".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "spread operator not yet in HIR"]
fn class_constructor_with_multiple_spread_operators() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Point {
                x int
                y int
                z int
                w int
            }

            function x_one() -> Point {
                Point { x: 0, y: 0, z: 0, w: 0 }
            }

            function xy_one() -> Point {
                Point { x: 1, y: 1, z: 0, w: 0 }
            }

            function xy_one_last() -> Point {
                let p = Point { ...x_one(), ...xy_one() };
                p
            }

            function x_one_last() -> Point {
                let p = Point { ...xy_one(), ...x_one() };
                p
            }
        "#,
        expected: vec![
            (
                "xy_one_last",
                vec![
                    Instruction::AllocInstance(Value::class("Point")),
                    Instruction::LoadGlobal(Value::function("xy_one")),
                    Instruction::Call(0),
                    Instruction::Copy(1),
                    Instruction::Copy(1),
                    Instruction::LoadField(0),
                    Instruction::StoreField(0),
                    Instruction::Copy(1),
                    Instruction::Copy(1),
                    Instruction::LoadField(1),
                    Instruction::StoreField(1),
                    Instruction::Copy(1),
                    Instruction::Copy(1),
                    Instruction::LoadField(2),
                    Instruction::StoreField(2),
                    Instruction::Copy(1),
                    Instruction::Copy(1),
                    Instruction::LoadField(3),
                    Instruction::StoreField(3),
                    Instruction::Pop(1),
                    Instruction::LoadVar("p".to_string()),
                    Instruction::Return,
                ],
            ),
            (
                "x_one_last",
                vec![
                    Instruction::AllocInstance(Value::class("Point")),
                    Instruction::LoadGlobal(Value::function("x_one")),
                    Instruction::Call(0),
                    Instruction::Copy(1),
                    Instruction::Copy(1),
                    Instruction::LoadField(0),
                    Instruction::StoreField(0),
                    Instruction::Copy(1),
                    Instruction::Copy(1),
                    Instruction::LoadField(1),
                    Instruction::StoreField(1),
                    Instruction::Copy(1),
                    Instruction::Copy(1),
                    Instruction::LoadField(2),
                    Instruction::StoreField(2),
                    Instruction::Copy(1),
                    Instruction::Copy(1),
                    Instruction::LoadField(3),
                    Instruction::StoreField(3),
                    Instruction::Pop(1),
                    Instruction::LoadVar("p".to_string()),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
#[ignore = "spread operator not yet in HIR"]
fn class_constructor_with_spread_operator_does_not_break_locals() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: r#"
            class Point {
                x int
                y int
                z int
                w int
            }

            function default_point() -> Point {
                Point { x: 0, y: 0, z: 0, w: 0 }
            }

            function main() -> int {
                let p = Point { x: 1, y: 2, ...default_point() };
                let x = 0;
                x
            }
        "#,
        expected: vec![(
            "main",
            vec![
                Instruction::AllocInstance(Value::class("Point")),
                Instruction::LoadGlobal(Value::function("default_point")),
                Instruction::Call(0),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(0),
                Instruction::StoreField(0),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(1),
                Instruction::StoreField(1),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(2),
                Instruction::StoreField(2),
                Instruction::Copy(1),
                Instruction::Copy(1),
                Instruction::LoadField(3),
                Instruction::StoreField(3),
                Instruction::Pop(1),
                Instruction::LoadConst(Value::Int(0)),
                Instruction::LoadVar("x".to_string()),
                Instruction::Return,
            ],
        )],
    })
}

// ============================================================================
// Field assignment tests (from old compiler)
// ============================================================================

#[test]
#[ignore = "field assignment not yet in HIR"]
fn field_assignment_compound_add() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            class Counter {
                value int
            }

            function incrementCounter(c: Counter) -> int {
                c.value += 10;
                c.value
            }
        ",
        expected: vec![(
            "incrementCounter",
            vec![
                // c.value += 10
                Instruction::LoadVar("c".to_string()), // Load c
                Instruction::Copy(0),                  // Duplicate c reference
                Instruction::LoadField(0),             // Load c.value
                Instruction::LoadConst(Value::Int(10)), // Load 10
                Instruction::BinOp(BinOp::Add),        // Add
                Instruction::StoreField(0),            // Store back to c.value
                // c.value
                Instruction::LoadVar("c".to_string()), // Load c
                Instruction::LoadField(0),             // Load c.value
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_object_construction() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
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
                o.value
            }
        ",
        expected: vec![(
            "main",
            // 'o' is Virtual (single-use), inlined at use site:
            vec![
                Instruction::AllocInstance(Value::class("Outer")),
                Instruction::Copy(0),
                Instruction::AllocInstance(Value::class("Inner")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::StoreField(1),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(30)),
                Instruction::StoreField(1),
                Instruction::LoadField(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "field assignment not yet in HIR"]
fn nested_field_assignment() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }

            function setNestedValue(i: Inner, o: Outer) -> int {
                o.inner.value = 99;
                o.inner.value
            }
        ",
        expected: vec![(
            "setNestedValue",
            vec![
                // o.inner.value = 99
                Instruction::LoadVar("o".to_string()), // Load o
                Instruction::LoadField(0),             // Load o.inner (returns Inner object)
                Instruction::LoadConst(Value::Int(99)), // Load 99
                Instruction::StoreField(0),            // Store to inner.value
                // o.inner.value
                Instruction::LoadVar("o".to_string()), // Load o
                Instruction::LoadField(0),             // Load o.inner
                Instruction::LoadField(0),             // Load inner.value
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "field assignment not yet in HIR"]
fn nested_field_assignment_compound() -> anyhow::Result<()> {
    assert_compiles(Program {
        source: "
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }

            function incrementNestedValue(o: Outer) -> int {
                o.inner.value += 10;
                o.inner.value
            }
        ",
        expected: vec![(
            "incrementNestedValue",
            vec![
                // o.inner.value += 10
                Instruction::LoadVar("o".to_string()), // Load o
                Instruction::LoadField(0),             // Load o.inner (returns Inner object)
                Instruction::Copy(0),                  // Duplicate inner reference
                Instruction::LoadField(0),             // Load inner.value
                Instruction::LoadConst(Value::Int(10)), // Load 10
                Instruction::BinOp(BinOp::Add),        // Add
                Instruction::StoreField(0),            // Store back to inner.value
                // o.inner.value
                Instruction::LoadVar("o".to_string()), // Load o
                Instruction::LoadField(0),             // Load o.inner
                Instruction::LoadField(0),             // Load inner.value
                Instruction::Return,
            ],
        )],
    })
}
