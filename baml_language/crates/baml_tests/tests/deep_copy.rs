//! Unified tests for deep_copy, deep_equals, and ref_equals.

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
        alloc_instance user.Tree
        load_const "1"
        init_field .value
        alloc_instance user.Tree
        load_const "2"
        init_field .value
        alloc_array 0
        init_field .children
        alloc_instance user.Tree
        load_const "3"
        init_field .value
        alloc_array 0
        init_field .children
        alloc_array 2
        init_field .children
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_instance user.Node
        load_const 2
        init_field .value
        alloc_array 0
        init_field .children
        alloc_instance user.Node
        load_const 3
        init_field .value
        alloc_array 0
        init_field .children
        alloc_array 2
        init_field .children
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        alloc_instance user.Matrix
        load_const 1
        load_const 2
        alloc_array 2
        load_const 3
        load_const 4
        alloc_array 2
        alloc_array 2
        init_field .data
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
        alloc_instance user.Container
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        init_field .values
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
        alloc_instance user.Outer
        alloc_instance user.Middle
        alloc_instance user.Inner
        load_const 1
        init_field .value
        init_field .inner
        alloc_instance user.Inner
        load_const 2
        init_field .value
        alloc_instance user.Inner
        load_const 3
        init_field .value
        alloc_array 2
        init_field .list
        init_field .middle
        alloc_instance user.Inner
        load_const 4
        init_field .value
        alloc_instance user.Inner
        load_const 5
        init_field .value
        load_const "first"
        load_const "second"
        alloc_map 2
        init_field .data
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> int {
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_array 0
        init_field .children
        store_var a
        load_var a
        alloc_instance user.Node
        load_const 2
        init_field .value
        load_var a
        alloc_array 1
        init_field .children
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

    insta::assert_snapshot!(output.bytecode, @"
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

    insta::assert_snapshot!(output.bytecode, @"
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Point
        load_const 10
        init_field .x
        load_const 20
        init_field .y
        alloc_instance user.Point
        load_const 10
        init_field .x
        load_const 20
        init_field .y
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Point
        load_const 10
        init_field .x
        load_const 20
        init_field .y
        alloc_instance user.Point
        load_const 10
        init_field .x
        load_const 21
        init_field .y
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_instance user.Node
        load_const 2
        init_field .value
        alloc_array 0
        init_field .children
        alloc_instance user.Node
        load_const 3
        init_field .value
        alloc_array 0
        init_field .children
        alloc_array 2
        init_field .children
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_instance user.Node
        load_const 2
        init_field .value
        alloc_array 0
        init_field .children
        alloc_instance user.Node
        load_const 3
        init_field .value
        alloc_array 0
        init_field .children
        alloc_array 2
        init_field .children
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_instance user.Node
        load_const 2
        init_field .value
        alloc_array 0
        init_field .children
        alloc_instance user.Node
        load_const 3
        init_field .value
        alloc_array 0
        init_field .children
        alloc_array 2
        init_field .children
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_instance user.Node
        load_const 2
        init_field .value
        alloc_array 0
        init_field .children
        alloc_instance user.Node
        load_const 4
        init_field .value
        alloc_array 0
        init_field .children
        alloc_array 2
        init_field .children
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Container
        load_const 1
        load_const 2
        load_const 3
        load_const 4
        alloc_array 4
        init_field .data
        alloc_instance user.Container
        load_const 1
        load_const 2
        load_const 3
        load_const 4
        alloc_array 4
        init_field .data
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
        alloc_instance user.MapContainer
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        init_field .values
        alloc_instance user.MapContainer
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        init_field .values
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_array 0
        init_field .children
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

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_array 0
        init_field .children
        store_var a1
        load_var a1
        alloc_instance user.Node
        load_const 2
        init_field .value
        load_var a1
        alloc_array 1
        init_field .children
        alloc_array 1
        store_field .children
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_array 0
        init_field .children
        store_var a2
        load_var a2
        alloc_instance user.Node
        load_const 2
        init_field .value
        load_var a2
        alloc_array 1
        init_field .children
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

// ============ ref_equals tests ============

#[tokio::test]
async fn ref_equals_same_reference() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function main() -> bool {
            let p = Point { x: 1, y: 2 };
            baml.ref_equals(p, p)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Point
        load_const 1
        init_field .x
        load_const 2
        init_field .y
        store_var p
        load_var p
        load_var p
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn ref_equals_distinct_but_equal_objects() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function main() -> bool {
            let p1 = Point { x: 1, y: 2 };
            let p2 = Point { x: 1, y: 2 };
            baml.ref_equals(p1, p2)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Point
        load_const 1
        init_field .x
        load_const 2
        init_field .y
        alloc_instance user.Point
        load_const 1
        init_field .x
        load_const 2
        init_field .y
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_after_deep_copy() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function main() -> bool {
            let p = Point { x: 1, y: 2 };
            let copy = baml.deep_copy(p);
            // deep_copy must produce a distinct object even though it's value-equal.
            baml.ref_equals(p, copy)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Point
        load_const 1
        init_field .x
        load_const 2
        init_field .y
        store_var p
        load_var p
        call baml.deep_copy
        store_var copy
        load_var p
        load_var copy
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_alias_is_same_reference() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        function main() -> bool {
            let p = Point { x: 1, y: 2 };
            let alias = p;
            baml.ref_equals(p, alias)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Point
        load_const 1
        init_field .x
        load_const 2
        init_field .y
        store_var p
        load_var p
        load_var p
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn ref_equals_nested_field_shared() {
    let output = baml_test!(
        r#"
        class Inner {
            value int
        }

        class Outer {
            a Inner
            b Inner
        }

        function main() -> bool {
            let shared = Inner { value: 42 };
            // Both fields point at the same inner instance.
            let o = Outer { a: shared, b: shared };
            baml.ref_equals(o.a, o.b)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Inner
        load_const 42
        init_field .value
        store_var shared
        alloc_instance user.Outer
        load_var shared
        init_field .a
        load_var shared
        init_field .b
        store_var o
        load_var o
        load_field .a
        load_var o
        load_field .b
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn ref_equals_nested_field_distinct() {
    let output = baml_test!(
        r#"
        class Inner {
            value int
        }

        class Outer {
            a Inner
            b Inner
        }

        function main() -> bool {
            // Two distinct inner instances, even though their contents match.
            let o = Outer { a: Inner { value: 42 }, b: Inner { value: 42 } };
            baml.ref_equals(o.a, o.b)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Outer
        alloc_instance user.Inner
        load_const 42
        init_field .value
        init_field .a
        alloc_instance user.Inner
        load_const 42
        init_field .value
        init_field .b
        store_var o
        load_var o
        load_field .a
        load_var o
        load_field .b
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_primitive_ints() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            // Primitives are by-copy, so even equal ints must compare false.
            baml.ref_equals(42, 42)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        load_const 42
        load_const 42
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_primitive_bools() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            baml.ref_equals(true, true)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        load_const true
        load_const true
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_primitive_nulls() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            baml.ref_equals(null, null)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        load_const null
        load_const null
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_primitive_floats() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            baml.ref_equals(1.5, 1.5)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        load_const 1.5
        load_const 1.5
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_mixed_primitive_and_object() {
    let output = baml_test!(
        r#"
        class Point { x int }

        function main() -> bool {
            let p = Point { x: 1 };
            baml.ref_equals(p, 1)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Point
        load_const 1
        init_field .x
        load_const 1
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_same_array() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let xs = [1, 2, 3];
            baml.ref_equals(xs, xs)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        store_var xs
        load_var xs
        load_var xs
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn ref_equals_distinct_arrays() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a = [1, 2, 3];
            let b = [1, 2, 3];
            baml.ref_equals(a, b)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        load_const 1
        load_const 2
        load_const 3
        alloc_array 3
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_same_map() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let m = {"a": 1, "b": 2};
            baml.ref_equals(m, m)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        store_var m
        load_var m
        load_var m
        call baml.ref_equals
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn ref_equals_distinct_maps() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let m1 = {"a": 1, "b": 2};
            let m2 = {"a": 1, "b": 2};
            baml.ref_equals(m1, m2)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        load_const 1
        load_const 2
        load_const "a"
        load_const "b"
        alloc_map 2
        call baml.ref_equals
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn ref_equals_circular_self_reference() {
    // A pathological case for any recursive equality check: ref_equals must
    // terminate immediately because it never traverses into children.
    let output = baml_test!(
        r#"
        class Node {
            value int
            children Node[]
        }

        function main() -> bool {
            let a = Node { value: 1, children: [] };
            let b = Node { value: 2, children: [a] };
            a.children = [b];

            baml.ref_equals(a, a.children[0].children[0])
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function main() -> bool {
        alloc_instance user.Node
        load_const 1
        init_field .value
        alloc_array 0
        init_field .children
        store_var a
        load_var a
        alloc_instance user.Node
        load_const 2
        init_field .value
        load_var a
        alloc_array 1
        init_field .children
        alloc_array 1
        store_field .children
        load_var a
        load_var a
        load_field .children
        load_const 0
        load_array_element
        load_field .children
        load_const 0
        load_array_element
        call baml.ref_equals
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
