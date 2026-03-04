//! Unified tests for deep_copy and deep_equals.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn deep_copy_object() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> Tree {
        alloc_instance Tree
        copy 0
        load_const "1"
        store_field .value
        copy 0
        alloc_instance Tree
        copy 0
        load_const "2"
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_instance Tree
        copy 0
        load_const "3"
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_array 2
        store_field .children
        call baml.deep_copy
        return
    }
    "#);

    assert!(
        output.result.is_ok(),
        "deep_copy_object should succeed: {:?}",
        output.result
    );
}

#[tokio::test]
async fn deep_copy_independence() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_instance Node
        copy 0
        load_const 2
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_instance Node
        copy 0
        load_const 3
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_array 2
        store_field .children
        store_var original
        load_var original
        call baml.deep_copy
        load_var original
        load_field .children
        load_const 0
        load_array_element
        load_const 99
        store_field .value
        load_field .children
        load_const 0
        load_array_element
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn deep_copy_nested_arrays_in_class() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Matrix
        copy 0
        load_const 1
        load_const 2
        alloc_array 2
        load_const 3
        load_const 4
        alloc_array 2
        alloc_array 2
        store_field .data
        store_var original
        load_var original
        call baml.deep_copy
        load_var original
        load_field .data
        load_const 0
        load_array_element
        load_const 0
        load_const 99
        store_array_element
        load_field .data
        load_const 0
        load_array_element
        load_const 0
        load_array_element
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn deep_copy_map_in_class() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance Container
        copy 0
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        store_field .values
        store_var original
        load_var original
        call baml.deep_copy
        load_var original
        load_field .values
        load_const "a"
        load_const 99
        store_map_element
        load_field .values
        load_const "a"
        load_map_element
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn deep_copy_complex_nested_structure() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Middle
        copy 0
        alloc_instance Inner
        copy 0
        load_const 1
        store_field .value
        store_field .inner
        copy 0
        alloc_instance Inner
        copy 0
        load_const 2
        store_field .value
        alloc_instance Inner
        copy 0
        load_const 3
        store_field .value
        alloc_array 2
        store_field .list
        store_field .middle
        copy 0
        alloc_instance Inner
        copy 0
        load_const 4
        store_field .value
        alloc_instance Inner
        copy 0
        load_const 5
        store_field .value
        load_const "first"
        load_const "second"
        alloc_map 2
        store_field .data
        store_var original
        load_var original
        call baml.deep_copy
        store_var copy
        load_var original
        load_field .middle
        load_field .inner
        load_const 100
        store_field .value
        load_var original
        load_field .middle
        load_field .list
        load_const 0
        load_array_element
        load_const 200
        store_field .value
        load_var original
        load_field .data
        load_const "first"
        load_map_element
        load_const 300
        store_field .value
        load_var copy
        load_field .middle
        load_field .inner
        load_field .value
        load_var copy
        load_field .middle
        load_field .list
        load_const 0
        load_array_element
        load_field .value
        bin_op +
        load_var copy
        load_field .data
        load_const "first"
        load_map_element
        load_field .value
        bin_op +
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn deep_copy_circular_reference() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        store_var a
        load_var a
        alloc_instance Node
        copy 0
        load_const 2
        store_field .value
        copy 0
        load_var a
        alloc_array 1
        store_field .children
        alloc_array 1
        store_field .children
        load_var a
        call baml.deep_copy
        load_var a
        load_const 99
        store_field .value
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============ deep_equals tests ============

#[tokio::test]
async fn deep_equals_primitives() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = 42;
            let b = 42;
            baml.deep_equals(a, b)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 42
        load_const 42
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn deep_equals_different_primitives() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = 42;
            let b = 43;
            baml.deep_equals(a, b)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        load_const 42
        load_const 43
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn deep_equals_simple_objects() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function main() -> bool {
            let p1 = Point { x: 10, y: 20 };
            let p2 = Point { x: 10, y: 20 };
            baml.deep_equals(p1, p2)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        alloc_instance Point
        copy 0
        load_const 10
        store_field .x
        copy 0
        load_const 20
        store_field .y
        alloc_instance Point
        copy 0
        load_const 10
        store_field .x
        copy 0
        load_const 20
        store_field .y
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn deep_equals_different_objects() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function main() -> bool {
            let p1 = Point { x: 10, y: 20 };
            let p2 = Point { x: 10, y: 21 };
            baml.deep_equals(p1, p2)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        alloc_instance Point
        copy 0
        load_const 10
        store_field .x
        copy 0
        load_const 20
        store_field .y
        alloc_instance Point
        copy 0
        load_const 10
        store_field .x
        copy 0
        load_const 21
        store_field .y
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn deep_equals_nested_objects() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_instance Node
        copy 0
        load_const 2
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_instance Node
        copy 0
        load_const 3
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_array 2
        store_field .children
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_instance Node
        copy 0
        load_const 2
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_instance Node
        copy 0
        load_const 3
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_array 2
        store_field .children
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn deep_equals_nested_objects_different() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_instance Node
        copy 0
        load_const 2
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_instance Node
        copy 0
        load_const 3
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_array 2
        store_field .children
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_instance Node
        copy 0
        load_const 2
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_instance Node
        copy 0
        load_const 4
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        alloc_array 2
        store_field .children
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn deep_equals_with_arrays() {
    let output = baml_test!(
        r#"
        class Container {
            data int[]
        }

        function main() -> bool {
            let c1 = Container { data: [1, 2, 3, 4] };
            let c2 = Container { data: [1, 2, 3, 4] };
            baml.deep_equals(c1, c2)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        alloc_instance Container
        copy 0
        load_const 1
        load_const 2
        load_const 3
        load_const 4
        alloc_array 4
        store_field .data
        alloc_instance Container
        copy 0
        load_const 1
        load_const 2
        load_const 3
        load_const 4
        alloc_array 4
        store_field .data
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn deep_equals_with_maps() {
    let output = baml_test!(
        r#"
        class MapContainer {
            values map<string, int>
        }

        function main() -> bool {
            let m1 = MapContainer { values: {"a": 1, "b": 2} };
            let m2 = MapContainer { values: {"a": 1, "b": 2} };
            baml.deep_equals(m1, m2)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        alloc_instance MapContainer
        copy 0
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        store_field .values
        alloc_instance MapContainer
        copy 0
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        store_field .values
        call baml.deep_equals
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn deep_equals_same_reference() {
    let output = baml_test!(
        r#"
        class Node {
            value int
            children Node[]
        }

        function main() -> bool {
            let n = Node { value: 1, children: [] };
            baml.deep_equals(n, n)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        store_var n
        load_var n
        load_var n
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn deep_equals_circular_structure() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        store_var a1
        load_var a1
        alloc_instance Node
        copy 0
        load_const 2
        store_field .value
        copy 0
        load_var a1
        alloc_array 1
        store_field .children
        alloc_array 1
        store_field .children
        alloc_instance Node
        copy 0
        load_const 1
        store_field .value
        copy 0
        alloc_array 0
        store_field .children
        store_var a2
        load_var a2
        alloc_instance Node
        copy 0
        load_const 2
        store_field .value
        copy 0
        load_var a2
        alloc_array 1
        store_field .children
        alloc_array 1
        store_field .children
        load_var a1
        load_var a2
        call baml.deep_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
