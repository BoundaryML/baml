//! VM tests for deep_copy functionality

mod common;
use common::{assert_vm_executes, ExecState, Instance, Object, Program, Value};
use indexmap::indexmap;

#[test]
fn deep_copy_object() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            class Tree {
                value string
                children Tree[]
            }

            function main() -> Tree {
                let t = Tree { value: "1", children: [
                    Tree { value: "2", children: [] },
                    Tree { value: "3", children: [] },
                ] };

                let copy = baml.deep_copy(t);

                copy
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Object(Object::Instance(Instance {
            class: String::from("Tree"),
            fields: Instance::fields(indexmap! {
                "value" => Value::string("1"),

                "children" => Value::Object(Object::Array(vec![
                    Value::Object(Object::Instance(Instance {
                        class: String::from("Tree"),
                        fields: Instance::fields(indexmap! {
                            "value" => Value::string("2"),
                            "children" => Value::Object(Object::Array(vec![])),
                        }),
                    })),

                    Value::Object(Object::Instance(Instance {
                        class: String::from("Tree"),
                        fields: Instance::fields(indexmap! {
                            "value" => Value::string("3"),
                            "children" => Value::Object(Object::Array(vec![])),
                        }),
                    })),
                ])),
            }),
        }))),
    })
}

#[test]
fn deep_copy_independence() -> anyhow::Result<()> {
    // Test that deep copy creates truly independent objects
    assert_vm_executes(Program {
        source: r#"
            class Node {
                value int
                children Node[]
            }

            function main() -> int {
                let original = Node { value: 1, children: [
                    Node { value: 2, children: [] },
                    Node { value: 3, children: [] },
                ] };

                let copy = baml.deep_copy(original);

                // Modify the original nested object
                original.children[0].value = 99;

                // The copy should remain unchanged
                copy.children[0].value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(2)),
    })
}

