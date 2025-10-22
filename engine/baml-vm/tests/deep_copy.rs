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
                "value" => Value::Object(Object::String(String::from("1"))),

                "children" => Value::Object(Object::Array(vec![
                    Value::Object(Object::Instance(Instance {
                        class: String::from("Tree"),
                        fields: Instance::fields(indexmap! {
                            "value" => Value::Object(Object::String(String::from("2"))),
                            "children" => Value::Object(Object::Array(vec![])),
                        }),
                    })),

                    Value::Object(Object::Instance(Instance {
                        class: String::from("Tree"),
                        fields: Instance::fields(indexmap! {
                            "value" => Value::Object(Object::String(String::from("3"))),
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
