//! Unified tests for instanceof operator and narrowing.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn instance_of_returns_true() {
    let output = baml_test!(
        "
        class StopTool {
            action \"stop\"
        }

        function main() -> bool {
            let t = StopTool { action: \"stop\" };
            t instanceof StopTool
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        alloc_instance StopTool
        copy 0
        load_const "stop"
        store_field .action
        load_const StopTool
        cmp_op instanceof
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn instance_of_returns_false() {
    let output = baml_test!(
        "
        class StopTool {
            action \"stop\"
        }

        class StartTool {
            action \"start\"
        }

        function main() -> bool {
            let t = StopTool { action: \"stop\" };
            t instanceof StartTool
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        alloc_instance StopTool
        copy 0
        load_const "stop"
        store_field .action
        load_const StartTool
        cmp_op instanceof
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn instanceof_narrowing_true_branch() {
    let output = baml_test!(
        "
        class Foo {
            field string
        }

        class Bar {
            other int
        }

        function main() -> string {
            let x = Foo { field: \"test value\" };
            if (x instanceof Foo) {
                return x.field;
            } else {
                return \"not foo\";
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance Foo
        copy 0
        load_const "test value"
        store_field .field
        store_var x
        load_var x
        load_const Foo
        cmp_op instanceof
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "not foo"
        return

      L1:
        load_var x
        load_field .0
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("test value".to_string()))
    );
}

#[tokio::test]
async fn instanceof_narrowing_false_branch() {
    let output = baml_test!(
        "
        class Foo {
            field string
        }

        class Bar {
            other int
        }

        function main() -> string {
            let x = Bar { other: 42 };
            if (x instanceof Foo) {
                return \"is foo\";
            } else {
                return \"not foo\";
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance Bar
        copy 0
        load_const 42
        store_field .other
        load_const Foo
        cmp_op instanceof
        pop_jump_if_false L0
        jump L1

      L0:
        load_const "not foo"
        return

      L1:
        load_const "is foo"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("not foo".to_string()))
    );
}

#[tokio::test]
async fn instanceof_chained_checks() {
    let output = baml_test!(
        "
        class A {
            a_field string
        }

        class B {
            b_field string
        }

        class C {
            c_field string
        }

        function main() -> string {
            let x = B { b_field: \"b value\" };
            if (x instanceof A) {
                return \"is A\";
            } else if (x instanceof B) {
                return x.b_field;
            } else if (x instanceof C) {
                return \"is C\";
            } else {
                return \"unknown\";
            }
        }
    "
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance B
        copy 0
        load_const "b value"
        store_field .b_field
        store_var x
        load_var x
        load_const A
        cmp_op instanceof
        pop_jump_if_false L0
        jump L5

      L0:
        load_var x
        load_const B
        cmp_op instanceof
        pop_jump_if_false L1
        jump L4

      L1:
        load_var x
        load_const C
        cmp_op instanceof
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "unknown"
        return

      L3:
        load_const "is C"
        return

      L4:
        load_var x
        load_field .0
        return

      L5:
        load_const "is A"
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("b value".to_string()))
    );
}
