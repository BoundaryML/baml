//! Compiler tests for class construction and field operations.

use baml_vm::{BinOp, GlobalIndex, Instruction, ObjectIndex};

mod common;
use common::{assert_compiles, Program};

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
            vec![
                Instruction::AllocInstance(ObjectIndex::from_raw(2)),
                Instruction::Copy(0),
                Instruction::LoadConst(0),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(1),
                Instruction::StoreField(1),
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
                Instruction::AllocInstance(ObjectIndex::from_raw(3)),
                Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
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
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
                Instruction::AllocInstance(ObjectIndex::from_raw(3)),
                Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
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
                Instruction::LoadConst(0),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(1),
                Instruction::StoreField(1),
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
                Instruction::AllocInstance(ObjectIndex::from_raw(3)),
                Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
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
                Instruction::LoadVar(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
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
                    Instruction::AllocInstance(ObjectIndex::from_raw(5)),
                    Instruction::LoadGlobal(GlobalIndex::from_raw(1)),
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
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            ),
            (
                "x_one_last",
                vec![
                    Instruction::AllocInstance(ObjectIndex::from_raw(5)),
                    Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
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
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            ),
        ],
    })
}

#[test]
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
                Instruction::AllocInstance(ObjectIndex::from_raw(3)),
                Instruction::LoadGlobal(GlobalIndex::from_raw(0)),
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
                Instruction::LoadConst(0),
                Instruction::LoadVar(2),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn field_assignment_compound_add_bytecode() -> anyhow::Result<()> {
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
                Instruction::LoadVar(1),        // Load c
                Instruction::Copy(0),           // Duplicate c reference
                Instruction::LoadField(0),      // Load c.value
                Instruction::LoadConst(0),      // Load 10
                Instruction::BinOp(BinOp::Add), // Add
                Instruction::StoreField(0),     // Store back to c.value
                // c.value
                Instruction::LoadVar(1),   // Load c
                Instruction::LoadField(0), // Load c.value
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_field_read_bytecode() -> anyhow::Result<()> {
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
            vec![
                // Create Outer { inner: Inner { value: 42 } }
                Instruction::AllocInstance(ObjectIndex::from_raw(3)), // Outer class
                Instruction::Copy(0),                                 // Copy Outer instance
                // Create Inner inline
                Instruction::AllocInstance(ObjectIndex::from_raw(2)), // Inner class
                Instruction::Copy(0),                                 // Copy Inner instance
                Instruction::LoadConst(0),                            // 42
                Instruction::StoreField(0),                           // Inner.value = 42
                Instruction::StoreField(0), // Outer.inner = Inner instance
                // o.inner.value
                Instruction::LoadVar(1),   // Load o
                Instruction::LoadField(0), // Load o.inner (returns Inner)
                Instruction::LoadField(0), // Load inner.value (returns 42)
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_object_construction_bytecode() -> anyhow::Result<()> {
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
            vec![
                // Outer constructor
                Instruction::AllocInstance(ObjectIndex::from_raw(3)), // Outer
                Instruction::Copy(0),                                 // Copy Outer instance
                // Nested Inner construction
                Instruction::AllocInstance(ObjectIndex::from_raw(2)), // Inner
                Instruction::Copy(0),                                 // Copy Inner instance
                Instruction::LoadConst(0),                            // 10
                Instruction::StoreField(0),                           // x = 10
                Instruction::Copy(0),                                 // Copy Inner instance again
                Instruction::LoadConst(1),                            // 20
                Instruction::StoreField(1),                           // y = 20
                Instruction::StoreField(0),                           // Outer.inner = Inner
                Instruction::Copy(0),                                 // Copy Outer instance
                Instruction::LoadConst(2),                            // 30
                Instruction::StoreField(1),                           // Outer.value = 30
                // o.value
                Instruction::LoadVar(1),   // o
                Instruction::LoadField(1), // value
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_field_assignment_bytecode() -> anyhow::Result<()> {
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
                Instruction::LoadVar(2),    // Load o
                Instruction::LoadField(0),  // Load o.inner (returns Inner object)
                Instruction::LoadConst(0),  // Load 99
                Instruction::StoreField(0), // Store to inner.value
                // o.inner.value
                Instruction::LoadVar(2),   // Load o
                Instruction::LoadField(0), // Load o.inner
                Instruction::LoadField(0), // Load inner.value
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn nested_field_assignment_compound_bytecode() -> anyhow::Result<()> {
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
                Instruction::LoadVar(1),        // Load o
                Instruction::LoadField(0),      // Load o.inner (returns Inner object)
                Instruction::Copy(0),           // Duplicate inner reference
                Instruction::LoadField(0),      // Load inner.value
                Instruction::LoadConst(0),      // Load 10
                Instruction::BinOp(BinOp::Add), // Add
                Instruction::StoreField(0),     // Store back to inner.value
                // o.inner.value
                Instruction::LoadVar(1),   // Load o
                Instruction::LoadField(0), // Load o.inner
                Instruction::LoadField(0), // Load inner.value
                Instruction::Return,
            ],
        )],
    })
}

#[test]
fn field_assignment_simple_bytecode() -> anyhow::Result<()> {
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
                Instruction::LoadVar(1),    // Load d
                Instruction::LoadConst(0),  // Load 42
                Instruction::StoreField(0), // Store to d.value
                // d.value
                Instruction::LoadVar(1),   // Load d
                Instruction::LoadField(0), // Load d.value
                Instruction::Return,
            ],
        )],
    })
}
