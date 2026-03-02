//! Unified tests for assignments and field mutations.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn block_expr() {
    let output = baml_test!(
        "
        function main() -> int {
            let a = {
                let b = 1;
                b
            };
            a
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 1
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn mutable_var_in_function() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let y = 3;
            y = 5;
            y
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        load_const 3
        store_var y
        load_const 5
        store_var y
        load_var y
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn mutable_param() {
    let output = baml_test!(
        r#"
        function MutableInArg(x: int) -> int {
            x = 3;
            x
        }

        function main() -> int {
            let r = MutableInArg(42);
            r
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function MutableInArg(x: int) -> int {
        load_const 3
        store_var x
        load_var x
        return
    }

    function main() -> int {
        load_const 42
        call MutableInArg
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn virtual_cross_block_soundness() {
    let output = baml_test! {
        baml: r#"
        function main(c: bool) -> int {
            let a = 1;
            let b = a;
            if (c) {
                a = 2;
            }
            b
        }

        function entry() -> int {
            main(true)
        }
    "#,
        entry: "entry"
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function entry() -> int {
        load_const true
        call main
        return
    }

    function main(c: bool) -> int {
        load_const 1
        store_var a
        load_var a
        store_var b
        load_var c
        pop_jump_if_false L0
        load_const 2
        store_var a

      L0:
        load_var b
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn virtual_cross_block_param_mutation_soundness() {
    let output = baml_test! {
        baml: r#"
        function main(c: bool, p: int) -> int {
            let x = p;
            if (c) {
                p = 2;
            }
            x
        }

        function entry() -> int {
            main(true, 42)
        }
    "#,
        entry: "entry"
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function entry() -> int {
        load_const true
        load_const 42
        call main
        return
    }

    function main(c: bool, p: int) -> int {
        load_var p
        store_var x
        load_var c
        pop_jump_if_false L0
        load_const 2
        store_var p

      L0:
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn copy_of_mutable_param_soundness() {
    let output = baml_test! {
        baml: r#"
        function main(x: int) -> int {
            let y = x;
            x = 2;
            y
        }

        function entry() -> int {
            main(42)
        }
    "#,
        entry: "entry"
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function entry() -> int {
        load_const 42
        call main
        return
    }

    function main(x: int) -> int {
        load_var x
        store_var y
        load_const 2
        store_var x
        load_var y
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn virtual_cross_block_transitive_param_mutation_soundness() {
    let output = baml_test! {
        baml: r#"
        function main(c: bool, p: int) -> int {
            let t = p;
            let x = t;
            if (c) {
                p = 2;
            }
            x
        }

        function entry() -> int {
            main(true, 42)
        }
    "#,
        entry: "entry"
    };

    insta::assert_snapshot!(output.bytecode, @r"
    function entry() -> int {
        load_const true
        load_const 42
        call main
        return
    }

    function main(c: bool, p: int) -> int {
        load_var p
        store_var x
        load_var c
        pop_jump_if_false L0
        load_const 2
        store_var p

      L0:
        load_var x
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn field_assignment_add_assign() {
    let output = baml_test!(
        r#"
        class Counter {
            value int
        }
        function main() -> int {
            let c = Counter { value: 10 };
            c.value += 5;
            c.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Counter
        copy 0
        load_const 10
        store_field .value
        store_var c
        load_var c
        load_var c
        load_field .value
        load_const 5
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
async fn field_assignment_sub_assign() {
    let output = baml_test!(
        r#"
        class Counter {
            value int
        }
        function main() -> int {
            let c = Counter { value: 20 };
            c.value -= 8;
            c.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Counter
        copy 0
        load_const 20
        store_field .value
        store_var c
        load_var c
        load_var c
        load_field .value
        load_const 8
        bin_op -
        store_field .value
        load_var c
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(12)));
}

#[tokio::test]
async fn field_assignment_mul_assign() {
    let output = baml_test!(
        r#"
        class Counter {
            value int
        }
        function main() -> int {
            let c = Counter { value: 7 };
            c.value *= 3;
            c.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Counter
        copy 0
        load_const 7
        store_field .value
        store_var c
        load_var c
        load_var c
        load_field .value
        load_const 3
        bin_op *
        store_field .value
        load_var c
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(21)));
}

#[tokio::test]
async fn field_assignment_div_assign() {
    let output = baml_test!(
        r#"
        class Counter {
            value int
        }
        function main() -> int {
            let c = Counter { value: 24 };
            c.value /= 4;
            c.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Counter
        copy 0
        load_const 24
        store_field .value
        store_var c
        load_var c
        load_var c
        load_field .value
        load_const 4
        bin_op /
        store_field .value
        load_var c
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn field_assignment_mod_assign() {
    let output = baml_test!(
        r#"
        class Counter {
            value int
        }
        function main() -> int {
            let c = Counter { value: 17 };
            c.value %= 5;
            c.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Counter
        copy 0
        load_const 17
        store_field .value
        store_var c
        load_var c
        load_var c
        load_field .value
        load_const 5
        bin_op %
        store_field .value
        load_var c
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn field_assignment_simple() {
    let output = baml_test!(
        r#"
        class Data {
            value int
            active bool
        }
        function main() -> int {
            let d = Data { value: 100, active: true };
            d.value = 42;
            d.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Data
        copy 0
        load_const 100
        store_field .value
        copy 0
        load_const true
        store_field .active
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
async fn field_assignment_multiple_ops() {
    let output = baml_test!(
        r#"
        class Stats {
            score int
        }
        function main() -> int {
            let s = Stats { score: 10 };
            s.score += 5;   // 15
            s.score *= 2;   // 30
            s.score -= 10;  // 20
            s.score /= 4;   // 5
            s.score
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Stats
        copy 0
        load_const 10
        store_field .score
        store_var s
        load_var s
        load_var s
        load_field .score
        load_const 5
        bin_op +
        store_field .score
        load_var s
        load_var s
        load_field .score
        load_const 2
        bin_op *
        store_field .score
        load_var s
        load_var s
        load_field .score
        load_const 10
        bin_op -
        store_field .score
        load_var s
        load_var s
        load_field .score
        load_const 4
        bin_op /
        store_field .score
        load_var s
        load_field .score
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn nested_field_assignment_simple() {
    let output = baml_test!(
        r#"
        class Inner {
            value int
        }
        class Outer {
            inner Inner
        }
        function main() -> int {
            let i = Inner { value: 10 };
            let o = Outer { inner: i };
            o.inner.value = 42;
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
        load_const 10
        store_field .value
        store_field .inner
        store_var o
        load_var o
        load_field .inner
        load_const 42
        store_field .value
        load_var o
        load_field .inner
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn nested_field_assignment_compound() {
    let output = baml_test!(
        r#"
        class Inner {
            value int
        }
        class Outer {
            inner Inner
        }
        function main() -> int {
            let i = Inner { value: 10 };
            let o = Outer { inner: i };
            o.inner.value += 32;
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
        load_const 10
        store_field .value
        store_field .inner
        store_var o
        load_var o
        load_field .inner
        load_var o
        load_field .inner
        load_field .value
        load_const 32
        bin_op +
        store_field .value
        load_var o
        load_field .inner
        load_field .value
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn field_assignment_object_field() {
    let output = baml_test!(
        r#"
        class Inner {
            value int
        }
        class Outer {
            inner Inner
        }
        function main() -> bool {
            let o = Outer { inner: Inner { value: 10 } };
            o.inner = Inner { value: 20 };
            // For now, test that assignment works, not nested field access
            true
        }"#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> bool {
        alloc_instance Outer
        copy 0
        alloc_instance Inner
        copy 0
        load_const 10
        store_field .value
        store_field .inner
        alloc_instance Inner
        copy 0
        load_const 20
        store_field .value
        store_field .inner
        load_const true
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn array_element_field_assignment() {
    let output = baml_test!(
        r#"
        class Item {
            count int
        }

        function main() -> int {
            let items = [
                Item { count: 10 },
                Item { count: 20 },
                Item { count: 30 }
            ];

            // Modify field of array element
            items[1].count += 5;
            items[1].count
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Item
        copy 0
        load_const 10
        store_field .count
        alloc_instance Item
        copy 0
        load_const 20
        store_field .count
        alloc_instance Item
        copy 0
        load_const 30
        store_field .count
        alloc_array 3
        store_var items
        load_var items
        load_const 1
        load_array_element
        load_var items
        load_const 1
        load_array_element
        load_field .count
        load_const 5
        bin_op +
        store_field .count
        load_var items
        load_const 1
        load_array_element
        load_field .count
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(25)));
}

#[tokio::test]
async fn array_element_method_field_assignment() {
    let output = baml_test!(
        r#"
        class Data {
            value int

            function get_self(self) -> Data {
                self
            }
        }

        class Container {
            data Data

            function get_data(self) -> Data {
                self.data
            }
        }

        function main() -> int {
            let containers = [
                Container { data: Data { value: 10 } },
                Container { data: Data { value: 20 } },
                Container { data: Data { value: 30 } }
            ];

            // First test: Can we modify array element's field?
            containers[1].data.value += 5;
            let result1 = containers[1].data.value; // Should be 25

            // Test method call assignment:
            containers[1].get_data().value += 10;
            let result2 = containers[1].data.value; // Should be 35 (25 + 10)

            result2
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function Container.get_data(self: Container) -> Data {
        load_var self
        load_field .data
        return
    }

    function Data.get_self(self: Data) -> Data {
        load_var self
        return
    }

    function main() -> int {
        alloc_instance Container
        copy 0
        alloc_instance Data
        copy 0
        load_const 10
        store_field .value
        store_field .data
        alloc_instance Container
        copy 0
        alloc_instance Data
        copy 0
        load_const 20
        store_field .value
        store_field .data
        alloc_instance Container
        copy 0
        alloc_instance Data
        copy 0
        load_const 30
        store_field .value
        store_field .data
        alloc_array 3
        store_var containers
        load_var containers
        load_const 1
        load_array_element
        load_field .data
        load_var containers
        load_const 1
        load_array_element
        load_field .data
        load_field .value
        load_const 5
        bin_op +
        store_field .value
        load_var containers
        load_const 1
        load_array_element
        load_field .data
        load_field .value
        store_var result1
        load_var containers
        load_const 1
        load_array_element
        call Container.get_data
        store_var _22
        load_var _22
        load_var _22
        load_field .value
        load_const 10
        bin_op +
        store_field .value
        load_var containers
        load_const 1
        load_array_element
        load_field .data
        load_field .value
        return
    }

    function stream_Container.get_data(self: stream_Container) -> Data {
        load_var self
        load_field .data
        return
    }

    function stream_Data.get_self(self: stream_Data) -> Data {
        load_var self
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(35)));
}

#[tokio::test]
async fn method_call_then_array_access_assignment() {
    let output = baml_test!(
        r#"
        class Item {
            value int
        }
        class Container {
            data Item[]
            function get_nested(self) -> Item[] {
                self.data
            }
        }
        function main() -> int {
            let i1 = Item { value: 10 };
            let i2 = Item { value: 20 };
            let i3 = Item { value: 30 };
            let arr = [i1, i2, i3];
            let obj = Container { data: arr };
            obj.get_nested()[1].value += 5;
            obj.data[1].value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function Container.get_nested(self: Container) -> Item[] {
        load_var self
        load_field .data
        return
    }

    function main() -> int {
        alloc_instance Container
        copy 0
        alloc_instance Item
        copy 0
        load_const 10
        store_field .value
        alloc_instance Item
        copy 0
        load_const 20
        store_field .value
        alloc_instance Item
        copy 0
        load_const 30
        store_field .value
        alloc_array 3
        store_field .data
        store_var obj
        load_var obj
        call Container.get_nested
        store_var _14
        load_var _14
        load_const 1
        load_array_element
        load_var _14
        load_const 1
        load_array_element
        load_field .value
        load_const 5
        bin_op +
        store_field .value
        load_var obj
        load_field .data
        load_const 1
        load_array_element
        load_field .value
        return
    }

    function stream_Container.get_nested(self: stream_Container) -> Item[] {
        load_var self
        load_field .data
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(25)));
}

#[tokio::test]
async fn method_call_field_assignment() {
    let output = baml_test!(
        r#"
        class Counter {
            value int
        }

        class Factory {
            counter Counter

            function get_counter(self) -> Counter {
                self.counter
            }
        }

        function main() -> int {
            let f = Factory {
                counter: Counter { value: 10 }
            };

            f.get_counter().value += 5;

            f.get_counter().value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function Factory.get_counter(self: Factory) -> Counter {
        load_var self
        load_field .counter
        return
    }

    function main() -> int {
        alloc_instance Factory
        copy 0
        alloc_instance Counter
        copy 0
        load_const 10
        store_field .value
        store_field .counter
        store_var f
        load_var f
        call Factory.get_counter
        store_var _5
        load_var _5
        load_var _5
        load_field .value
        load_const 5
        bin_op +
        store_field .value
        load_var f
        call Factory.get_counter
        load_field .value
        return
    }

    function stream_Factory.get_counter(self: stream_Factory) -> Counter {
        load_var self
        load_field .counter
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

#[tokio::test]
async fn method_call_field_assignment_with_copy() {
    let output = baml_test!(
        r#"
        class Counter {
            value int
        }

        class Factory {
            counter Counter

            function get_counter(self) -> Counter {
                self.counter
            }
        }

        function main() -> int {
            let f = Factory {
                counter: Counter { value: 10 }
            };

            let c = f.get_counter();

            c.value += 5;

            c.value
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function Factory.get_counter(self: Factory) -> Counter {
        load_var self
        load_field .counter
        return
    }

    function main() -> int {
        alloc_instance Factory
        copy 0
        alloc_instance Counter
        copy 0
        load_const 10
        store_field .value
        store_field .counter
        call Factory.get_counter
        store_var c
        load_var c
        load_var c
        load_field .value
        load_const 5
        bin_op +
        store_field .value
        load_var c
        load_field .value
        return
    }

    function stream_Factory.get_counter(self: stream_Factory) -> Counter {
        load_var self
        load_field .counter
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(15)));
}

#[tokio::test]
async fn virtual_multiple_defs_preserve_side_effects() {
    let output = baml_test!(
        r#"
        function fail() -> int {
            assert(false);
            1
        }

        function main() -> int {
            let x = fail();
            x = 2;
            x
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function fail() -> int {
        load_const false
        assert
        load_const 1
        return
    }

    function main() -> int {
        call fail
        store_var x
        load_const 2
        store_var x
        load_var x
        return
    }
    ");

    assert_eq!(
        output.result,
        Err(bex_engine::EngineError::VmError(
            bex_vm::errors::VmError::RuntimeError(bex_vm::errors::RuntimeError::AssertionError)
        ))
    );
}
