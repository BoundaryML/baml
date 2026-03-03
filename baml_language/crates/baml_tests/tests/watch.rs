//! Unified tests for watch functionality and viz headers.
//!
//! TODO: Notification assertions are documented as comments only. Once
//! `BexEngine` plumbs `VmExecState::Notify` through to callers, revisit
//! these tests to assert on `output.notifications`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// Watch primitive
// ============================================================================

#[tokio::test]
async fn watch_primitive() {
    // Expected notifications: [["value"]]
    // (one notification event: channel "value" fires when value = 1)
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let value = 0;
            value = 1;
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const 1
        store_var value
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_primitive_nested_scope() {
    // Expected notifications: [["value"]]
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let value = 0;
            if (true) {
                value = 1;
            }
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const true
        pop_jump_if_false L0
        load_const 1
        store_var value

      L0:
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_default_filter() {
    // Expected notifications: [["value"]]
    // (value = 0 is no-op (same value), value = 6 triggers notification)
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let value = 0;
            value = 0;
            value = 6;
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const 0
        store_var value
        load_const 6
        store_var value
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn watch_user_filter() {
    // Expected notifications: [["value"]]
    // (value = 1 filtered out by greater_than_five, value = 6 passes)
    let output = baml_test!(
        r#"
        function greater_than_five(value: int) -> bool {
            value > 5
        }

        function main() -> int {
            watch let value = 0;
            value.$watch.options(baml.WatchOptions { when: greater_than_five });
            value = 1;
            value = 6;
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function greater_than_five(value: int) -> bool {
        load_var value
        load_const 5
        cmp_op >
        return
    }

    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const "value"
        load_global greater_than_five
        watch value
        load_const 1
        store_var value
        load_const 6
        store_var value
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn watch_manual_notify() {
    // Expected notifications: [["value"]]
    // (assignments don't notify in manual mode, only explicit $watch.notify() does)
    let output = baml_test!(
        r#"
        function main() -> int {
            watch let value = 0;
            value.$watch.options(baml.WatchOptions { when: "manual" });
            value = 1;
            value = 2;
            value = 3;
            value.$watch.notify();
            value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const 0
        store_var value
        load_const "value"
        load_const null
        watch value
        load_const "value"
        load_const "manual"
        watch value
        load_const 1
        store_var value
        load_const 2
        store_var value
        load_const 3
        store_var value
        notify value
        load_var value
        unwatch value
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// Watch with aliases and scope exit
// ============================================================================

#[tokio::test]
async fn watch_alias() {
    // Expected notifications: [["point"]]
    // (alias.x = 1 notifies on channel "point")
    let output = baml_test!(
        r#"
        class Point { x int  y int }

        function main() -> int {
            watch let point = Point { x: 0, y: 0 };
            let alias = point;
            alias.x = 1;
            point.x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance Point
        copy 0
        load_const 0
        store_field .x
        copy 0
        load_const 0
        store_field .y
        store_var point
        load_const "point"
        load_const null
        watch point
        load_var point
        load_const 1
        store_field .x
        load_var point
        load_field .x
        unwatch point
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_alias_nested_scope() {
    // Expected notifications: [["point"]]
    let output = baml_test!(
        r#"
        class Point { x int  y int }

        function main() -> int {
            watch let point = Point { x: 0, y: 0 };
            if (true) {
                let alias = point;
                alias.x = 1;
            }
            point.x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance Point
        copy 0
        load_const 0
        store_field .x
        copy 0
        load_const 0
        store_field .y
        store_var point
        load_const "point"
        load_const null
        watch point
        load_const true
        pop_jump_if_false L0
        load_var point
        load_const 1
        store_field .x

      L0:
        load_var point
        load_field .x
        unwatch point
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn watch_scope_exit() {
    // Expected notifications: [["point"]]
    // (point.x = 1 inside block notifies, outter_point.x = 2 after scope exit does not)
    let output = baml_test!(
        r#"
        class Point { x int  y int }

        function main() -> int {
            let outter_point = {
                watch let point = Point { x: 0, y: 0 };
                point.x = 1;
                point
            };
            outter_point.x = 2;
            outter_point.x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance Point
        copy 0
        load_const 0
        store_field .x
        copy 0
        load_const 0
        store_field .y
        store_var point
        load_const "point"
        load_const null
        watch point
        load_var point
        load_const 1
        store_field .x
        load_var point
        store_var outter_point
        unwatch point
        load_var outter_point
        load_const 2
        store_field .x
        load_var outter_point
        load_field .x
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

// ============================================================================
// Watch with function calls and nested objects
// ============================================================================

#[tokio::test]
async fn watch_function_call_modifications() {
    // Expected notifications: [["point"], ["point"]]
    // (self.x = x and self.y = y each trigger a notification)
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int

            function set(self, x: int, y: int) -> Point {
                self.x = x;
                self.y = y;
                self
            }
        }

        function main() -> int {
            watch let point = Point { x: 0, y: 0 };
            point.set(1, 2);
            point.x + point.y
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function Point.set(self: Point, x: int, y: int) -> Point {
        load_var self
        load_var x
        store_field .x
        load_var self
        load_var y
        store_field .y
        load_var self
        return
    }

    function main() -> int {
        alloc_instance Point
        copy 0
        load_const 0
        store_field .x
        copy 0
        load_const 0
        store_field .y
        store_var point
        load_const "point"
        load_const null
        watch point
        load_var point
        load_const 1
        load_const 2
        call Point.set
        pop 1
        load_var point
        load_field .x
        load_var point
        load_field .y
        bin_op +
        unwatch point
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn watch_nested_object_added() {
    // Expected notifications: [["vec"], ["vec"]]
    // (vec.p = p notifies, then p.x.value = 2 also notifies because p is now part of vec)
    let output = baml_test!(
        r#"
        class Value { value int }
        class Point { x Value  y Value }
        class Vec2D { p Point  q Point }

        function main() -> int {
            watch let vec = Vec2D {
                p: Point { x: Value { value: 0 }, y: Value { value: 0 } },
                q: Point { x: Value { value: 0 }, y: Value { value: 0 } },
            };
            let p = Point { x: Value { value: 1 }, y: Value { value: 1 } };
            vec.p = p;
            p.x.value = 2;
            vec.p.x.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance Vec2D
        copy 0
        alloc_instance Point
        copy 0
        alloc_instance Value
        copy 0
        load_const 0
        store_field .value
        store_field .x
        copy 0
        alloc_instance Value
        copy 0
        load_const 0
        store_field .value
        store_field .y
        store_field .p
        copy 0
        alloc_instance Point
        copy 0
        alloc_instance Value
        copy 0
        load_const 0
        store_field .value
        store_field .x
        copy 0
        alloc_instance Value
        copy 0
        load_const 0
        store_field .value
        store_field .y
        store_field .q
        store_var vec
        load_const "vec"
        load_const null
        watch vec
        alloc_instance Point
        copy 0
        alloc_instance Value
        copy 0
        load_const 1
        store_field .value
        store_field .x
        copy 0
        alloc_instance Value
        copy 0
        load_const 1
        store_field .value
        store_field .y
        store_var p
        load_var vec
        load_var p
        store_field .p
        load_var p
        load_field .x
        load_const 2
        store_field .value
        load_var vec
        load_field .p
        load_field .x
        load_field .value
        unwatch vec
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn watch_nested_object_removed() {
    // Expected notifications: [["vec"]]
    // (vec.p = <new Point> notifies, then p.x.value = 2 does NOT notify because p
    //  was detached from vec)
    let output = baml_test!(
        r#"
        class Value { value int }
        class Point { x Value  y Value }
        class Vec2D { p Point  q Point }

        function main() -> int {
            watch let vec = Vec2D {
                p: Point { x: Value { value: 0 }, y: Value { value: 0 } },
                q: Point { x: Value { value: 0 }, y: Value { value: 0 } },
            };
            let p = vec.p;
            vec.p = Point { x: Value { value: 1 }, y: Value { value: 1 } };
            p.x.value = 2;
            vec.p.x.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance Vec2D
        copy 0
        alloc_instance Point
        copy 0
        alloc_instance Value
        copy 0
        load_const 0
        store_field .value
        store_field .x
        copy 0
        alloc_instance Value
        copy 0
        load_const 0
        store_field .value
        store_field .y
        store_field .p
        copy 0
        alloc_instance Point
        copy 0
        alloc_instance Value
        copy 0
        load_const 0
        store_field .value
        store_field .x
        copy 0
        alloc_instance Value
        copy 0
        load_const 0
        store_field .value
        store_field .y
        store_field .q
        store_var vec
        load_const "vec"
        load_const null
        watch vec
        load_var vec
        load_field .p
        store_var p
        load_var vec
        alloc_instance Point
        copy 0
        alloc_instance Value
        copy 0
        load_const 1
        store_field .value
        store_field .x
        copy 0
        alloc_instance Value
        copy 0
        load_const 1
        store_field .value
        store_field .y
        store_field .p
        load_var p
        load_field .x
        load_const 2
        store_field .value
        load_var vec
        load_field .p
        load_field .x
        load_field .value
        unwatch vec
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

// ============================================================================
// Cyclic graph
// ============================================================================

#[tokio::test]
async fn watch_cyclic_graph() {
    // Expected notifications:
    //   v2.edges = [v3]       -> [["v2"]]
    //   v3.edges = [v4]       -> [["v2"]]
    //   v4.edges = [v1]       -> [["v2", "v4"]]
    //   v2.value = 20         -> [["v2", "v4"]]
    //   v1.value = 10         -> [["v2", "v4"]]
    //   v3.value = 30         -> [["v2", "v4"]]
    let output = baml_test!(
        r#"
        class Vertex {
            edges Vertex[]
            value int
        }

        function main() -> int {
            let v1 = Vertex { value: 1, edges: [] };
            watch let v2 = Vertex { value: 2, edges: [] };
            let v3 = Vertex { value: 3, edges: [] };
            watch let v4 = Vertex { value: 4, edges: [] };

            v1.edges = [v2];
            v2.edges = [v3];
            v3.edges = [v4];
            v4.edges = [v1];

            v2.value = 20;
            v1.value = 10;
            v3.value = 30;

            0
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        alloc_instance Vertex
        copy 0
        load_const 1
        store_field .edges
        copy 0
        alloc_array 0
        store_field .value
        store_var v1
        alloc_instance Vertex
        copy 0
        load_const 2
        store_field .edges
        copy 0
        alloc_array 0
        store_field .value
        store_var v2
        load_const "v2"
        load_const null
        watch v2
        alloc_instance Vertex
        copy 0
        load_const 3
        store_field .edges
        copy 0
        alloc_array 0
        store_field .value
        store_var v3
        alloc_instance Vertex
        copy 0
        load_const 4
        store_field .edges
        copy 0
        alloc_array 0
        store_field .value
        store_var v4
        load_const "v4"
        load_const null
        watch v4
        load_var v1
        load_var v2
        alloc_array 1
        store_field .edges
        load_var v2
        load_var v3
        alloc_array 1
        store_field .edges
        load_var v3
        load_var v4
        alloc_array 1
        store_field .edges
        load_var v4
        load_var v1
        alloc_array 1
        store_field .edges
        load_var v2
        load_const 20
        store_field .value
        load_var v1
        load_const 10
        store_field .value
        load_var v3
        load_const 30
        store_field .value
        unwatch v4
        unwatch v2
        load_const 0
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// ============================================================================
// Block notifications (//# headers)
// ============================================================================

#[tokio::test]
async fn block_notification_basic() {
    // Expected notifications: [Block("test_blocks", "entering_computation", Statement, enter)]
    let output = baml_test!(
        r#"
        function main() -> int {
            //# entering_computation
            let x = 1;
            let y = 2;
            x + y
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        notify_block entering_computation
        load_const 1
        load_const 2
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn block_notification_multiple() {
    // Expected notifications:
    //   [Block("test_multiple_blocks", "first_block", Statement, enter)]
    //   [Block("test_multiple_blocks", "second_block", Statement, enter)]
    let output = baml_test!(
        r#"
        function main() -> int {
            //# first_block
            let x = 1;
            //# second_block
            let y = 2;
            x + y
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        notify_block first_block
        notify_block second_block
        load_const 1
        load_const 2
        bin_op +
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// VizEnter/VizExit (//# header before control flow)
// ============================================================================

#[tokio::test]
async fn viz_header_before_if() {
    // Expected notifications:
    //   Block("header_before_if", "MyHeader", Statement, enter)
    //   Viz("header_before_if", "MyHeader", enter)
    //   Viz("header_before_if", "MyHeader", exit)
    let output = baml_test! {
        baml: r#"
            function header_before_if() -> int {
                //# MyHeader
                if (true) {
                    1
                } else {
                    2
                }
            }
        "#,
        entry: "header_before_if",
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function header_before_if() -> int {
        notify_block MyHeader
        viz_enter MyHeader
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        viz_exit MyHeader
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn viz_header_before_while() {
    // Expected notifications:
    //   Block("header_before_while", "LoopHeader", Statement, enter)
    //   Viz("header_before_while", "LoopHeader", enter)
    //   Viz("header_before_while", "LoopHeader", exit)
    let output = baml_test! {
        baml: r#"
            function header_before_while() -> int {
                let x = 0;
                //# LoopHeader
                while (x < 1) {
                    x = x + 1;
                }
                x
            }
        "#,
        entry: "header_before_while",
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function header_before_while() -> int {
        load_const 0
        store_var x
        notify_block LoopHeader
        viz_enter LoopHeader

      L0:
        load_var x
        load_const 1
        cmp_op <
        pop_jump_if_false L1
        jump L2

      L1:
        viz_exit LoopHeader
        load_var x
        return

      L2:
        load_var x
        load_const 1
        bin_op +
        store_var x
        jump L0
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn viz_standalone_header_no_viz_events() {
    // Expected notifications: [Block("standalone_header", "JustAHeader", Statement, enter)]
    // (no VizEnter/VizExit because header is not before control flow)
    let output = baml_test! {
        baml: r#"
            function standalone_header() -> int {
                //# JustAHeader
                let x = 5;
                x
            }
        "#,
        entry: "standalone_header",
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function standalone_header() -> int {
        notify_block JustAHeader
        load_const 5
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn viz_multiple_headers_only_one_before_if() {
    // Expected notifications:
    //   Block("multiple_headers", "FirstHeader", Statement, enter)
    //   Block("multiple_headers", "SecondHeader", Statement, enter)
    //   Viz("multiple_headers", "SecondHeader", enter)   (only SecondHeader precedes if)
    //   Viz("multiple_headers", "SecondHeader", exit)
    let output = baml_test! {
        baml: r#"
            function multiple_headers() -> int {
                //# FirstHeader
                let x = 1;
                //# SecondHeader
                if (x > 0) {
                    2
                } else {
                    3
                }
            }
        "#,
        entry: "multiple_headers",
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function multiple_headers() -> int {
        notify_block FirstHeader
        notify_block SecondHeader
        viz_enter SecondHeader
        load_const 1
        load_const 0
        cmp_op >
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 3
        jump L2

      L1:
        load_const 2

      L2:
        viz_exit SecondHeader
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn viz_if_without_header_no_viz() {
    // Expected notifications: [] (no header, no viz events)
    let output = baml_test! {
        baml: r#"
            function if_no_header() -> int {
                if (true) {
                    1
                } else {
                    2
                }
            }
        "#,
        entry: "if_no_header",
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function if_no_header() -> int {
        load_const true
        pop_jump_if_false L0
        jump L1

      L0:
        load_const 2
        jump L2

      L1:
        load_const 1

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}
