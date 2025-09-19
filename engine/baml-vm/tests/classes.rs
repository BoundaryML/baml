//! VM tests for classes (constructors, field access, ...)

use baml_vm::{ObjectIndex, Value, VmExecState};

mod common;
use common::{assert_vm_executes, assert_vm_executes_with_inspection, Program};

// Class tests
#[test]
fn class_constructor() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
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
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(38))),
        },
        |vm| {
            let baml_vm::Object::Instance(instance) = &vm.objects[ObjectIndex::from_raw(38)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(38)]
                );
            };

            assert_eq!(instance.fields, &[Value::Int(1), Value::Int(2)]);

            Ok(())
        },
    )
}

#[test]
fn class_constructor_with_spread_operator() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: "
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
            ",
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(39))),
        },
        |vm| {
            let baml_vm::Object::Instance(instance) = &vm.objects[ObjectIndex::from_raw(39)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
                );
            };

            assert_eq!(
                instance.fields,
                &[Value::Int(0), Value::Int(0), Value::Int(0), Value::Int(0)],
            );

            Ok(())
        },
    )
}

#[test]
fn class_constructor_with_multiple_spread_operators() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: "
                class Point {
                    x int
                    y int
                    z int
                    w int
                }

                function x_one() -> Point {
                    Point { x: 1, y: 0, z: 0, w: 0 }
                }

                function xy_one() -> Point {
                    Point { x: 1, y: 1, z: 0, w: 0 }
                }

                function main() -> Point {
                    let p = Point { ...x_one(), ...xy_one() };
                    p
                }
            ",
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(40))),
        },
        |vm| {
            let baml_vm::Object::Instance(instance) = &vm.objects[ObjectIndex::from_raw(40)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
                );
            };

            assert_eq!(
                instance.fields,
                &[Value::Int(1), Value::Int(1), Value::Int(0), Value::Int(0)],
            );

            Ok(())
        },
    )
}

#[test]
fn class_constructor_with_spread_operator_before_named_fields() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: "
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
            ",
            function: "main",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(39))),
        },
        |vm| {
            let baml_vm::Object::Instance(instance) = &vm.objects[ObjectIndex::from_raw(39)] else {
                panic!(
                    "expected Instance, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
                );
            };

            assert_eq!(
                instance.fields,
                &[Value::Int(1), Value::Int(2), Value::Int(0), Value::Int(0)],
            );

            Ok(())
        },
    )
}

#[test]
fn class_constructor_with_spread_operator_does_not_break_locals() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: "
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
            ",
        function: "main",
        expected: VmExecState::Complete(Value::Int(0)),
    })
}

#[test]
fn nested_object_construction() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
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
                // Test that construction worked by accessing a simple field
                o.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(30)),
    })
}

#[test]
fn nested_object_construction_with_field_access() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
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
                // Test nested field access after nested construction
                o.inner.y
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(20)),
    })
}

#[test]
fn nested_field_read_with_nested_construction() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
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
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn nested_field_read() -> anyhow::Result<()> {
    // Test nested field read without nested construction
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }
            class Outer {
                inner Inner
            }
            function main() -> int {
                let i = Inner { value: 42 };
                let o = Outer { inner: i };
                o.inner.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(42)),
    })
}

#[test]
fn constructor_with_preceding_variables() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class MyClass {
                x int
                y int
            }
            function main() -> int {
                let a = 10;
                let b = 20;
                let c = 30;
                let obj = MyClass { x: 100, y: 200 };
                obj.x + obj.y + a + b + c
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(360)), // 100 + 200 + 10 + 20 + 30
    })
}

#[test]
fn nested_constructor_with_preceding_variables() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                val int
            }
            class Outer {
                inner Inner
                x int
            }
            function main() -> int {
                let a = 5;
                let b = 10;
                let obj = Outer {
                    inner: Inner { val: 100 },
                    x: 50
                };
                obj.inner.val + obj.x + a + b
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(165)), // 100 + 50 + 5 + 10
    })
}

#[test]
fn basic_method_decl() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Number {
                value int

                function add(self, other: Number) -> Number {
                    Number { value: self.value + other.value }
                }
            }

            function main() -> int {
                let a = Number { value: 1 };
                let b = Number { value: 2 };
                let n = a.add(b);
                n.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}

// Method tests
#[test]
fn mut_self_method_decl() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Number {
                value int

                function add(self, other: Number) -> bool {
                    self.value += other.value;
                    true
                }
            }

            function main() -> int {
                let a = Number { value: 1 };
                let b = Number { value: 2 };
                a.add(b);
                a.value
            }
        "#,
        function: "main",
        expected: VmExecState::Complete(Value::Int(3)),
    })
}