#[test]
fn deep_copy_nested_arrays_in_class() -> anyhow::Result<()> {
    // Test deep copy with nested arrays inside class instances
    assert_vm_executes(Program {
        source: r#"
            class Matrix {
                data int[][]
            }

            function main() -> int {
                let original = Matrix { data: [[1, 2], [3, 4]] };
                let copy = baml.deep_copy(original);

                // Modify the original nested array
                original.data[0][0] = 99;

                // The copy should remain unchanged
                copy.data[0][0]
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn deep_copy_map_in_class() -> anyhow::Result<()> {
    // Test deep copy with maps inside class instances
    assert_vm_executes(Program {
        source: r#"
            class Container {
                values map<string, int>
            }

            function main() -> int {
                let original = Container {
                    values: {"a": 1, "b": 2}
                };
                let copy = baml.deep_copy(original);

                // Modify the original map
                original.values["a"] = 99;

                // The copy should remain unchanged
                copy.values["a"]
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

#[test]
fn deep_copy_complex_nested_structure() -> anyhow::Result<()> {
    // Test deep copy with complex nested structures
    assert_vm_executes(Program {
        source: r#"
            class Inner {
                value int
            }

            class Middle {
                inner Inner
                list Inner[]
            }

            class Outer {
                middle Middle
                data map<string, Inner>
            }

            function main() -> int {
                let original = Outer {
                    middle: Middle {
                        inner: Inner { value: 1 },
                        list: [Inner { value: 2 }, Inner { value: 3 }]
                    },
                    data: {
                        "first": Inner { value: 4 },
                        "second": Inner { value: 5 }
                    }
                };

                let copy = baml.deep_copy(original);

                // Modify multiple parts of the original
                original.middle.inner.value = 100;
                original.middle.list[0].value = 200;
                original.data["first"].value = 300;

                // The copy should remain completely unchanged
                // Return sum of original values: 1 + 2 + 4 = 7
                copy.middle.inner.value + copy.middle.list[0].value + copy.data["first"].value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(7)),
    })
}

#[test]
fn deep_copy_circular_reference() -> anyhow::Result<()> {
    // Test that deep_copy handles circular references correctly
    assert_vm_executes(Program {
        source: r#"
            class Node {
                value int
                children Node[]
            }

            function main() -> int {
                let a = Node { value: 1, children: [] };
                let b = Node { value: 2, children: [a] };

                // Create a circular reference
                a.children = [b];

                let copy = baml.deep_copy(a);

                // Modify the original
                a.value = 99;

                // The copy should be unchanged
                copy.value
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Int(1)),
    })
}

// ============ deep_equals tests ============

#[test]
fn deep_equals_primitives() -> anyhow::Result<()> {
    // Test equality of primitive values
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                let a = 42;
                let b = 42;
                baml.deep_equals(a, b)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn deep_equals_different_primitives() -> anyhow::Result<()> {
    // Test inequality of different primitive values
    assert_vm_executes(Program {
        source: r#"
            function main() -> bool {
                let a = 42;
                let b = 43;
                baml.deep_equals(a, b)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn deep_equals_simple_objects() -> anyhow::Result<()> {
    // Test equality of simple class instances
    assert_vm_executes(Program {
        source: r#"
            class Point {
                x int
                y int
            }

            function main() -> bool {
                let p1 = Point { x: 10, y: 20 };
                let p2 = Point { x: 10, y: 20 };
                baml.deep_equals(p1, p2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn deep_equals_different_objects() -> anyhow::Result<()> {
    // Test inequality when objects have different values
    assert_vm_executes(Program {
        source: r#"
            class Point {
                x int
                y int
            }

            function main() -> bool {
                let p1 = Point { x: 10, y: 20 };
                let p2 = Point { x: 10, y: 21 };
                baml.deep_equals(p1, p2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn deep_equals_nested_objects() -> anyhow::Result<()> {
    // Test deep equality with nested objects
    assert_vm_executes(Program {
        source: r#"
            class Node {
                value int
                children Node[]
            }

            function main() -> bool {
                let n1 = Node { value: 1, children: [
                    Node { value: 2, children: [] },
                    Node { value: 3, children: [] }
                ] };

                let n2 = Node { value: 1, children: [
                    Node { value: 2, children: [] },
                    Node { value: 3, children: [] }
                ] };

                baml.deep_equals(n1, n2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn deep_equals_nested_objects_different() -> anyhow::Result<()> {
    // Test inequality with different nested objects
    assert_vm_executes(Program {
        source: r#"
            class Node {
                value int
                children Node[]
            }

            function main() -> bool {
                let n1 = Node { value: 1, children: [
                    Node { value: 2, children: [] },
                    Node { value: 3, children: [] }
                ] };

                let n2 = Node { value: 1, children: [
                    Node { value: 2, children: [] },
                    Node { value: 4, children: [] } // Different value here
                ] };

                baml.deep_equals(n1, n2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(false)),
    })
}

#[test]
fn deep_equals_with_arrays() -> anyhow::Result<()> {
    // Test equality with arrays in class fields
    assert_vm_executes(Program {
        source: r#"
            class Container {
                data int[]
            }

            function main() -> bool {
                let c1 = Container { data: [1, 2, 3, 4] };
                let c2 = Container { data: [1, 2, 3, 4] };
                baml.deep_equals(c1, c2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn deep_equals_with_maps() -> anyhow::Result<()> {
    // Test equality with maps in class fields
    assert_vm_executes(Program {
        source: r#"
            class MapContainer {
                values map<string, int>
            }

            function main() -> bool {
                let m1 = MapContainer { values: {"a": 1, "b": 2} };
                let m2 = MapContainer { values: {"a": 1, "b": 2} };
                baml.deep_equals(m1, m2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn deep_equals_same_reference() -> anyhow::Result<()> {
    // Test that same reference is equal (optimization path)
    assert_vm_executes(Program {
        source: r#"
            class Node {
                value int
                children Node[]
            }

            function main() -> bool {
                let n = Node { value: 1, children: [] };
                baml.deep_equals(n, n)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn deep_equals_circular_structure() -> anyhow::Result<()> {
    // Test deep equals with circular references
    assert_vm_executes(Program {
        source: r#"
            class Node {
                value int
                children Node[]
            }

            function main() -> bool {
                // Create two identical circular structures
                let a1 = Node { value: 1, children: [] };
                let b1 = Node { value: 2, children: [a1] };
                a1.children = [b1];

                let a2 = Node { value: 1, children: [] };
                let b2 = Node { value: 2, children: [a2] };
                a2.children = [b2];

                baml.deep_equals(a1, a2)
            }
        "#,
        function: "main",
        expected: ExecState::Complete(Value::Bool(true)),
    })
}
