//! Unified tests for class construction, field access, and methods.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use indexmap::indexmap;

// ============================================================================
// Construction
// ============================================================================

#[tokio::test]
async fn class_constructor() {
    let output = baml_test!(
        "
        class Point {
            x int
            y int
        }

        function main() -> Point {
            let p = Point { x: 1, y: 2 };
            p
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Point {
        alloc_instance Point
        copy 0
        load_const 1
        store_field .x
        copy 0
        load_const 2
        store_field .y
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::instance(
            "Point",
            indexmap! {
                "x" => BexExternalValue::Int(1),
                "y" => BexExternalValue::Int(2),
            }
        ))
    );
}

#[tokio::test]
async fn class_constructor_return_directly() {
    let output = baml_test!(
        "
        class Point {
            x int
            y int
        }

        function main() -> Point {
            Point { x: 1, y: 2 }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Point {
        alloc_instance Point
        copy 0
        load_const 1
        store_field .x
        copy 0
        load_const 2
        store_field .y
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::instance(
            "Point",
            indexmap! {
                "x" => BexExternalValue::Int(1),
                "y" => BexExternalValue::Int(2),
            }
        ))
    );
}

#[tokio::test]
async fn constructor_with_preceding_variables() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance MyClass
        copy 0
        load_const 100
        store_field .x
        copy 0
        load_const 200
        store_field .y
        store_var obj
        load_var obj
        load_field .x
        load_var obj
        load_field .y
        bin_op +
        load_const 10
        bin_op +
        load_const 20
        bin_op +
        load_const 30
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(360)));
}

// ============================================================================
// Nested construction
// ============================================================================

#[tokio::test]
async fn nested_construction_dead_store() {
    let output = baml_test!(
        "
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
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 42
        store_field .value
        store_field .inner
        store_var o
        load_const 42
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn nested_construction_field_access() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 10
        store_field .x
        copy 0
        load_const 20
        store_field .y
        store_field .inner
        copy 0
        load_const 30
        store_field .value
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

#[tokio::test]
async fn nested_construction_inner_field_access() {
    let output = baml_test!(
        r#"
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
            o.inner.y
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 10
        store_field .x
        copy 0
        load_const 20
        store_field .y
        store_field .inner
        copy 0
        load_const 30
        store_field .value
        load_field .inner
        load_field .y
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(20)));
}

#[tokio::test]
async fn nested_field_read() {
    let output = baml_test!(
        "
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
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 42
        store_field .value
        store_field .inner
        load_field .inner
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn nested_field_read_separate_construction() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 42
        store_field .value
        store_field .inner
        load_field .inner
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn nested_constructor_with_preceding_variables() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 100
        store_field .val
        store_field .inner
        copy 0
        load_const 50
        store_field .x
        store_var obj
        load_var obj
        load_field .inner
        load_field .val
        load_var obj
        load_field .x
        bin_op +
        load_const 5
        bin_op +
        load_const 10
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(165)));
}

// ============================================================================
// Spread operator
// ============================================================================

#[tokio::test]
async fn spread_before_named_fields() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function default_point() -> Point {
        alloc_instance Point
        copy 0
        load_const 0
        store_field .x
        copy 0
        load_const 0
        store_field .y
        copy 0
        load_const 0
        store_field .z
        copy 0
        load_const 0
        store_field .w
        return
    }

    function main() -> Point {
        call default_point
        store_var _2
        alloc_instance Point
        copy 0
        load_const 1
        store_field .x
        copy 0
        load_const 2
        store_field .y
        copy 0
        load_var _2
        load_field .z
        store_field .z
        copy 0
        load_var _2
        load_field .w
        store_field .w
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::instance(
            "Point",
            indexmap! {
                "x" => BexExternalValue::Int(1),
                "y" => BexExternalValue::Int(2),
                "z" => BexExternalValue::Int(0),
                "w" => BexExternalValue::Int(0),
            }
        ))
    );
}

#[tokio::test]
async fn spread_after_named_fields() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function default_point() -> Point {
        alloc_instance Point
        copy 0
        load_const 0
        store_field .x
        copy 0
        load_const 0
        store_field .y
        copy 0
        load_const 0
        store_field .z
        copy 0
        load_const 0
        store_field .w
        return
    }

    function main() -> Point {
        call default_point
        store_var _2
        alloc_instance Point
        copy 0
        load_var _2
        load_field .x
        store_field .x
        copy 0
        load_var _2
        load_field .y
        store_field .y
        copy 0
        load_var _2
        load_field .z
        store_field .z
        copy 0
        load_var _2
        load_field .w
        store_field .w
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::instance(
            "Point",
            indexmap! {
                "x" => BexExternalValue::Int(0),
                "y" => BexExternalValue::Int(0),
                "z" => BexExternalValue::Int(0),
                "w" => BexExternalValue::Int(0),
            }
        ))
    );
}

