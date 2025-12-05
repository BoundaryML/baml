//! Compiler tests for class construction and field operations.

use baml_vm::test::{Instruction, Value};

use super::common::{Program, assert_compiles};

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
                Instruction::AllocInstance(Value::class("Point")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(1)),
                Instruction::StoreField(0),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(2)),
                Instruction::StoreField(1),
                Instruction::LoadVar("p".to_string()),
                Instruction::PopReplace(1),
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
            vec![
                // Outer constructor
                Instruction::AllocInstance(Value::class("Outer")),
                Instruction::Copy(0),
                // Nested Inner construction
                Instruction::AllocInstance(Value::class("Inner")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::StoreField(0), // Inner.value = 42
                Instruction::StoreField(0), // Outer.inner = Inner
                Instruction::LoadConst(Value::Int(42)),
                Instruction::PopReplace(1),
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
            vec![
                // Outer constructor
                Instruction::AllocInstance(Value::class("Outer")),
                Instruction::Copy(0),
                // Nested Inner construction
                Instruction::AllocInstance(Value::class("Inner")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(10)),
                Instruction::StoreField(0), // x = 10
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(20)),
                Instruction::StoreField(1), // y = 20
                Instruction::StoreField(0), // Outer.inner = Inner
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(30)),
                Instruction::StoreField(1), // Outer.value = 30
                Instruction::LoadConst(Value::Int(30)),
                Instruction::PopReplace(1),
                Instruction::Return,
            ],
        )],
    })
}

#[test]
#[ignore = "field access not yet properly resolved in HIR"]
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
            vec![
                // Create Outer { inner: Inner { value: 42 } }
                Instruction::AllocInstance(Value::class("Outer")),
                Instruction::Copy(0),
                Instruction::AllocInstance(Value::class("Inner")),
                Instruction::Copy(0),
                Instruction::LoadConst(Value::Int(42)),
                Instruction::StoreField(0), // Inner.value = 42
                Instruction::StoreField(0), // Outer.inner = Inner
                // o.inner.value
                Instruction::LoadVar("o".to_string()),
                Instruction::LoadField(0), // o.inner
                Instruction::LoadField(0), // inner.value
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
