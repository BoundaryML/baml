//! Unified tests for match types: instanceof/typed patterns, enums, classes,
//! guards (both typed and int literal guards), typetag switch, and mixed patterns
//! (literals + types + guards).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// Typed Pattern Tests (instanceof)
// ============================================================================

#[tokio::test]
#[ignore = "compiler2 todo"]
async fn match_typed_pattern_first_arm() {
    let output = baml_test!(
        r#"
        class Success {
            data string
        }

        class Failure {
            reason string
        }

        function main() -> string {
            let result = Success { data: "hello" };
            match (result: Success | Failure) {
                s: Success => "success: " + s.data,
                _: Failure => "failure",
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance Success
        copy 0
        load_const "hello"
        store_field .data
        store_var result
        load_var result
        load_const Success
        cmp_op instanceof
        pop_jump_if_false L0
        jump L1

      L0:
        load_var result
        load_const Failure
        cmp_op instanceof
        pop_jump_if_false L2
        load_const "failure"
        jump L2

      L1:
        load_const "success: "
        load_var result
        load_field .data
        bin_op +

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("success: hello".to_string()))
    );
}

#[tokio::test]
async fn match_typed_pattern_second_arm() {
    let output = baml_test!(
        r#"
        class Success {
            data string
        }

        class Failure {
            reason string
        }

        type Res = Success | Failure

        function main() -> string {
            let result: Res = Failure { reason: "error" };
            match (result) {
                s: Success => "success: " + s.data,
                f: Failure => "failure: " + f.reason,
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance Failure
        copy 0
        load_const "error"
        store_field .reason
        store_var result
        load_var result
        type_tag
        copy 0
        load_const type_tag:102
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L1

      L0:
        pop 1
        load_const "failure: "
        load_var result
        load_field .reason
        bin_op +
        jump L2

      L1:
        load_const "success: "
        load_var result
        load_field .data
        bin_op +

      L2:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("failure: error".to_string()))
    );
}

#[tokio::test]
#[ignore = "compiler2 todo"]
async fn match_typed_pattern_with_field_access() {
    let output = baml_test!(
        r#"
        class Point {
            x int
            y int
        }

        class Circle {
            radius int
        }

        type Shape = Point | Circle

        function main() -> int {
            let shape: Shape = Point { x: 10, y: 20 };
            match (shape) {
                p: Point => p.x + p.y,
                c: Circle => c.radius,
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> int {
        alloc_instance Point
        copy 0
        load_const 10
        store_field .x
        copy 0
        load_const 20
        store_field .y
        store_var shape
        load_var shape
        load_const Point
        cmp_op instanceof
        pop_jump_if_false L0
        jump L1

      L0:
        load_var shape
        load_const Circle
        cmp_op instanceof
        pop_jump_if_false L2
        load_var shape
        load_field .radius
        jump L2

      L1:
        load_var shape
        store_var p
        load_var p
        load_field .x
        load_var p
        load_field .y
        bin_op +

      L2:
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(30)));
}

#[tokio::test]
async fn match_typed_discard_patterns_typetag_switch_path() {
    let output = baml_test!(
        r#"
        function classify(x: int | string | bool | float) -> int {
            match (x) {
                _: int => 1,
                _: string => 2,
                _: bool => 3,
                _: float => 4,
            }
        }

        function main() -> int {
            classify(true)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @"
    function classify(x: int | string | bool | float) -> int {
        load_var x
        type_tag
        jump_table [L3, L2, L1, _, L0], default L5

      L0: type_tag:4
        load_const 4
        jump L4

      L1: type_tag:2
        load_const 3
        jump L4

      L2: type_tag:1
        load_const 2
        jump L4

      L3: type_tag:0
        load_const 1

      L4:
        return

      L5:
        unreachable
    }

    function main() -> int {
        load_const true
        call user.classify
        return
    }
    ");

    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

// ============================================================================
// Guard Tests
// ============================================================================

#[tokio::test]
async fn match_guard_true() {
    let output = baml_test!(
        r#"
        class Score {
            value int
        }

        function main() -> string {
            let s = Score { value: 95 };
            match (s) {
                x: Score if x.value >= 90 => "excellent",
                x: Score if x.value >= 70 => "good",
                _: Score => "needs work",
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance Score
        copy 0
        load_const 95
        store_field .value
        store_var s
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L0
        load_var s
        load_field .value
        load_const 90
        cmp_op >=
        pop_jump_if_false L0
        jump L3

      L0:
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L1
        load_var s
        load_field .value
        load_const 70
        cmp_op >=
        pop_jump_if_false L1
        jump L2

      L1:
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L4
        load_const "needs work"
        jump L4

      L2:
        load_const "good"
        jump L4

      L3:
        load_const "excellent"

      L4:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("excellent".to_string()))
    );
}

#[tokio::test]
async fn match_guard_fallthrough() {
    let output = baml_test!(
        r#"
        class Score {
            value int
        }

        function main() -> string {
            let s = Score { value: 75 };
            match (s) {
                x: Score if x.value >= 90 => "excellent",
                x: Score if x.value >= 70 => "good",
                _: Score => "needs work",
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance Score
        copy 0
        load_const 75
        store_field .value
        store_var s
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L0
        load_var s
        load_field .value
        load_const 90
        cmp_op >=
        pop_jump_if_false L0
        jump L3

      L0:
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L1
        load_var s
        load_field .value
        load_const 70
        cmp_op >=
        pop_jump_if_false L1
        jump L2

      L1:
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L4
        load_const "needs work"
        jump L4

      L2:
        load_const "good"
        jump L4

      L3:
        load_const "excellent"

      L4:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("good".to_string()))
    );
}

#[tokio::test]
async fn match_guard_all_fail() {
    let output = baml_test!(
        r#"
        class Score {
            value int
        }

        function main() -> string {
            let s = Score { value: 50 };
            match (s) {
                x: Score if x.value >= 90 => "excellent",
                x: Score if x.value >= 70 => "good",
                _: Score => "needs work",
            }
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        alloc_instance Score
        copy 0
        load_const 50
        store_field .value
        store_var s
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L0
        load_var s
        load_field .value
        load_const 90
        cmp_op >=
        pop_jump_if_false L0
        jump L3

      L0:
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L1
        load_var s
        load_field .value
        load_const 70
        cmp_op >=
        pop_jump_if_false L1
        jump L2

      L1:
        load_var s
        load_const Score
        cmp_op instanceof
        pop_jump_if_false L4
        load_const "needs work"
        jump L4

      L2:
        load_const "good"
        jump L4

      L3:
        load_const "excellent"

      L4:
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("needs work".to_string()))
    );
}

// ============================================================================
// Guards with Integer Literals (should prevent switch optimization)
// ============================================================================

#[tokio::test]
async fn match_guarded_int_literal_guard_true() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int, flag: bool) -> string {
                match (x) {
                    1 if flag => "one with flag",
                    1 => "one without flag",
                    2 => "two",
                    _ => "other"
                }
            }
            function main() -> string {
                classify(1, true)
            }
        "#,
        entry: "main",
        args: {},
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int, flag: bool) -> string {
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        load_var flag
        pop_jump_if_false L0
        jump L5

      L0:
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_var x
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "other"
        jump L6

      L3:
        load_const "two"
        jump L6

      L4:
        load_const "one without flag"
        jump L6

      L5:
        load_const "one with flag"

      L6:
        return
    }

    function main() -> string {
        load_const 1
        load_const true
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one with flag".to_string()))
    );
}

#[tokio::test]
async fn match_guarded_int_literal_guard_false() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int, flag: bool) -> string {
                match (x) {
                    1 if flag => "one with flag",
                    1 => "one without flag",
                    2 => "two",
                    _ => "other"
                }
            }
            function main() -> string {
                classify(1, false)
            }
        "#,
        entry: "main",
        args: {},
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int, flag: bool) -> string {
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L0
        load_var flag
        pop_jump_if_false L0
        jump L5

      L0:
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        jump L4

      L1:
        load_var x
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "other"
        jump L6

      L3:
        load_const "two"
        jump L6

      L4:
        load_const "one without flag"
        jump L6

      L5:
        load_const "one with flag"

      L6:
        return
    }

    function main() -> string {
        load_const 1
        load_const false
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one without flag".to_string()))
    );
}

#[tokio::test]
async fn match_all_arms_guarded_all_fail() {
    let output = baml_test! {
        baml: r#"
            function classify(x: int, flag: bool) -> string {
                match (x) {
                    0 if flag => "zero",
                    1 if flag => "one",
                    2 if flag => "two",
                    _ => "fallback"
                }
            }
            function main() -> string {
                classify(1, false)
            }
        "#,
        entry: "main",
        args: {},
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int, flag: bool) -> string {
        load_var x
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        load_var flag
        pop_jump_if_false L0
        jump L5

      L0:
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        load_var flag
        pop_jump_if_false L1
        jump L4

      L1:
        load_var x
        load_const 2
        cmp_op ==
        pop_jump_if_false L2
        load_var flag
        pop_jump_if_false L2
        jump L3

      L2:
        load_const "fallback"
        jump L6

      L3:
        load_const "two"
        jump L6

      L4:
        load_const "one"
        jump L6

      L5:
        load_const "zero"

      L6:
        return
    }

    function main() -> string {
        load_const 1
        load_const false
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("fallback".to_string()))
    );
}

// ============================================================================
// Mixed Patterns: Literals + Types + Guards
// ============================================================================

#[tokio::test]
async fn match_mixed_literal_typed_guard() {
    let output = baml_test!(
        r#"
        function classify(x: int, flag: bool) -> string {
            match (x) {
                0 => "zero",
                1 if flag => "one with flag",
                n: int => "other int"
            }
        }
        function main() -> string {
            classify(1, true)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int, flag: bool) -> string {
        load_var x
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        load_var flag
        pop_jump_if_false L1
        jump L2

      L1:
        load_var x
        type_tag
        load_const 0
        cmp_op ==
        pop_jump_if_false L4
        load_const "other int"
        jump L4

      L2:
        load_const "one with flag"
        jump L4

      L3:
        load_const "zero"

      L4:
        return
    }

    function main() -> string {
        load_const 1
        load_const true
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one with flag".to_string()))
    );
}

#[tokio::test]
async fn match_mixed_literal_typed_guard_fallthrough() {
    let output = baml_test!(
        r#"
        function classify(x: int, flag: bool) -> string {
            match (x) {
                0 => "zero",
                1 if flag => "one with flag",
                n: int => "other int"
            }
        }
        function main() -> string {
            classify(1, false)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: int, flag: bool) -> string {
        load_var x
        load_const 0
        cmp_op ==
        pop_jump_if_false L0
        jump L3

      L0:
        load_var x
        load_const 1
        cmp_op ==
        pop_jump_if_false L1
        load_var flag
        pop_jump_if_false L1
        jump L2

      L1:
        load_var x
        type_tag
        load_const 0
        cmp_op ==
        pop_jump_if_false L4
        load_const "other int"
        jump L4

      L2:
        load_const "one with flag"
        jump L4

      L3:
        load_const "zero"

      L4:
        return
    }

    function main() -> string {
        load_const 1
        load_const false
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("other int".to_string()))
    );
}

#[tokio::test]
async fn match_guard_on_typed_pattern_field_access() {
    let output = baml_test!(
        r#"
        class Success { data string }
        class Failure { reason string }

        function classify(result: Success | Failure) -> string {
            match (result) {
                s: Success if s.data != "" => "success with data",
                s: Success => "empty success",
                f: Failure => "failure"
            }
        }
        function main() -> string {
            let r = Success { data: "hello" };
            classify(r)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(result: Success | Failure) -> string {
        load_var result
        load_const Success
        cmp_op instanceof
        pop_jump_if_false L0
        load_var result
        load_field .data
        load_const ""
        cmp_op !=
        pop_jump_if_false L0
        jump L3

      L0:
        load_var result
        load_const Success
        cmp_op instanceof
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        load_const Failure
        cmp_op instanceof
        pop_jump_if_false L4
        load_const "failure"
        jump L4

      L2:
        load_const "empty success"
        jump L4

      L3:
        load_const "success with data"

      L4:
        return
    }

    function main() -> string {
        alloc_instance Success
        copy 0
        load_const "hello"
        store_field .data
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("success with data".to_string()))
    );
}

#[tokio::test]
async fn match_guard_on_typed_pattern_field_access_fails() {
    let output = baml_test!(
        r#"
        class Success { data string }
        class Failure { reason string }

        function classify(result: Success | Failure) -> string {
            match (result) {
                s: Success if s.data != "" => "success with data",
                s: Success => "empty success",
                f: Failure => "failure"
            }
        }
        function main() -> string {
            let r = Success { data: "" };
            classify(r)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(result: Success | Failure) -> string {
        load_var result
        load_const Success
        cmp_op instanceof
        pop_jump_if_false L0
        load_var result
        load_field .data
        load_const ""
        cmp_op !=
        pop_jump_if_false L0
        jump L3

      L0:
        load_var result
        load_const Success
        cmp_op instanceof
        pop_jump_if_false L1
        jump L2

      L1:
        load_var result
        load_const Failure
        cmp_op instanceof
        pop_jump_if_false L4
        load_const "failure"
        jump L4

      L2:
        load_const "empty success"
        jump L4

      L3:
        load_const "success with data"

      L4:
        return
    }

    function main() -> string {
        alloc_instance Success
        copy 0
        load_const ""
        store_field .data
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("empty success".to_string()))
    );
}

// ============================================================================
// Enum Variant Patterns
// ============================================================================

#[tokio::test]
async fn match_enum_variant_first() {
    let output = baml_test!(
        r#"
        enum Status {
            Active
            Inactive
            Pending
        }

        function classify(s: Status) -> string {
            match (s) {
                Status.Active => "active",
                Status.Inactive => "inactive",
                Status.Pending => "pending"
            }
        }
        function main() -> string {
            classify(Status.Active)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: Status) -> string {
        load_var s
        discriminant
        copy 0
        load_const Status.Active
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const Status.Inactive
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "pending"
        jump L4

      L2:
        load_const "inactive"
        jump L4

      L3:
        load_const "active"

      L4:
        return
    }

    function main() -> string {
        load_const user.Status.Active
        alloc_variant user.Status
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("active".to_string()))
    );
}

#[tokio::test]
async fn match_enum_variant_last() {
    let output = baml_test!(
        r#"
        enum Status {
            Active
            Inactive
            Pending
        }

        function classify(s: Status) -> string {
            match (s) {
                Status.Active => "active",
                Status.Inactive => "inactive",
                Status.Pending => "pending"
            }
        }
        function main() -> string {
            classify(Status.Pending)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: Status) -> string {
        load_var s
        discriminant
        copy 0
        load_const Status.Active
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const Status.Inactive
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "pending"
        jump L4

      L2:
        load_const "inactive"
        jump L4

      L3:
        load_const "active"

      L4:
        return
    }

    function main() -> string {
        load_const user.Status.Pending
        alloc_variant user.Status
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("pending".to_string()))
    );
}

// ============================================================================
// Non-Exhaustive Enum Tests (with wildcard)
// ============================================================================

#[tokio::test]
async fn match_enum_variant_with_wildcard() {
    let output = baml_test!(
        r#"
        enum Status {
            Active
            Inactive
            Pending
        }

        function classify(s: Status) -> string {
            match (s) {
                Status.Active => "active",
                Status.Inactive => "inactive",
                _ => "other"
            }
        }
        function main() -> string {
            classify(Status.Pending)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: Status) -> string {
        load_var s
        discriminant
        copy 0
        load_const Status.Active
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const Status.Inactive
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "other"
        jump L4

      L2:
        load_const "inactive"
        jump L4

      L3:
        load_const "active"

      L4:
        return
    }

    function main() -> string {
        load_const user.Status.Pending
        alloc_variant user.Status
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("other".to_string()))
    );
}

#[tokio::test]
async fn match_enum_variant_with_wildcard_matched() {
    let output = baml_test!(
        r#"
        enum Status {
            Active
            Inactive
            Pending
        }

        function classify(s: Status) -> string {
            match (s) {
                Status.Active => "active",
                Status.Inactive => "inactive",
                _ => "other"
            }
        }
        function main() -> string {
            classify(Status.Active)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(s: Status) -> string {
        load_var s
        discriminant
        copy 0
        load_const Status.Active
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const Status.Inactive
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "other"
        jump L4

      L2:
        load_const "inactive"
        jump L4

      L3:
        load_const "active"

      L4:
        return
    }

    function main() -> string {
        load_const user.Status.Active
        alloc_variant user.Status
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("active".to_string()))
    );
}

/// Enum variant patterns with 4+ arms should use Discriminant + JumpTable.
#[tokio::test]
async fn match_enum_four_variants_jump_table() {
    let output = baml_test! {
        baml: r#"
            enum Direction {
                North
                East
                South
                West
            }

            function compass(d: Direction) -> string {
                match (d) {
                    Direction.North => "N",
                    Direction.East => "E",
                    Direction.South => "S",
                    Direction.West => "W"
                }
            }

            function main() -> string {
                compass(Direction.South)
            }
        "#,
        entry: "main",
        args: {},
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function compass(d: Direction) -> string {
        load_var d
        discriminant
        jump_table [L3, L2, L1, L0], default L5

      L0: Direction.West
        load_const "W"
        jump L4

      L1: Direction.South
        load_const "S"
        jump L4

      L2: Direction.East
        load_const "E"
        jump L4

      L3: Direction.North
        load_const "N"

      L4:
        return

      L5:
        unreachable
    }

    function main() -> string {
        load_const user.Direction.South
        alloc_variant user.Direction
        call user.compass
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::String("S".to_string())));
}

// ============================================================================
// Exhaustive and Non-Exhaustive Class Type Tests
// ============================================================================

#[tokio::test]
async fn match_class_types_exhaustive_first() {
    let output = baml_test!(
        r#"
        class Cat { name string }
        class Dog { name string }
        class Bird { name string }

        function classify(animal: Cat | Dog | Bird) -> string {
            match (animal) {
                c: Cat => "cat: " + c.name,
                d: Dog => "dog: " + d.name,
                b: Bird => "bird: " + b.name
            }
        }
        function main() -> string {
            classify(Cat { name: "Whiskers" })
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(animal: Cat | Dog | Bird) -> string {
        load_var animal
        type_tag
        copy 0
        load_const type_tag:103
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const type_tag:104
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "bird: "
        load_var animal
        load_field .name
        bin_op +
        jump L4

      L2:
        load_const "dog: "
        load_var animal
        load_field .name
        bin_op +
        jump L4

      L3:
        load_const "cat: "
        load_var animal
        load_field .name
        bin_op +

      L4:
        return
    }

    function main() -> string {
        alloc_instance Cat
        copy 0
        load_const "Whiskers"
        store_field .name
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("cat: Whiskers".to_string()))
    );
}

#[tokio::test]
async fn match_class_types_exhaustive_last() {
    let output = baml_test!(
        r#"
        class Cat { name string }
        class Dog { name string }
        class Bird { name string }

        function classify(animal: Cat | Dog | Bird) -> string {
            match (animal) {
                c: Cat => "cat: " + c.name,
                d: Dog => "dog: " + d.name,
                b: Bird => "bird: " + b.name
            }
        }
        function main() -> string {
            classify(Bird { name: "Tweety" })
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(animal: Cat | Dog | Bird) -> string {
        load_var animal
        type_tag
        copy 0
        load_const type_tag:103
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const type_tag:104
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "bird: "
        load_var animal
        load_field .name
        bin_op +
        jump L4

      L2:
        load_const "dog: "
        load_var animal
        load_field .name
        bin_op +
        jump L4

      L3:
        load_const "cat: "
        load_var animal
        load_field .name
        bin_op +

      L4:
        return
    }

    function main() -> string {
        alloc_instance Bird
        copy 0
        load_const "Tweety"
        store_field .name
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("bird: Tweety".to_string()))
    );
}

#[tokio::test]
async fn match_class_types_non_exhaustive_wildcard() {
    let output = baml_test!(
        r#"
        class Cat { name string }
        class Dog { name string }
        class Bird { name string }

        function classify(animal: Cat | Dog | Bird) -> string {
            match (animal) {
                c: Cat => "cat: " + c.name,
                d: Dog => "dog: " + d.name,
                _ => "other"
            }
        }
        function main() -> string {
            classify(Bird { name: "Tweety" })
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(animal: Cat | Dog | Bird) -> string {
        load_var animal
        type_tag
        copy 0
        load_const type_tag:103
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const type_tag:104
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "other"
        jump L4

      L2:
        load_const "dog: "
        load_var animal
        load_field .name
        bin_op +
        jump L4

      L3:
        load_const "cat: "
        load_var animal
        load_field .name
        bin_op +

      L4:
        return
    }

    function main() -> string {
        alloc_instance Bird
        copy 0
        load_const "Tweety"
        store_field .name
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("other".to_string()))
    );
}

#[tokio::test]
async fn match_class_types_non_exhaustive_matched() {
    let output = baml_test!(
        r#"
        class Cat { name string }
        class Dog { name string }
        class Bird { name string }

        function classify(animal: Cat | Dog | Bird) -> string {
            match (animal) {
                c: Cat => "cat: " + c.name,
                d: Dog => "dog: " + d.name,
                _ => "other"
            }
        }
        function main() -> string {
            classify(Dog { name: "Rex" })
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(animal: Cat | Dog | Bird) -> string {
        load_var animal
        type_tag
        copy 0
        load_const type_tag:103
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L3

      L0:
        copy 0
        load_const type_tag:104
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L2

      L1:
        pop 1
        load_const "other"
        jump L4

      L2:
        load_const "dog: "
        load_var animal
        load_field .name
        bin_op +
        jump L4

      L3:
        load_const "cat: "
        load_var animal
        load_field .name
        bin_op +

      L4:
        return
    }

    function main() -> string {
        alloc_instance Dog
        copy 0
        load_const "Rex"
        store_field .name
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("dog: Rex".to_string()))
    );
}

// ============================================================================
// TypeTag Switch Tests (Union Types with Typed Patterns)
// ============================================================================

/// Union type with 4+ typed primitive patterns should use TypeTag + JumpTable.
#[tokio::test]
async fn match_union_type_four_patterns_type_tag() {
    let output = baml_test! {
        baml: r#"
            function identify(x: int | string | bool | float) -> string {
                match (x) {
                    n: int => "integer",
                    s: string => "text",
                    b: bool => "boolean",
                    f: float => "decimal"
                }
            }
        "#,
        entry: "identify",
        args: { "x" => BexExternalValue::String("hello".to_string()) },
    };

    insta::assert_snapshot!(output.bytecode, @r#"
    function identify(x: int | string | bool | float) -> string {
        load_var x
        type_tag
        jump_table [L3, L2, L1, _, L0], default L5

      L0: type_tag:4
        load_const "decimal"
        jump L4

      L1: type_tag:2
        load_const "boolean"
        jump L4

      L2: type_tag:1
        load_const "text"
        jump L4

      L3: type_tag:0
        load_const "integer"

      L4:
        return

      L5:
        unreachable
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("text".to_string()))
    );
}

// ============================================================================
// Multiple Typed Patterns with Guards
// ============================================================================

#[tokio::test]
async fn match_multiple_typed_patterns_with_guards() {
    let output = baml_test!(
        r#"
        class Success { code int }
        class Failure { code int }

        function classify(result: Success | Failure, strict: bool) -> string {
            match (result) {
                s: Success if s.code > 200 => "redirect",
                s: Success if strict => "strict success",
                s: Success => "success",
                f: Failure => "failure"
            }
        }
        function main() -> string {
            let r = Success { code: 301 };
            classify(r, false)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(result: Success | Failure, strict: bool) -> string {
        load_var result
        load_const Success
        cmp_op instanceof
        pop_jump_if_false L0
        load_var result
        load_field .code
        load_const 200
        cmp_op >
        pop_jump_if_false L0
        jump L5

      L0:
        load_var result
        load_const Success
        cmp_op instanceof
        pop_jump_if_false L1
        load_var strict
        pop_jump_if_false L1
        jump L4

      L1:
        load_var result
        load_const Success
        cmp_op instanceof
        pop_jump_if_false L2
        jump L3

      L2:
        load_var result
        load_const Failure
        cmp_op instanceof
        pop_jump_if_false L6
        load_const "failure"
        jump L6

      L3:
        load_const "success"
        jump L6

      L4:
        load_const "strict success"
        jump L6

      L5:
        load_const "redirect"

      L6:
        return
    }

    function main() -> string {
        alloc_instance Success
        copy 0
        load_const 301
        store_field .code
        load_const false
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("redirect".to_string()))
    );
}

// ============================================================================
// Step 1.6 — Explicit TypeTag dispatch shape verification
// ============================================================================

/// 4+ sequentially-declared class types → dense type tags → JumpTable dispatch.
#[tokio::test]
async fn match_class_type_tag_jump_table() {
    let output = baml_test!(
        r#"
        class Cat  { name string }
        class Dog  { name string }
        class Bird { name string }
        class Fish { name string }

        function describe(animal: Cat | Dog | Bird | Fish) -> string {
            match (animal) {
                c: Cat  => "cat",
                d: Dog  => "dog",
                b: Bird => "bird",
                f: Fish => "fish"
            }
        }
        function main() -> string {
            describe(Dog { name: "Rex" })
        }
    "#
    );

    // Bytecode must use type_tag + jump_table, not instanceof chains.
    assert!(
        output.bytecode.contains("type_tag"),
        "expected type_tag instruction in bytecode:\n{}",
        output.bytecode
    );
    assert!(
        output.bytecode.contains("jump_table"),
        "expected jump_table instruction in bytecode:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("instanceof"),
        "unexpected instanceof instruction in bytecode:\n{}",
        output.bytecode
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function describe(animal: Cat | Dog | Bird | Fish) -> string {
        load_var animal
        type_tag
        jump_table [L1, L0, _, L3, L2], default L5

      L0: type_tag:104
        load_const "fish"
        jump L4

      L1: type_tag:103
        load_const "bird"
        jump L4

      L2: type_tag:107
        load_const "dog"
        jump L4

      L3: type_tag:106
        load_const "cat"

      L4:
        return

      L5:
        unreachable
    }

    function main() -> string {
        alloc_instance Dog
        copy 0
        load_const "Rex"
        store_field .name
        call user.describe
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("dog".to_string()))
    );
}

/// <4 class types → type_tag + if_else_chain dispatch (not instanceof chains).
#[tokio::test]
async fn match_class_type_tag_if_else() {
    let output = baml_test!(
        r#"
        class Cat { name string }
        class Dog { name string }

        function describe(animal: Cat | Dog) -> string {
            match (animal) {
                c: Cat => "cat",
                d: Dog => "dog"
            }
        }
        function main() -> string {
            describe(Cat { name: "Whiskers" })
        }
    "#
    );

    // Bytecode must use type_tag + if_else comparison (copy/load_const type_tag/cmp_op),
    // not instanceof chains or jump_table.
    assert!(
        output.bytecode.contains("type_tag"),
        "expected type_tag instruction in bytecode:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("instanceof"),
        "unexpected instanceof instruction in bytecode:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("jump_table"),
        "expected if_else dispatch (no jump_table for <4 arms):\n{}",
        output.bytecode
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function describe(animal: Cat | Dog) -> string {
        load_var animal
        type_tag
        copy 0
        load_const type_tag:101
        cmp_op ==
        pop_jump_if_false L0
        pop 1
        jump L1

      L0:
        pop 1
        load_const "dog"
        jump L2

      L1:
        load_const "cat"

      L2:
        return
    }

    function main() -> string {
        alloc_instance Cat
        copy 0
        load_const "Whiskers"
        store_field .name
        call user.describe
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("cat".to_string()))
    );
}

/// Mixed class + primitive union with 4+ typed arms → type_tag + switch.
#[tokio::test]
async fn match_mixed_class_primitive_type_tag_switch() {
    let output = baml_test!(
        r#"
        class MyClass { value int }

        function classify(x: MyClass | int | string | bool) -> string {
            match (x) {
                c: MyClass => "class",
                n: int     => "int",
                s: string  => "string",
                b: bool    => "bool"
            }
        }
        function main() -> string {
            classify(MyClass { value: 42 })
        }
    "#
    );

    // Bytecode must use type_tag and some form of switch dispatch (jump_table or
    // if_else chain), not instanceof chains.
    assert!(
        output.bytecode.contains("type_tag"),
        "expected type_tag instruction in bytecode:\n{}",
        output.bytecode
    );
    assert!(
        !output.bytecode.contains("instanceof"),
        "unexpected instanceof instruction in bytecode:\n{}",
        output.bytecode
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function classify(x: MyClass | int | string | bool) -> string {
        load_var x
        type_tag
        copy 0
        load_const type_tag:2
        cmp_op ==
        pop_jump_if_false L0
        pop 1

      L0:
        copy 0
        load_const type_tag:2
        cmp_op <
        pop_jump_if_false L2
        copy 0
        load_const type_tag:0
        cmp_op ==
        pop_jump_if_false L1
        pop 1
        jump L5

      L1:
        copy 0
        load_const type_tag:1
        cmp_op ==
        pop_jump_if_false L2
        pop 1
        jump L4

      L2:
        copy 0
        load_const type_tag:101
        cmp_op ==
        pop_jump_if_false L3
        pop 1
        jump L6

      L3:
        pop 1
        jump L8
        load_const "bool"
        jump L7

      L4:
        load_const "string"
        jump L7

      L5:
        load_const "int"
        jump L7

      L6:
        load_const "class"

      L7:
        return

      L8:
        unreachable
    }

    function main() -> string {
        alloc_instance MyClass
        copy 0
        load_const 42
        store_field .value
        call user.classify
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("class".to_string()))
    );
}