#[tokio::test]
async fn multiple_spreads() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> Point {
        call x_one
        pop 1
        call xy_one
        store_var _4
        alloc_instance Point
        copy 0
        load_var _4
        load_field .x
        store_field .x
        copy 0
        load_var _4
        load_field .y
        store_field .y
        copy 0
        load_var _4
        load_field .z
        store_field .z
        copy 0
        load_var _4
        load_field .w
        store_field .w
        return
    }

    function x_one() -> Point {
        alloc_instance Point
        copy 0
        load_const 1
        store_field .x
        copy 0
        load_const 0
        store_field .y
        copy 0
        load_const 0
        store_field .z
        copy 0
        load_const 0
        store_field .w
        return
    }

    function xy_one() -> Point {
        alloc_instance Point
        copy 0
        load_const 1
        store_field .x
        copy 0
        load_const 1
        store_field .y
        copy 0
        load_const 0
        store_field .z
        copy 0
        load_const 0
        store_field .w
        return
    }
    ");

    assert_eq!(
        output.result,
        Ok(BexExternalValue::instance(
            "Point",
            indexmap! {
                "x" => BexExternalValue::Int(1),
                "y" => BexExternalValue::Int(1),
                "z" => BexExternalValue::Int(0),
                "w" => BexExternalValue::Int(0),
            }
        ))
    );
}

#[tokio::test]
async fn spread_does_not_break_locals() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function default_point() -> Point {
        alloc_instance Point
        copy 0
        load_const 0
        store_field .x
        copy 0
        load_const 0
        store_field .y
        copy 0
        load_const 0
        store_field .z
        copy 0
        load_const 0
        store_field .w
        return
    }

    function main() -> int {
        call default_point
        store_var _2
        alloc_instance Point
        copy 0
        load_var _2
        load_field .x
        store_field .x
        copy 0
        load_var _2
        load_field .y
        store_field .y
        copy 0
        load_var _2
        load_field .z
        store_field .z
        copy 0
        load_var _2
        load_field .w
        store_field .w
        store_var p
        load_const 0
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// ============================================================================
// Field assignment
// ============================================================================

#[tokio::test]
async fn field_assignment() {
    let output = baml_test!(
        "
        class Data {
            value int
        }

        function main() -> int {
            let d = Data { value: 0 };
            d.value = 42;
            d.value
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Data
        copy 0
        load_const 0
        store_field .value
        store_var d
        load_var d
        load_const 42
        store_field .value
        load_var d
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn field_compound_assignment() {
    let output = baml_test!(
        "
        class Counter {
            value int
        }

        function main() -> int {
            let c = Counter { value: 5 };
            c.value += 10;
            c.value
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Counter
        copy 0
        load_const 5
        store_field .value
        store_var c
        load_var c
        load_var c
        load_field .value
        load_const 10
        bin_op +
        store_field .value
        load_var c
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

#[tokio::test]
async fn nested_field_assignment() {
    let output = baml_test!(
        "
        class Inner {
            value int
        }
        class Outer {
            inner Inner
        }

        function main() -> int {
            let o = Outer { inner: Inner { value: 0 } };
            o.inner.value = 99;
            o.inner.value
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 0
        store_field .value
        store_field .inner
        store_var o
        load_var o
        load_field .inner
        load_const 99
        store_field .value
        load_var o
        load_field .inner
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(99)));
}

#[tokio::test]
async fn nested_field_compound_assignment() {
    let output = baml_test!(
        "
        class Inner {
            value int
        }
        class Outer {
            inner Inner
        }

        function main() -> int {
            let o = Outer { inner: Inner { value: 5 } };
            o.inner.value += 10;
            o.inner.value
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 5
        store_field .value
        store_field .inner
        store_var o
        load_var o
        load_field .inner
        load_var o
        load_field .inner
        load_field .value
        load_const 10
        bin_op +
        store_field .value
        load_var o
        load_field .inner
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

// ============================================================================
// Methods
// ============================================================================

#[tokio::test]
async fn method_call() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function Number.add(self: Number, other: Number) -> Number {
        alloc_instance Number
        copy 0
        load_var self
        load_field .value
        load_var other
        load_field .value
        bin_op +
        store_field .value
        return
    }

    function main() -> int {
        alloc_instance Number
        copy 0
        load_const 1
        store_field .value
        alloc_instance Number
        copy 0
        load_const 2
        store_field .value
        call Number.add
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn mutable_self_method() {
    let output = baml_test!(
        r#"
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
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function Number.add(self: Number, other: Number) -> bool {
        load_var self
        load_var self
        load_field .value
        load_var other
        load_field .value
        bin_op +
        store_field .value
        load_const true
        return
    }

    function main() -> int {
        alloc_instance Number
        copy 0
        load_const 1
        store_field .value
        store_var a
        load_var a
        alloc_instance Number
        copy 0
        load_const 2
        store_field .value
        call Number.add
        pop 1
        load_var a
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}
